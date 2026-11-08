use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use super::InstallError;

/// 后缀之后是纳秒时间戳，保证同一目录里多次挪开不会撞名。
const PARKED_SUFFIX: &str = ".old-";

/// 一次安装的结果。`parked_running_binary` 为真表示旧程序当时正被客户端运行，
/// 已改名挪开给新程序让位，客户端重启后才会改用新版。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryInstall {
    pub path: PathBuf,
    pub parked_running_binary: bool,
}

pub fn install_binary(source: &Path, destination: &Path) -> Result<PathBuf, InstallError> {
    install_binary_with_report(source, destination).map(|report| report.path)
}

pub fn install_binary_with_report(
    source: &Path,
    destination: &Path,
) -> Result<BinaryInstall, InstallError> {
    let unchanged = |path: &Path| BinaryInstall {
        path: path.to_path_buf(),
        parked_running_binary: false,
    };
    if !source.is_file() {
        return Err(InstallError::BinarySource("source 不存在或不是文件".into()));
    }
    // 放在「已是同一版本」的提前返回之前：重跑同一版安装命令也要清掉上次挪开的旧程序。
    remove_parked_binaries(destination);
    if same_file(source, destination)? {
        make_executable(destination)?;
        return Ok(unchanged(destination));
    }
    if destination.is_file() && files_equal(source, destination)? {
        make_executable(destination)?;
        return Ok(unchanged(destination));
    }

    let parent = destination
        .parent()
        .ok_or_else(|| InstallError::BinaryDirectory("目标没有 parent".into()))?;
    fs::create_dir_all(parent).map_err(|error| InstallError::BinaryDirectory(error.to_string()))?;
    make_private_directory(parent)?;
    let mut temp = tempfile::Builder::new()
        .prefix(".twgga-binary-")
        .tempfile_in(parent)
        .map_err(|error| InstallError::BinaryCopy(error.to_string()))?;
    let mut input =
        File::open(source).map_err(|error| InstallError::BinarySource(error.to_string()))?;
    std::io::copy(&mut input, temp.as_file_mut())
        .map_err(|error| InstallError::BinaryCopy(error.to_string()))?;
    temp.as_file_mut()
        .flush()
        .map_err(|error| InstallError::BinaryCopy(error.to_string()))?;
    temp.as_file_mut()
        .sync_all()
        .map_err(|error| InstallError::BinaryCopy(error.to_string()))?;
    make_executable(temp.path())?;
    let parked_running_binary = match temp.persist(destination) {
        Ok(_) => false,
        Err(error)
            if cfg!(windows) && destination.is_file() && is_replace_blocked(&error.error) =>
        {
            replace_running_binary(error.file, destination, &error.error)?;
            true
        }
        Err(error) => return Err(InstallError::BinaryReplace(error.error.to_string())),
    };
    sync_parent(parent)?;
    Ok(BinaryInstall {
        path: destination.to_path_buf(),
        parked_running_binary,
    })
}

// Windows 不许覆盖正在运行的 exe（拒绝访问 os error 5，或共享冲突 os error 32），但允许
// 给它改名。客户多半是让 AI 在客户端里跑升级命令，那个客户端此刻正开着旧版 MCP，
// 所以这是升级的常态而不是意外（2026-11-09 实测 v0.5.0 在这里直接失败）。
// 做法：旧程序改名挪开，新程序放到原位；运行中的旧进程不受影响，客户端重启后改用新版。
// 挪开后仍放不进去时把旧程序挪回原位，客户端配置照旧可用。
fn replace_running_binary(
    temp: tempfile::NamedTempFile,
    destination: &Path,
    first_error: &std::io::Error,
) -> Result<(), InstallError> {
    let parked = parked_path(destination);
    fs::rename(destination, &parked).map_err(|_| replace_blocked_error(first_error))?;
    if let Err(error) = temp.persist(destination) {
        let _ = fs::rename(&parked, destination);
        return Err(replace_blocked_error(&error.error));
    }
    Ok(())
}

fn replace_blocked_error(error: &std::io::Error) -> InstallError {
    InstallError::BinaryReplace(format!(
        "{error}；旧版程序可能正被 Claude Code / Codex 等客户端运行，请先退出客户端（或结束 twgga-image-mcp 进程）后重试"
    ))
}

fn is_replace_blocked(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(32)
}

fn parked_path(destination: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let mut name = destination
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(format!("{PARKED_SUFFIX}{stamp}"));
    destination.with_file_name(name)
}

/// 清掉以前挪开的旧程序。旧进程还在运行时删不掉，留到下次安装再清。
fn remove_parked_binaries(destination: &Path) {
    let (Some(parent), Some(name)) = (destination.parent(), destination.file_name()) else {
        return;
    };
    let prefix = format!("{}{PARKED_SUFFIX}", name.to_string_lossy());
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn same_file(left: &Path, right: &Path) -> Result<bool, InstallError> {
    if left == right {
        return Ok(true);
    }
    if !right.exists() {
        return Ok(false);
    }
    let left =
        fs::canonicalize(left).map_err(|error| InstallError::BinarySource(error.to_string()))?;
    let right =
        fs::canonicalize(right).map_err(|error| InstallError::BinarySource(error.to_string()))?;
    Ok(left == right)
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, InstallError> {
    let left_metadata =
        fs::metadata(left).map_err(|error| InstallError::BinarySource(error.to_string()))?;
    let right_metadata =
        fs::metadata(right).map_err(|error| InstallError::BinarySource(error.to_string()))?;
    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }
    let mut left_file =
        File::open(left).map_err(|error| InstallError::BinarySource(error.to_string()))?;
    let mut right_file =
        File::open(right).map_err(|error| InstallError::BinarySource(error.to_string()))?;
    left_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| InstallError::BinarySource(error.to_string()))?;
    right_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| InstallError::BinarySource(error.to_string()))?;
    let mut left_buffer = [0_u8; 16 * 1024];
    let mut right_buffer = [0_u8; 16 * 1024];
    loop {
        let left_read = left_file
            .read(&mut left_buffer)
            .map_err(|error| InstallError::BinarySource(error.to_string()))?;
        let right_read = right_file
            .read(&mut right_buffer)
            .map_err(|error| InstallError::BinarySource(error.to_string()))?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| InstallError::BinaryCopy(error.to_string()))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o700);
    fs::set_permissions(path, permissions)
        .map_err(|error| InstallError::BinaryCopy(error.to_string()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), InstallError> {
    Ok(())
}

#[cfg(unix)]
fn make_private_directory(path: &Path) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| InstallError::BinaryDirectory(error.to_string()))
}

#[cfg(not(unix))]
fn make_private_directory(_path: &Path) -> Result<(), InstallError> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), InstallError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| InstallError::BinaryReplace(error.to_string()))
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), InstallError> {
    Ok(())
}
