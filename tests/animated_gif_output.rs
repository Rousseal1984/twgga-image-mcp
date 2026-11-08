use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{
    AnimationDecoder, Delay, Frame, Rgba, RgbaImage,
    codecs::gif::{GifDecoder, GifEncoder},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Cursor,
    path::Path,
    sync::{Arc, Mutex},
};
use twgga_devtools::{
    Result,
    mock::{HttpFixture, MockTwgga, body, header, multipart, png, respond_json},
    stdio::Session,
};

// 本站只有 gpt-image-2，不开放 GIF 请求（上游 micu 的 GIF 只在 gpt-image-2.6 上有）。
// 但服务端若真回了一个 GIF，逐帧校验与解压炸弹防护照样要生效，这些测试守的是那一层。
const FRAMES: usize = 3;
/// 上游 micu 会在尺寸不符时建议改用这条精确像素线路；本站没有它，任何提示都不能提到。
const ABSENT_ROUTE: &str = "gpt-image-2-openai";

fn animated_gif() -> Result<Vec<u8>> {
    let mut encoded = Vec::new();
    let mut encoder = GifEncoder::new(&mut encoded);
    for seed in 0..FRAMES as u8 {
        let image = RgbaImage::from_pixel(48, 32, Rgba([seed * 100, 90, 160, 255]));
        encoder.encode_frame(Frame::from_parts(
            image,
            0,
            0,
            Delay::from_numer_denom_ms(100, 1),
        ))?;
    }
    drop(encoder);
    Ok(encoded)
}

/// Serves `gif` for every POST and records each JSON or multipart form field set.
fn gif_upstream(gif: &[u8]) -> Result<(HttpFixture, Arc<Mutex<Vec<Value>>>)> {
    let payload = STANDARD.encode(gif);
    let fields = Arc::new(Mutex::new(Vec::new()));
    let captured = fields.clone();
    let api = HttpFixture::start(move |mut request| {
        let content_type = header(&request, "Content-Type");
        let bytes = body(&mut request)?;
        let form = if content_type.starts_with("multipart/") {
            multipart(&content_type, &bytes)?.0
        } else {
            serde_json::from_slice(&bytes)?
        };
        captured.lock().unwrap().push(form);
        respond_json(request, 200, json!({"data":[{"b64_json":payload}]}), &[])
    })?;
    Ok((api, fields))
}

fn start(root: &Path, url: &str) -> Result<Session> {
    let mut server = Session::new(
        Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp")),
        root,
        &[("TWGGA_BASEURL", url), ("TWGGA_API_KEY", "fixture")],
    )?;
    server.initialize()?;
    Ok(server)
}

#[test]
fn unexpected_gif_output_keeps_every_frame_and_reports_format() -> Result<()> {
    let gif = animated_gif()?;
    let root = tempfile::tempdir()?;
    let (api, fields) = gif_upstream(&gif)?;
    let mut server = start(root.path(), &api.url)?;

    let generated = server.call(
        "image_generate",
        json!({"prompt":"looping animation", "size":"1024x1024"}),
    )?;
    assert_eq!(generated["ok"], true, "{generated}");
    assert!(fields.lock().unwrap()[0].get("output_format").is_none());
    let saved = &generated["saved"][0];
    assert_eq!(saved["format"], "gif");
    assert_eq!(saved["frame_count"], FRAMES);
    assert_eq!(saved["actual_size"], "48x32");
    assert_eq!(generated["size_honored"], false);
    let notes = generated["notes"].to_string();
    assert!(notes.contains("实际 48×32"), "{notes}");
    assert!(notes.contains("GIF"), "{notes}");
    assert!(!notes.contains(ABSENT_ROUTE), "{notes}");
    let path = Path::new(saved["path"].as_str().unwrap());
    assert_eq!(path.extension().unwrap(), "gif");
    let stored = fs::read(path)?;
    assert_eq!(stored, gif);
    let frames = GifDecoder::new(Cursor::new(stored))?
        .into_frames()
        .collect_frames()?;
    assert_eq!(frames.len(), FRAMES);
    Ok(())
}

#[test]
fn size_advice_never_names_a_route_this_service_lacks() -> Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("a.png");
    fs::write(&source, png(32, 24, 1)?)?;

    // Static results get this service's own advice, whatever format was requested.
    let (api, _) = gif_upstream(&png(32, 24, 1)?)?;
    let mut server = start(root.path(), &api.url)?;
    for (tool, args) in [
        ("image_generate", json!({"prompt":"x"})),
        (
            "image_generate",
            json!({"prompt":"x","output_format":"webp"}),
        ),
        (
            "image_edit",
            json!({"prompt":"x","image_path":source,"output_format":"jpeg"}),
        ),
    ] {
        let result = server.call(tool, args)?;
        assert_eq!(result["ok"], true, "{result}");
        let notes = result["notes"].to_string();
        assert!(notes.contains("实际 32×24"), "{notes}");
        assert!(notes.contains("saved.actual_size"), "{notes}");
        assert!(!notes.contains(ABSENT_ROUTE), "{notes}");
    }

    // Multi-reference has no output_format, so only the returned file can show a GIF.
    let (api, _) = gif_upstream(&animated_gif()?)?;
    let mut server = start(root.path(), &api.url)?;
    let result = server.call(
        "image_multi_reference",
        json!({"prompt":"x","image_paths":[source,source]}),
    )?;
    assert_eq!(result["ok"], true, "{result}");
    let notes = result["notes"].to_string();
    assert!(notes.contains("实际 48×32"), "{notes}");
    assert!(!notes.contains(ABSENT_ROUTE), "{notes}");
    Ok(())
}

#[test]
fn gif_session_revision_points_to_a_working_recovery_path() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (api, fields) = gif_upstream(&animated_gif()?)?;
    let mut server = start(root.path(), &api.url)?;
    let generated = server.call(
        "image_generate",
        json!({"prompt":"looping animation", "session_id":"new"}),
    )?;
    assert_eq!(generated["ok"], true, "{generated}");
    let id = generated["session"]["id"].clone();
    let source = root.path().join("converted.png");
    fs::write(&source, png(32, 24, 1)?)?;

    for args in [
        json!({"prompt":"recolour", "session_id":id}),
        json!({"prompt":"recolour", "session_id":id, "image_path":source}),
    ] {
        let edited = server.call("image_edit", args)?;
        assert_eq!(edited["ok"], false, "{edited}");
        assert!(edited.to_string().contains("session_id=new"), "{edited}");
    }
    assert_eq!(fields.lock().unwrap().len(), 1);

    let recovered = server.call(
        "image_edit",
        json!({"prompt":"recolour", "session_id":"new", "image_path":source}),
    )?;
    assert_eq!(recovered["ok"], true, "{recovered}");
    Ok(())
}

#[test]
fn output_format_reaches_both_endpoints_and_stays_absent_when_omitted() -> Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("a.png");
    fs::write(&source, png(32, 24, 1)?)?;
    let api = MockTwgga::new()?;
    let mut server = Session::new(
        Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp")),
        root.path(),
        &api.env(),
    )?;
    server.initialize()?;

    server.call("image_generate", json!({"prompt":"x"}))?;
    server.call("image_edit", json!({"prompt":"x","image_path":source}))?;
    let generated = server.call(
        "image_generate",
        json!({"prompt":"x","output_format":"jpeg"}),
    )?;
    let edited = server.call(
        "image_edit",
        json!({"prompt":"x","image_path":source,"output_format":"jpeg"}),
    )?;

    let session = server.call("image_generate", json!({"prompt":"x","session_id":"new"}))?;
    let continued = server.call(
        "image_generate",
        json!({"prompt":"y","session_id":session["session"]["id"],"output_format":"webp"}),
    )?;
    assert_eq!(continued["ok"], true, "{continued}");

    let requests = api.requests();
    assert_eq!(requests.len(), 6);
    assert_eq!(requests[5].path, "/v1/images/edits");
    assert_eq!(requests[5].fields["output_format"], "webp");
    assert!(requests[0].fields.get("output_format").is_none());
    assert!(requests[1].fields.get("output_format").is_none());
    assert_eq!(requests[2].fields["output_format"], "jpeg");
    assert_eq!(requests[3].fields["output_format"], "jpeg");
    // The fixture answers with PNG, so both calls must report the mismatch without failing.
    for result in [&generated, &edited] {
        assert_eq!(result["ok"], true, "{result}");
        assert!(
            result["notes"].to_string().contains("output_format=jpeg"),
            "{result}"
        );
    }
    Ok(())
}

#[test]
fn gif_requests_and_absent_models_fail_locally_before_any_billable_post() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (api, fields) = gif_upstream(&animated_gif()?)?;
    let mut server = start(root.path(), &api.url)?;
    for tool in ["image_generate", "image_edit"] {
        let source = root.path().join("a.png");
        fs::write(&source, png(32, 24, 1)?)?;
        let args = if tool == "image_generate" {
            json!({"prompt":"x","output_format":"gif"})
        } else {
            json!({"prompt":"x","image_path":source,"output_format":"gif"})
        };
        let result = server.call(tool, args)?;
        assert_eq!(result["ok"], false, "{result}");
        assert!(result.to_string().contains("output_format"), "{result}");
    }
    for model in [
        ABSENT_ROUTE,
        "gpt-image-2.6-flare",
        "gpt-image-2.5-sunburst",
    ] {
        let result = server.call(
            "image_generate",
            json!({"prompt":"x","model":model,"output_format":"gif"}),
        )?;
        assert_eq!(result["ok"], false, "{result}");
        assert!(result.to_string().contains("gpt-image-2"), "{result}");
    }
    assert!(fields.lock().unwrap().is_empty());
    Ok(())
}
