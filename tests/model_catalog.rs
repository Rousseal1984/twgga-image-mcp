use serde_json::json;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use twgga_devtools::{
    Result,
    mock::{HttpFixture, MockTwgga, respond_json},
    stdio::Session,
};

#[test]
fn manual_directory_never_fetches_upstream_and_removes_grok_fields() -> Result<()> {
    let root = tempfile::tempdir()?;
    let gets = Arc::new(AtomicUsize::new(0));
    let count = gets.clone();
    let api = HttpFixture::start(move |request| {
        count.fetch_add(1, Ordering::SeqCst);
        respond_json(request, 401, json!({"error":"must not query catalog"}), &[])
    })?;
    let mut server = Session::new(
        Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp")),
        root.path(),
        &[("TWGGA_BASEURL", &api.url), ("TWGGA_API_KEY", "fixture")],
    )?;
    server.initialize()?;
    for _ in 0..3 {
        let info = server.call("server_info", json!({}))?;
        assert_eq!(info["available_models"], json!(["gpt-image-2"]));
        assert_eq!(info["model_catalog"]["source"], "manual");
        assert_eq!(info["model_catalog"]["refresh_status"], "not_applicable");
        assert!(info["model_catalog"]["last_success_unix"].is_null());
        assert!(!info.to_string().to_lowercase().contains("grok"));
    }
    assert_eq!(gets.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn unknown_manual_models_are_rejected_without_echoing_credentials() -> Result<()> {
    let key = "sk-fixture-secret";
    let root = tempfile::tempdir()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let api = HttpFixture::start(move |request| {
        count.fetch_add(1, Ordering::SeqCst);
        respond_json(request, 500, json!({}), &[])
    })?;
    let mut server = Session::new(
        Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp")),
        root.path(),
        &[("TWGGA_BASEURL", &api.url), ("TWGGA_API_KEY", key)],
    )?;
    server.initialize()?;
    for model in [
        "gpt-image-2.7-new".to_owned(),
        format!("gpt-image-2.5-{key}"),
    ] {
        let result = server.call("image_generate", json!({"prompt":"x", "model":model}))?;
        assert_eq!(result["ok"], false);
        assert!(!result.to_string().contains(key));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

// 上游 micu 在 ≥2K 时把 gpt-image-2 自动切到 gpt-image-2-openai；本站没有那条线路，
// 切过去客户拿到的是「模型未配置价格」。这里守住「不切」，以及那个模型名在本地就被拒。
#[test]
fn single_route_never_switches_models_by_size() -> Result<()> {
    let root = tempfile::tempdir()?;
    let api = MockTwgga::new()?;
    let mut server = Session::new(
        Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp")),
        root.path(),
        &api.env(),
    )?;
    server.initialize()?;
    for size in ["1024x1024", "2048x1152", "3840x2160"] {
        let result = server.call(
            "image_generate",
            json!({"prompt":"x", "model":"gpt-image-2","size":size}),
        )?;
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["model"], "gpt-image-2");
        assert_eq!(
            api.requests().last().unwrap().fields["model"],
            "gpt-image-2"
        );
    }
    let sent = api.requests().len();
    let rejected = server.call(
        "image_generate",
        json!({"prompt":"x", "model":"gpt-image-2-openai"}),
    )?;
    assert_eq!(rejected["ok"], false, "{rejected}");
    assert_eq!(api.requests().len(), sent);
    Ok(())
}
