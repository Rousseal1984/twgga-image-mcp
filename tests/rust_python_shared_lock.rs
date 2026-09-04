use std::{path::PathBuf, process::Command, time::Duration};

use twgga_image_mcp::fs::lock::BigRequestGate;

#[test]
fn python_and_rust_serialize_on_the_same_lock_file() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let lock = temp.path().join(".cache/twgga-image/bigsize.lock");
    let ready = temp.path().join("python-ready");
    let python = std::env::var_os("PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "python" } else { "python3" }));
    let script = r#"
import asyncio
import os
from pathlib import Path
from twgga_image_mcp.locks import _big_size_file_lock_async

async def main():
    async with _big_size_file_lock_async():
        Path(os.environ["TWGGA_LOCK_READY"]).write_text("ready", encoding="utf-8")
        await asyncio.sleep(0.7)

asyncio.run(main())
"#;
    let mut child = Command::new(&python)
        .args(["-c", script])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOME", temp.path())
        .env("USERPROFILE", temp.path())
        .env("TWGGA_LOCK_READY", &ready)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("unable to start Python reference: {error}"));

    // 冷启动的 CI runner 上，第一次 import mcp/httpx/Pillow 就可能花掉好几秒，
    // 原本 10 秒的窗口在 Windows runner 上不够用。
    let wait_started = std::time::Instant::now();
    while !ready.is_file() && wait_started.elapsed() < Duration::from_secs(60) {
        if let Ok(Some(status)) = child.try_wait() {
            // 子进程先退出了：它不会再拿锁了，等下去没有意义，
            // 而且它的输出正是唯一能说明原因的东西。
            let output = child
                .wait_with_output()
                .unwrap_or_else(|error| panic!("{error}"));
            panic!(
                "Python reference exited early with {status}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        ready.is_file(),
        "Python reference ({}) did not acquire its lock within 60s",
        python.display()
    );

    let runtime = tokio::runtime::Runtime::new().unwrap_or_else(|error| panic!("{error}"));
    let rust_started = std::time::Instant::now();
    runtime.block_on(async {
        let gate = BigRequestGate::new(lock);
        let _guard = gate
            .acquire(&mut Vec::new())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
    });
    let rust_wait = rust_started.elapsed();
    let status = child.wait().unwrap_or_else(|error| panic!("{error}"));
    assert!(status.success());
    assert!(
        rust_wait >= Duration::from_millis(350),
        "Rust acquired in {rust_wait:?}; Python and Rust did not share the lock"
    );
}
