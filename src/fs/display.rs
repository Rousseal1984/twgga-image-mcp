use std::path::Path;

/// Windows 扩展长度路径前缀。`fs::canonicalize` 在 Windows 上一律返回这种形式。
const VERBATIM_PREFIX: &str = r"\\?\";
/// 网络共享的扩展形式，例如 `\\?\UNC\server\share`。
const VERBATIM_UNC_PREFIX: &str = r"\\?\UNC\";

/// 把路径渲染成给人看的形式，仅用于**输出边界**。
///
/// Windows 上 `fs::canonicalize` 返回的是扩展形式（`\\?\C:\...`），而 Python 的
/// `expanduser` / `resolve` 给的是普通的 `C:\...`。两者指同一个位置，但差异会一路
/// 漏到客户眼前：server_info 里的目录、写进 Claude / Codex 配置的可执行文件路径、
/// 以及「文件不存在」这类报错。有些客户端和工具链并不认扩展形式。
///
/// 文件系统操作与路径包含性检查仍然使用规范化后的原值 —— 那是安全边界，不能因为
/// 好看就改。这里只负责显示。
pub fn display_path(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if let Some(rest) = text.strip_prefix(VERBATIM_UNC_PREFIX) {
        return Some(format!(r"\\{rest}"));
    }
    Some(
        text.strip_prefix(VERBATIM_PREFIX)
            .unwrap_or(text)
            .to_owned(),
    )
}

/// 同上，但在路径含非 Unicode 字节时退回 `Path::display` 的有损渲染，
/// 供报错文案这类「宁可难看也不能没有」的场合使用。
pub fn display_path_lossy(path: &Path) -> String {
    display_path(path).unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 断言用的字面量刻意逐字写出，不靠常量拼装：这两个常量本身就是被测对象，
    /// 用它们造期望值只会让测试与实现一起错、还一起绿。这条测试的第一版正是
    /// 这样漏掉了一个多余的反斜杠 —— 常量与断言同时写错，彼此吻合，
    /// 而真实路径一个都匹配不上。
    #[test]
    fn strips_the_windows_extended_prefix_but_leaves_ordinary_paths_alone() {
        assert_eq!(
            display_path(Path::new("\\\\?\\C:\\Users\\x\\out")).as_deref(),
            Some("C:\\Users\\x\\out")
        );
        assert_eq!(
            display_path(Path::new("C:\\Users\\x\\out")).as_deref(),
            Some("C:\\Users\\x\\out")
        );
        assert_eq!(
            display_path(Path::new("/home/x/out")).as_deref(),
            Some("/home/x/out")
        );
    }

    /// 前缀必须恰好是四个字符 `\`, `\`, `?`, `\`。多一个少一个都匹配不上真实路径。
    #[test]
    fn the_prefix_constant_is_exactly_the_windows_verbatim_marker() {
        assert_eq!(VERBATIM_PREFIX.len(), 4);
        assert_eq!(
            VERBATIM_PREFIX.chars().collect::<Vec<_>>(),
            vec!['\\', '\\', '?', '\\']
        );
        assert_eq!(VERBATIM_UNC_PREFIX, "\\\\?\\UNC\\");
    }

    /// 网络共享的扩展形式要还原成 `\\server\share`，而不是把 `UNC\` 留在中间 ——
    /// 那样得到的既不是有效路径，也没人认得出来。
    #[test]
    fn restores_unc_shares_rather_than_leaving_the_marker_in_place() {
        assert_eq!(
            display_path(Path::new("\\\\?\\UNC\\server\\share\\out")).as_deref(),
            Some("\\\\server\\share\\out")
        );
    }
}
