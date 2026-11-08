#![cfg(windows)]
//! Windows 不许覆盖正在运行的 exe。客户让 AI 在客户端里跑升级命令时，客户端正开着
//! 旧版 MCP：2026-11-09 实测 v0.5.0 在这种情况下报「拒绝访问 (os error 5)」，升级失败。

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use twgga_image_mcp::installer::binary::{install_binary, install_binary_with_report};

fn parked_binaries(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{error}"))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".exe.old-"))
        })
        .collect()
}

#[test]
fn running_binary_is_parked_so_the_upgrade_lands() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let bin_dir = temp.path().join("data-local/twgga-image-mcp/bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("{error}"));
    let destination = bin_dir.join("twgga-image-mcp.exe");
    let system_root =
        std::env::var_os("SystemRoot").unwrap_or_else(|| panic!("SystemRoot is not set"));
    fs::copy(
        PathBuf::from(system_root).join("System32").join("cmd.exe"),
        &destination,
    )
    .unwrap_or_else(|error| panic!("{error}"));

    // 让「旧版」一直跑着，就像客户端开着的 MCP。
    let mut running = Command::new(&destination)
        .args(["/c", "ping", "-n", "20", "127.0.0.1"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("{error}"));

    // 前提：直接覆盖运行中的 exe 会被拒绝，正是 v0.5.0 失败的那一步。
    let probe = temp.path().join("probe.exe");
    fs::write(&probe, b"probe").unwrap_or_else(|error| panic!("{error}"));
    let blocked = fs::rename(&probe, &destination);

    let source = temp.path().join("new.exe");
    fs::write(&source, b"native-rust-binary-v2").unwrap_or_else(|error| panic!("{error}"));
    let report = install_binary_with_report(&source, &destination);
    let _ = running.kill();
    let _ = running.wait();

    assert!(
        blocked.is_err(),
        "前提不成立：运行中的 exe 竟然能被直接覆盖"
    );
    let report = report.unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(report.path, destination);
    assert!(
        report.parked_running_binary,
        "应报告旧程序被挪开，安装器据此提示重启客户端"
    );
    assert_eq!(
        fs::read(&destination).unwrap_or_else(|error| panic!("{error}")),
        b"native-rust-binary-v2"
    );
    assert_eq!(parked_binaries(&bin_dir).len(), 1, "旧程序应改名留在原目录");

    // 下一次安装时旧进程已退出，挪开的旧程序被清掉；正常替换不再报告挪开。
    fs::write(&source, b"native-rust-binary-v3").unwrap_or_else(|error| panic!("{error}"));
    let again =
        install_binary_with_report(&source, &destination).unwrap_or_else(|error| panic!("{error}"));
    assert!(!again.parked_running_binary);
    assert_eq!(
        fs::read(&destination).unwrap_or_else(|error| panic!("{error}")),
        b"native-rust-binary-v3"
    );
    assert!(
        parked_binaries(&bin_dir).is_empty(),
        "已退出的旧程序应在下次安装时清掉"
    );
    // 重跑同一版（内容相同、走提前返回）也要清掉上次挪开的旧程序；原有的入口仍然可用。
    let leftover = bin_dir.join("twgga-image-mcp.exe.old-1");
    fs::write(&leftover, b"parked").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        install_binary(&source, &destination).unwrap_or_else(|error| panic!("{error}")),
        destination
    );
    assert!(
        parked_binaries(&bin_dir).is_empty(),
        "重跑同一版时也应清掉挪开的旧程序"
    );
}
