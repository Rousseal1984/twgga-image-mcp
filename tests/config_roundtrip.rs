use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use twgga_image_mcp::installer::{
    ClientLaunchSpec,
    claude::{
        merge_config as merge_claude_config, parse_config_launch as parse_claude_launch,
        reset_config as reset_claude_config,
    },
    codex::{merge_config, parse_config_launch, reset_config},
};

fn launch(command: impl Into<PathBuf>) -> ClientLaunchSpec {
    ClientLaunchSpec::new(
        command.into(),
        Vec::new(),
        BTreeMap::from([
            (
                "TWGGA_SAVE_DIR".into(),
                "C:\\Users\\张三\\Pictures\\TWGGA 图像".into(),
            ),
            (
                "TWGGA_SAVE_DIR_ROOT".into(),
                "C:\\Users\\张三\\Pictures\\TWGGA 图像".into(),
            ),
        ]),
    )
}

fn assert_codex_round_trip(path: &str) {
    let expected = launch(PathBuf::from(path));
    let rendered = merge_config("", &expected).unwrap_or_else(|error| panic!("{path}: {error}"));
    let parsed = parse_config_launch(&rendered)
        .unwrap_or_else(|error| panic!("{path}: {error}"))
        .unwrap_or_else(|| panic!("{path}: twgga-image missing"));
    assert_eq!(parsed, expected, "serialized TOML must preserve {path:?}");
}

#[test]
fn codex_config_windows_backslash_regression_issue_4() {
    let expected = ClientLaunchSpec::new(
        PathBuf::from(r"C:\Python313\python.exe"),
        vec![OsString::from(r"E:\twgga-image-mcp-main\server.py")],
        BTreeMap::from([
            (
                "TWGGA_SAVE_DIR".into(),
                r"C:\Users\Neo\Pictures\twgga-out".into(),
            ),
            (
                "TWGGA_SAVE_DIR_ROOT".into(),
                r"C:\Users\Neo\Pictures\twgga-out".into(),
            ),
        ]),
    );
    let rendered = merge_config("", &expected).unwrap_or_else(|error| panic!("{error}"));

    // This is the actual failure mode reported in #4: a parser must accept the serialized
    // document and recover the original Windows paths exactly. Quote style is irrelevant.
    let parsed = parse_config_launch(&rendered)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("twgga-image missing"));
    assert_eq!(parsed, expected);
}

#[test]
fn codex_config_round_trips_windows_path_matrix() {
    for path in [
        r"C:\Python313\python.exe",
        r"C:\Program Files\Twgga MCP\twgga-image-mcp.exe",
        r"C:\Users\Neo\Pictures\twgga-out",
        r"C:\Users\张三\Pictures\TWGGA 图像",
        r"D:\dev\twgga-image-mcp\target\release\twgga-image-mcp.exe",
        r"\\server\share\twgga image\twgga-image-mcp.exe",
        r"\\?\C:\very long directory\twgga-image-mcp.exe",
        r"C:\Users\O'Brien\Pictures\twgga-out",
        "C:\\path with \"double quote\"\\twgga-image-mcp.exe",
        r"C:\path with # hash\twgga-image-mcp.exe",
        r"C:\path=with=equals\twgga-image-mcp.exe",
        "C:\\trailing\\",
        "C:\\",
    ] {
        assert_codex_round_trip(path);
    }
}

#[test]
fn codex_config_round_trips_posix_path_matrix() {
    for path in [
        "/Users/neo/Twgga Image/twgga-image-mcp",
        "/home/neo/图片/TWGGA",
        "/tmp/a'b/twgga-image-mcp",
        "/tmp/a\"b/twgga-image-mcp",
        "/tmp/a#b/twgga-image-mcp",
        "/tmp/a=b/twgga-image-mcp",
    ] {
        assert_codex_round_trip(path);
    }
}

#[test]
fn codex_merge_is_idempotent_and_preserves_comments_unknown_fields_and_other_servers() {
    let existing = r#"# keep this comment
model = 'gpt-test'

[mcp_servers.other]
command = 'other'
unknown = 'keep'

[mcp_servers.twgga-image]
command = 'old'
custom = 'preserve-me'

[mcp_servers.twgga-image.env]
TWGGA_API_KEY = 'remove-me'
CUSTOM_ENV = 'keep-env'
"#;
    let expected = launch(r"C:\Program Files\Twgga MCP\twgga-image-mcp.exe");
    let first = merge_config(existing, &expected).unwrap_or_else(|error| panic!("{error}"));
    let second = merge_config(&first, &expected).unwrap_or_else(|error| panic!("{error}"));

    assert_eq!(second, first);
    assert!(first.contains("# keep this comment"));
    assert!(first.contains("[mcp_servers.other]"));
    assert!(first.contains("unknown = 'keep'"));
    assert!(first.contains("custom = 'preserve-me'"));
    assert!(first.contains("CUSTOM_ENV = 'keep-env'"));
    assert!(!first.contains("TWGGA_API_KEY"));
    let parsed = parse_config_launch(&first)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("twgga-image missing"));
    assert_eq!(parsed.command(), expected.command());
    assert_eq!(parsed.args(), expected.args());
    for (name, value) in expected.env() {
        assert_eq!(parsed.env().get(name), Some(value));
    }
    assert_eq!(
        parsed.env().get("CUSTOM_ENV").map(String::as_str),
        Some("keep-env")
    );
}

#[test]
fn codex_reset_removes_only_twgga_image_sections() {
    let existing = merge_config(
        "model = 'gpt-test'\n\n[mcp_servers.other]\ncommand = 'other'\n",
        &launch("/opt/twgga-image-mcp"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let reset = reset_config(&existing)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("expected a changed document"));
    assert!(reset.contains("model = 'gpt-test'"));
    assert!(reset.contains("[mcp_servers.other]"));
    assert!(!reset.contains("mcp_servers.twgga-image"));
}

#[test]
fn installers_remove_stale_managed_environment_but_preserve_unknown_environment() {
    let launch = ClientLaunchSpec::new(
        PathBuf::from("/stable/twgga-image-mcp"),
        Vec::new(),
        BTreeMap::from([("TWGGA_SAVE_DIR".into(), "/new/output".into())]),
    );
    let codex_existing = r#"[mcp_servers.twgga-image]
command = 'old'
args = []

[mcp_servers.twgga-image.env]
TWGGA_BASEURL = 'https://old.example'
TWGGA_INPUT_ROOT = '/old/input'
CUSTOM_ENV = 'keep'
"#;
    let codex = merge_config(codex_existing, &launch).unwrap_or_else(|error| panic!("{error}"));
    assert!(!codex.contains("TWGGA_BASEURL"));
    assert!(!codex.contains("TWGGA_INPUT_ROOT"));
    assert!(codex.contains("CUSTOM_ENV = 'keep'"));

    let claude_existing = r#"{
      "mcpServers": {
        "twgga-image": {
          "command": "old",
          "args": [],
          "env": {
            "TWGGA_BASEURL": "https://old.example",
            "TWGGA_INPUT_ROOT": "/old/input",
            "CUSTOM_ENV": "keep"
          }
        }
      }
    }"#;
    let claude =
        merge_claude_config(claude_existing, &launch).unwrap_or_else(|error| panic!("{error}"));
    assert!(!claude.contains("TWGGA_BASEURL"));
    assert!(!claude.contains("TWGGA_INPUT_ROOT"));
    assert!(claude.contains("CUSTOM_ENV"));
}

#[cfg(unix)]
#[test]
fn codex_config_rejects_non_unicode_paths_instead_of_lossy_rewriting() {
    use std::os::unix::ffi::OsStringExt;

    let path = PathBuf::from(OsString::from_vec(b"/tmp/twgga-\xff".to_vec()));
    let error = merge_config("", &launch(path)).expect_err("non-Unicode path must fail");
    assert!(error.to_string().contains("Unicode"));
}

#[test]
fn client_launch_spec_command_is_a_path_and_args_are_separate() {
    let spec = launch("/opt/Twgga MCP/twgga-image-mcp");
    assert_eq!(spec.command(), Path::new("/opt/Twgga MCP/twgga-image-mcp"));
    assert!(spec.args().is_empty());
}

#[test]
fn claude_json_round_trips_windows_paths_and_preserves_unknown_fields() {
    let existing = r#"{
      "theme": "dark",
      "mcpServers": {
        "other": {"command": "other"},
        "twgga-image": {"command": "old", "custom": "keep", "env": {"CUSTOM_ENV": "keep"}}
      }
    }"#;
    let expected = ClientLaunchSpec::new(
        PathBuf::from(r"C:\Program Files\Twgga MCP\twgga-image-mcp.exe"),
        Vec::new(),
        BTreeMap::from([
            ("TWGGA_API_KEY".into(), "must-not-persist".into()),
            (
                "TWGGA_SAVE_DIR".into(),
                r"C:\Users\O'Brien\Pictures\TWGGA 图像".into(),
            ),
        ]),
    );
    let first = merge_claude_config(existing, &expected).unwrap_or_else(|error| panic!("{error}"));
    let second = merge_claude_config(&first, &expected).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(second, first);

    let raw: serde_json::Value =
        serde_json::from_str(&first).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(raw["theme"], "dark");
    assert_eq!(raw["mcpServers"]["other"]["command"], "other");
    assert_eq!(raw["mcpServers"]["twgga-image"]["custom"], "keep");
    assert_eq!(raw["mcpServers"]["twgga-image"]["env"]["CUSTOM_ENV"], "keep");
    assert!(first.contains(r"C:\\Program Files\\Twgga MCP"));
    assert!(!first.contains("must-not-persist"));
    assert!(!first.contains("TWGGA_API_KEY"));

    let parsed = parse_claude_launch(&first)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("twgga-image missing"));
    assert_eq!(parsed.command(), expected.command());
    assert_eq!(parsed.args(), expected.args());
    assert_eq!(
        parsed.env().get("TWGGA_SAVE_DIR"),
        expected.env().get("TWGGA_SAVE_DIR")
    );
}

#[test]
fn claude_reset_removes_only_twgga_image() {
    let existing = merge_claude_config(
        r#"{"theme":"dark","mcpServers":{"other":{"command":"other"}}}"#,
        &launch("/opt/twgga-image-mcp"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let reset = reset_claude_config(&existing)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("expected a changed document"));
    let raw: serde_json::Value =
        serde_json::from_str(&reset).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(raw["theme"], "dark");
    assert_eq!(raw["mcpServers"]["other"]["command"], "other");
    assert!(raw["mcpServers"].get("twgga-image").is_none());
}
