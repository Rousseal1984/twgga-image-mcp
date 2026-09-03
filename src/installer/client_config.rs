use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fmt,
    path::{Path, PathBuf},
};

use super::InstallError;

const SECRET_ENV_NAMES: &[&str] = &[
    "TWGGA_API_KEY",
    "TWGGA_GROK_API_KEY",
    "XAI_API_KEY",
    "GROK_API_KEY",
];

const MANAGED_ENV_NAMES: &[&str] = &[
    "TWGGA_BASEURL",
    "TWGGA_MODEL",
    "TWGGA_SAVE_DIR",
    "TWGGA_SAVE_DIR_ROOT",
    "TWGGA_INPUT_ROOT",
    "TWGGA_USE_SHELL_PROXY",
    "TWGGA_RESPONSE_FORMAT",
    "TWGGA_TRUSTED_DOWNLOAD_HOSTS",
    "TWGGA_ALLOW_FAKE_IP_DOWNLOAD",
    "TWGGA_KEYCHAIN_ACCOUNT",
    "TWGGA_KEYCHAIN_SERVICE",
];

#[derive(Clone, Eq, PartialEq)]
pub struct ClientLaunchSpec {
    command: PathBuf,
    args: Vec<OsString>,
    env: BTreeMap<String, String>,
}

impl ClientLaunchSpec {
    pub fn new(command: PathBuf, args: Vec<OsString>, mut env: BTreeMap<String, String>) -> Self {
        for name in SECRET_ENV_NAMES {
            env.remove(*name);
        }
        Self { command, args, env }
    }

    pub(crate) fn from_parsed(
        command: PathBuf,
        args: Vec<OsString>,
        env: BTreeMap<String, String>,
    ) -> Self {
        Self { command, args, env }
    }

    pub fn command(&self) -> &Path {
        &self.command
    }

    pub fn args(&self) -> &[OsString] {
        &self.args
    }

    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// 写进客户端配置的可执行文件路径。
    ///
    /// **刻意不做显示归一化。** Windows 的 `\\?\` 扩展前缀不是噪声，它是超过
    /// MAX_PATH（260 字符）的路径唯一能被打开的形式；客户端要拿这个值真的去执行
    /// 二进制，剥掉前缀会让装在长路径下的安装当场失效。展示用的归一化只适用于
    /// 给人看的字段，不适用于这里。
    pub(crate) fn command_text(&self) -> Result<&str, InstallError> {
        unicode(self.command.as_os_str(), "Rust binary path")
    }

    pub(crate) fn argument_texts(&self) -> Result<Vec<&str>, InstallError> {
        self.args
            .iter()
            .map(|argument| unicode(argument, "MCP command argument"))
            .collect()
    }
}

impl fmt::Debug for ClientLaunchSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientLaunchSpec")
            .field("command", &self.command)
            .field("args", &self.args)
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

pub(crate) fn is_secret_environment_name(name: &str) -> bool {
    SECRET_ENV_NAMES.contains(&name)
}

pub(crate) fn keep_existing_environment_name(name: &str, expected: &ClientLaunchSpec) -> bool {
    !is_secret_environment_name(name)
        && (!MANAGED_ENV_NAMES.contains(&name) || expected.env().contains_key(name))
}

pub(crate) fn verify_launch_round_trip(
    actual: &ClientLaunchSpec,
    expected: &ClientLaunchSpec,
) -> Result<(), String> {
    if actual.command() != expected.command() {
        return Err("command 与原 PathBuf 不一致".into());
    }
    if actual.args() != expected.args() {
        return Err("args 与原 OsString 不一致".into());
    }
    for (name, expected_value) in expected.env() {
        if actual.env().get(name) != Some(expected_value) {
            return Err(format!("env.{name} 与原值不一致"));
        }
    }
    if actual
        .env()
        .keys()
        .any(|name| is_secret_environment_name(name))
    {
        return Err("客户端配置不得持久化 API key".into());
    }
    Ok(())
}

fn unicode<'a>(value: &'a OsStr, context: &'static str) -> Result<&'a str, InstallError> {
    value
        .to_str()
        .ok_or(InstallError::NonUnicodePath { context })
}
