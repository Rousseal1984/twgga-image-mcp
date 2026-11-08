use serde_json::{Value, json};
use std::{fs, path::Path};
use twgga_devtools::{
    Result,
    mock::{FIXTURE_KEY, MockTwgga, sha256},
    stdio::{Session, latest_meta, tool_value},
};

fn server(root: &Path, api: &MockTwgga) -> Result<Session> {
    let mut server = Session::new(
        Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp")),
        root,
        &api.env(),
    )?;
    server.initialize()?;
    Ok(server)
}

#[test]
fn generation_and_edit_memory_survive_restart_and_branch_from_the_selected_image() -> Result<()> {
    let root = tempfile::tempdir()?;
    let api = MockTwgga::new()?;
    let mut first_process = server(root.path(), &api)?;
    let first = first_process.call("image_generate", json!({"prompt":format!("ROOT_INTENT red clothing {FIXTURE_KEY}"),
        "session_id":"new", "size":"1536x1024", "quality":"low", "context":{"identity":"same face","clothing":"red","secret":FIXTURE_KEY}}))?;
    assert_eq!(first["ok"], true);
    assert_eq!(first["session"]["revision"], 1);
    assert!(first["session"]["parent_revision"].is_null());
    let id = first["session"]["id"].as_str().unwrap();
    let original = fs::read(first["saved"][0]["path"].as_str().unwrap())?;
    let other = first_process.call(
        "image_generate",
        json!({"prompt":"PRIVATE_OTHER_SESSION","session_id":"new"}),
    )?;
    assert_ne!(other["session"]["id"], id);
    assert!(
        !api.requests()[0].fields["prompt"]
            .as_str()
            .unwrap()
            .contains(FIXTURE_KEY)
    );
    drop(first_process);
    let mut second_process = server(root.path(), &api)?;
    let second = second_process.call("image_edit", json!({"prompt":"BEACH_INTENT background seaside","session_id":id,"context":{"background":"seaside"}}))?;
    assert_eq!(second["ok"], true);
    assert_eq!(second["session"]["revision"], 2);
    assert_eq!(second["session"]["parent_revision"], 1);
    assert_eq!(second["session"]["history_length"], 2);
    assert_eq!(second["session"]["context"]["clothing"], "red");
    assert_eq!(second["model"], first["model"]);
    assert_eq!(second["size"], "1536x1024");
    let captured = api.requests();
    let request = captured.last().unwrap();
    assert_eq!(request.path, "/v1/images/edits");
    assert_eq!(request.fields["quality"], "low");
    assert_eq!(request.files[0].sha256, sha256(&original));
    let prompt = request.fields["prompt"].as_str().unwrap();
    assert!(prompt.contains("ROOT_INTENT"));
    assert!(!prompt.contains("PRIVATE_OTHER_SESSION"));
    let second_image = fs::read(second["saved"]["path"].as_str().unwrap())?;
    assert_ne!(
        second_image, original,
        "fixture revisions must distinguish selected images"
    );
    drop(second_process);
    let mut third_process = server(root.path(), &api)?;
    let third = third_process.call("image_generate", json!({"prompt":"BLUE_INTENT blue clothing","session_id":id,"context":{"clothing":"blue","secret":null}}))?;
    assert_eq!(third["ok"], true);
    assert_eq!(third["saved"][0]["index"], 1);
    assert_eq!(third["requested_n"], 1);
    assert_eq!(third["errors"], json!([]));
    assert_eq!(third["size_honored"], false);
    assert_eq!(third["session"]["revision"], 3);
    assert_eq!(third["session"]["context"]["background"], "seaside");
    assert!(third["session"]["context"].get("secret").is_none());
    assert_eq!(
        api.requests().last().unwrap().files[0].sha256,
        sha256(&second_image)
    );
    let branch = third_process.call(
        "image_generate",
        json!({"prompt":"BRANCH_INTENT add hat","session_id":id,"parent_revision":1}),
    )?;
    assert_eq!(branch["ok"], true);
    assert_eq!(branch["session"]["revision"], 4);
    assert_eq!(branch["session"]["parent_revision"], 1);
    assert_eq!(branch["session"]["history_length"], 2);
    assert_eq!(branch["session"]["context"]["clothing"], "red");
    assert!(branch["session"]["context"].get("background").is_none());
    let captured = api.requests();
    let request = captured.last().unwrap();
    assert_eq!(request.files[0].sha256, sha256(&original));
    let prompt = request.fields["prompt"].as_str().unwrap();
    assert!(prompt.contains("ROOT_INTENT"));
    for excluded in [
        "BLUE_INTENT",
        "BEACH_INTENT",
        "PRIVATE_OTHER_SESSION",
        FIXTURE_KEY,
    ] {
        assert!(!prompt.contains(excluded));
    }
    for entry in fs::read_dir(root.path().join(".twgga-sessions").join(id))? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let record = fs::read_to_string(path)?;
            assert!(!record.contains(FIXTURE_KEY));
            assert!(!record.contains("api_key"));
        }
    }
    assert!(!String::from_utf8_lossy(&third_process.stderr_bytes()).contains(FIXTURE_KEY));
    Ok(())
}

#[test]
fn failed_first_and_continued_requests_never_advance_or_persist_failed_instructions() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let api = MockTwgga::new()?;
    let mut server = server(root.path(), &api)?;
    let failed = server.call(
        "image_generate",
        json!({"prompt":"[scenario:secret_error] FAILED_INITIAL","session_id":"new"}),
    )?;
    assert_eq!(failed["ok"], false);
    let first = server.call(
        "image_generate",
        json!({"prompt":"INITIAL_SUCCESS","session_id":"new","context":{"clothing":"red"}}),
    )?;
    let id = first["session"]["id"].as_str().unwrap();
    for scenario in ["secret_error", "no_image"] {
        let failed = server.call("image_generate", json!({"prompt":format!("[scenario:{scenario}] FAILED_CHANGE"),"session_id":id,"context":{"clothing":"green"}}))?;
        assert_eq!(failed["ok"], false);
        assert_eq!(failed["session"]["revision"], 1);
        assert_eq!(failed["session"]["context"]["clothing"], "red");
        assert!(!failed.to_string().contains(FIXTURE_KEY));
        assert!(
            !root
                .path()
                .join(".twgga-sessions")
                .join(id)
                .join("revision-2.json")
                .exists()
        );
    }
    let next = server.call(
        "image_generate",
        json!({"prompt":"SUCCESS_AFTER_FAILURE","session_id":id}),
    )?;
    assert_eq!(next["session"]["revision"], 2);
    let captures = api.requests();
    let prompt = captures.last().unwrap().fields["prompt"].as_str().unwrap();
    assert!(!prompt.contains("FAILED_CHANGE"));
    assert!(!prompt.contains("green"));
    Ok(())
}

#[test]
fn invalid_session_requests_are_rejected_before_http() -> Result<()> {
    let root = tempfile::tempdir()?;
    let api = MockTwgga::new()?;
    let mut server = server(root.path(), &api)?;
    for extra in [
        json!({"session_id":"new","n":2}),
        json!({"context":{"identity":"same"}}),
        json!({"parent_revision":1}),
        json!({"session_id":"new","parent_revision":1}),
        json!({"session_id":"../another-session"}),
        json!({"session_id":format!("edit_{}","a".repeat(32))}),
        json!({"session_id":"new","context":{"identity":""}}),
        json!({"session_id":"new","context":{"identity":"x".repeat(2049)}}),
        json!({"session_id":"new","prompt":"x".repeat(32001)}),
        json!({"session_id":"new","prompt":"描述".repeat(15750),"context":{"identity":"人物".repeat(1000)}}),
    ] {
        let mut args = json!({"prompt":"portrait"});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = server.call("image_generate", args)?;
        assert_eq!(result["ok"], false);
        assert!(api.requests().is_empty());
    }
    Ok(())
}

#[test]
fn remembered_images_cannot_bypass_the_input_root_or_be_used_after_removal() -> Result<()> {
    for restrict in [false, true] {
        let root = tempfile::tempdir()?;
        let allowed = root.path().join("allowed");
        fs::create_dir(&allowed)?;
        let api = MockTwgga::new()?;
        let mut env = api.env();
        if restrict {
            env.push(("TWGGA_INPUT_ROOT", allowed.to_str().unwrap()));
        }
        let mut server = Session::new(
            Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp")),
            root.path(),
            &env,
        )?;
        server.initialize()?;
        let first = server.call(
            "image_generate",
            json!({"prompt":"portrait","session_id":"new"}),
        )?;
        let id = first["session"]["id"].as_str().unwrap();
        if !restrict {
            fs::remove_file(first["saved"][0]["path"].as_str().unwrap())?;
        }
        let next = server.call(
            "image_generate",
            json!({"prompt":"continue","session_id":id}),
        )?;
        assert_eq!(next["ok"], false);
        if restrict {
            assert!(next["error"].as_str().unwrap().contains("TWGGA_INPUT_ROOT"));
        }
        assert_eq!(api.requests().len(), 1);
        assert!(
            !root
                .path()
                .join(".twgga-sessions")
                .join(id)
                .join("revision-2.json")
                .exists()
        );
    }
    Ok(())
}

#[test]
fn stateless_sessions_survive_a_process_restart_without_initialize() -> Result<()> {
    let root = tempfile::tempdir()?;
    let api = MockTwgga::new()?;
    let binary = Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp"));
    let mut first = Session::new(binary, root.path(), &api.env())?;
    first.request("server/discover", json!({"_meta":latest_meta()}))?;
    let generated = tool_value(&first.request("tools/call",json!({"_meta":latest_meta(),"name":"image_generate","arguments":{"prompt":"STATELESS_ORIGINAL","session_id":"new"}}))?)?;
    drop(first);
    let mut next = Session::new(binary, root.path(), &api.env())?;
    let edited = tool_value(&next.request("tools/call",json!({"_meta":latest_meta(),"name":"image_generate","arguments":{"prompt":"STATELESS_NEXT","session_id":generated["session"]["id"]}}))?)?;
    assert_eq!(edited["session"]["revision"], 2);
    assert!(
        api.requests().last().unwrap().fields["prompt"]
            .as_str()
            .unwrap()
            .contains("STATELESS_ORIGINAL")
    );
    Ok(())
}

#[test]
fn overlapping_edits_to_one_session_produce_one_revision_and_one_provider_request() -> Result<()> {
    let root = tempfile::tempdir()?;
    let api = MockTwgga::new()?;
    let mut server = server(root.path(), &api)?;
    let first = server.call(
        "image_generate",
        json!({"prompt":"original","session_id":"new"}),
    )?;
    let params = json!({"name":"image_generate","arguments":{"prompt":"[scenario:concurrency_probe] continue","session_id":first["session"]["id"]}});
    let a = server.send("tools/call", params.clone())?;
    let b = server.send("tools/call", params)?;
    let results: Vec<Value> = [a, b]
        .into_iter()
        .map(|id| tool_value(&server.wait(id)?))
        .collect::<Result<_>>()?;
    assert_eq!(
        results.iter().filter(|result| result["ok"] == true).count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result["ok"] == false)
            .count(),
        1
    );
    assert_eq!(api.requests().len(), 2);
    Ok(())
}

// 会话记录存的是返回给客户的 saved.path（已去掉 Windows 的 \\?\ 前缀）。设置了
// TWGGA_INPUT_ROOT 时，输入图经该根目录校验，返回带前缀的规范路径。继续编辑时两种写法
// 必须被认作同一个文件，否则 Windows 上「文生图后接着改」会被误拒。2026-11-09 线上实测撞到过。
#[test]
fn generated_session_continues_when_roots_are_canonical() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().canonicalize()?;
    let root_text = root.to_str().ok_or("root path is not unicode")?.to_owned();
    let api = MockTwgga::new()?;
    let mut env = api.env();
    env.push(("TWGGA_SAVE_DIR", &root_text));
    env.push(("TWGGA_SAVE_DIR_ROOT", &root_text));
    env.push(("TWGGA_INPUT_ROOT", &root_text));
    let binary = Path::new(env!("CARGO_BIN_EXE_twgga-image-mcp"));

    let mut first = Session::new(binary, &root, &env)?;
    first.initialize()?;
    let generated = first.call("image_generate", json!({"prompt":"cup","session_id":"new"}))?;
    assert_eq!(generated["ok"], true, "{generated}");
    let id = generated["session"]["id"].clone();
    let saved = generated["saved"][0]["path"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!saved.starts_with(r"\\?\"), "{saved}");
    drop(first);

    let mut second = Session::new(binary, &root, &env)?;
    second.initialize()?;
    let edited = second.call(
        "image_edit",
        json!({"prompt":"make it yellow","session_id":id}),
    )?;
    assert_eq!(edited["ok"], true, "{edited}");
    assert_eq!(edited["session"]["revision"], 2);
    assert_eq!(edited["session"]["input_path"], saved);

    // The path the customer saw, passed back explicitly, is the same file too.
    let explicit = second.call(
        "image_edit",
        json!({"prompt":"make it green","session_id":id,"parent_revision":1,"image_path":saved}),
    )?;
    assert_eq!(explicit["ok"], true, "{explicit}");
    Ok(())
}
