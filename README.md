<p align="center">
  <img src="assets/banner.svg?v=2" alt="TWGGA IMAGE — GPT Image 2 MCP server" width="820">
</p>

# TWGGA 画图 MCP

把 [TWGGA](https://twgga.work) 的图像接口接入 Claude Code、Claude Desktop、Codex、Cursor 等 MCP 客户端，支持生成图片、连续编辑、批量编辑和多图参考。

当前仅支持 `gpt-image-2`，你的 TWGGA API Key 需要能调用该模型。

> **关于尺寸**：`size` 不决定输出像素。本站实测输出约 157 万像素，与请求的尺寸无关：请求 3840x2160、2048x1152、1536x1024 都返回 1254×1254。想要横图或竖图，把画幅写进 prompt，例如写「16:9 宽幅横构图」得到 1672×941。实际尺寸一律以返回的 `saved.actual_size` 为准，不要向用户承诺 2K/4K。

[连续编辑](#连续编辑) · [功能](#功能) · [安装](#安装) · [API Key 保存在哪](#api-key-保存在哪) · [升级](#升级) · [配置](#配置) · [v0.5.0 更新说明](docs/releases/v0.5.0.md)

---

## 连续编辑

MCP 会把每次成功的图片版本、编辑要求和有效约束保存在你的电脑上。沿用同一个会话 ID，就能在上一版图片上继续改；不同主题用各自的会话，互不干扰。

| 你说的话 | 图片会话 | MCP 的处理 |
| --- | --- | --- |
| 「生成一只小猫跳伞。」 | A · 第 1 版 | 新建会话，保存首张图和描述 |
| 「给它换一种飞行装备。」 | A · 第 2 版 | 上传 A 的上一版图片，带上主体要求和有效约束 |
| 「另外生成一张海景人像。」 | B · 第 1 版 | 新建 B，使用这个主题自己的图片与约束 |
| 「回到刚才的小猫，把背景改成日落。」 | A · 第 3 版 | 回到 A，从 A 的成功历史继续改 |

1. 首次生成或编辑时传 `session_id="new"`，客户端保存返回的 `session.id`。会话模式要求 `n=1`。
2. 继续修改时传原 ID 和本轮指令。省略 `model`、`size`、`quality` 时继承所选版本；MCP 自动上传该版本图片。
3. 新主题用 `"new"`，回到旧图用原 ID。指定 `parent_revision` 可以从旧版本另开分支。
4. 会话保存在运行 MCP 的电脑上，重启后可以继续。失败的调用不会推进版本。

选哪个会话由客户端里的 AI 决定，MCP 按传入的 ID 读取记录，不会自动拿到完整聊天记录。用法细节见[连续编辑指南](docs/multi-turn-editing.md)，占用与清理见[会话存储说明](docs/session-storage.md)。

---

## 功能

| 工具 | 说明 |
|---|---|
| `image_generate` | 根据文字生成图片；传入已有会话 ID 时继续编辑 |
| `image_edit` | 根据一张参考图和指令编辑图片，支持蒙版和连续编辑 |
| `image_batch_edit` | 对 2 到 10 张图片分别应用同一条指令；1K 最多 5 并发，2K/4K 逐张串行 |
| `image_multi_reference` | 同时参考 2 到 10 张图片生成一张新图 |
| `server_info` | 查看版本、接口地址、模型、尺寸规则、重试策略、安全限制和 Key 状态 |

第一次使用前，让客户端调用一次 `server_info`，确认当前配置。各客户端的用法、工具选择和故障排查见 [MCP 使用教程](docs/MCP使用教程.md)。

- 生成与编辑统一走 `/v1/images/generations` 与 `/v1/images/edits`。
- `quality` 可选 `auto` / `low` / `medium` / `high`。
- `output_format` 可选 `png` / `jpeg` / `webp`，省略时不发送；实际格式以 `saved[].format` 为准。
- 输入图仅支持 PNG、JPEG、WebP，单张不超过 4MB，多图参考合计不超过 8MB。

---

## 安装

本项目是单文件程序，不需要 Python 或其他运行环境。无参数运行即启动 STDIO MCP 服务。

### 一行命令安装（推荐）

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/Rousseal1984/twgga-image-mcp/main/scripts/install.ps1 | iex
```

macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/Rousseal1984/twgga-image-mcp/main/scripts/install.sh | sh
```

脚本会识别当前平台，从最新 Release 下载程序，用该 Release 的 `SHA256SUMS` 校验，再执行程序自带的 `install --yes`。安装器默认写入 Claude Code（`~/.claude.json`）和 Codex（`~/.codex/config.toml`）的 `twgga-image` 配置。脚本源码在 [`scripts/install.ps1`](scripts/install.ps1) 和 [`scripts/install.sh`](scripts/install.sh)，可以先看再运行。

首次安装时如果还没有保存过 Key，安装器会在终端里提示输入一次，输入内容不显示。只配置 Codex 时：

```bash
curl -fsSL https://raw.githubusercontent.com/Rousseal1984/twgga-image-mcp/main/scripts/install.sh | sh -s -- --no-claude
```

Claude Desktop 的普通对话模式使用单独的配置文件，安装器不会自动写入，见[使用教程](docs/MCP使用教程.md)。

### 手动下载

从 [最新 Release](https://github.com/Rousseal1984/twgga-image-mcp/releases/latest) 下载对应平台的文件，核对 `SHA256SUMS` 后运行：

```powershell
# Windows PowerShell
.\twgga-image-mcp-windows-x86_64.exe install --yes
```

```bash
# macOS / Linux
chmod +x ./twgga-image-mcp-macos-arm64
./twgga-image-mcp-macos-arm64 install --yes
```

`install` 会把程序复制到当前用户的固定位置，再让客户端指向这份副本。之后移动或删除下载的文件都不影响 MCP。

| 平台 | 安装位置 |
|---|---|
| Windows | `%LOCALAPPDATA%\twgga-image-mcp\bin\twgga-image-mcp.exe` |
| macOS | `~/Library/Application Support/twgga-image-mcp/bin/twgga-image-mcp` |
| Linux | `~/.local/share/twgga-image-mcp/bin/twgga-image-mcp` |

常用命令：

```bash
twgga-image-mcp                       # 等同 serve，STDIO MCP
twgga-image-mcp install --yes --no-claude
twgga-image-mcp reset --yes           # 只删除 twgga-image 这一节配置
twgga-image-mcp doctor                # 检查配置与运行条件
twgga-image-mcp credentials set       # 更换 Key，隐藏输入
twgga-image-mcp credentials authorize # macOS 授权已有 Key
twgga-image-mcp version
```

---

## API Key 保存在哪

Key 不会写进 Claude Code、Codex 或 Desktop 的配置文件，而是保存在系统自带的安全凭据库里，MCP 每次启动时自己读取：

| 平台 | 保存位置 |
|---|---|
| Windows | 凭据管理器（控制面板 → 凭据管理器 → Windows 凭据），条目名 `image2-api-key.twgga-image-mcp` |
| macOS | 钥匙串，service=`twgga-image-mcp`、account=`image2-api-key` |
| Linux | Secret Service |

- 安装器先看环境变量 `TWGGA_API_KEY`，再看系统凭据库；都没有时在终端提示输入一次。Key 需以 `sk-` 开头。
- 旧版配置里如果有明文 `TWGGA_API_KEY`，安装器会先把它存进凭据库，再从配置里删掉。
- 环境变量 `TWGGA_API_KEY` 非空时优先使用它，不访问凭据库。
- 更换 Key：运行安装位置下的 `twgga-image-mcp credentials set`，不改客户端配置。
- macOS 后台启动只做无交互读取，钥匙串锁定或未授权时不会弹窗。MCP 照常连接，`server_info.credentials.status` 会说明状态，生图时会提示运行 `credentials authorize`。

---

## 升级

已安装的 MCP 不会自动更新。重新运行上面的一行安装命令即可，已保存的 Key 不用重新输入。手动下载的方式同样是用新程序再跑一次 `install --yes`。

完成后重新连接 MCP，调用 `server_info` 确认 `version` 已是新版本。客户端启动的是安装位置下的那份程序，只替换下载的文件不会生效。

---

## 配置

| 变量 | 默认值 | 说明 |
|---|---|---|
| `TWGGA_API_KEY` | 空 | 设置后优先于系统凭据库 |
| `TWGGA_BASEURL` | `https://twgga.work` | 接口地址，启动后不可通过工具参数修改 |
| `TWGGA_MODEL` | `gpt-image-2` | 默认模型 |
| `TWGGA_SAVE_DIR` | `~/Pictures/twgga-out` | 默认输出目录 |
| `TWGGA_SAVE_DIR_ROOT` | 同输出目录 | 输出安全根目录；会话记录保存在它下面的 `.twgga-sessions` |
| `TWGGA_INPUT_ROOT` | 空（不限制） | 可选的输入图片白名单根目录 |
| `TWGGA_KEYCHAIN_SERVICE` / `TWGGA_KEYCHAIN_ACCOUNT` | `twgga-image-mcp` / `image2-api-key` | 自定义凭据条目 |
| `TWGGA_USE_SHELL_PROXY` | `0` | 设为 `1` 才读取终端代理设置 |
| `TWGGA_RESPONSE_FORMAT` | `auto` | `auto`（先 url，失败改 b64_json）、`url` 或 `b64_json` |
| `TWGGA_ALLOW_FAKE_IP_DOWNLOAD` | `1` | 仅可信下载域名可放行 198.18.0.0/15 fake-ip |

手动配置 Claude Code 时，`command` 指向安装位置下的程序，`env` 里不要放 Key：

```json
{
  "mcpServers": {
    "twgga-image": {
      "command": "C:\\Users\\you\\AppData\\Local\\twgga-image-mcp\\bin\\twgga-image-mcp.exe",
      "args": [],
      "env": {
        "TWGGA_SAVE_DIR": "C:\\Users\\you\\Pictures\\twgga-out",
        "TWGGA_SAVE_DIR_ROOT": "C:\\Users\\you\\Pictures\\twgga-out"
      }
    }
  }
}
```

Codex 的 `~/.codex/config.toml` 同理，写在 `[mcp_servers.twgga-image]` 和 `[mcp_servers.twgga-image.env]` 下。不要手工把 Windows 路径拼进 TOML 字符串，优先用安装器写入。

---

## 开发与验证

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
cargo xtask smoke
```

离线测试全部使用本地 HTTP 替身，不读取个人 Key，不产生图片费用。付费在线验收需要显式开关，见 [Rust 开发与验收](docs/rust-only-development.md)。

本项目跟随开源项目 [micu-image-mcp](https://github.com/Subaru486desuwa/micu-image-mcp)（MIT）同步，在其之上保留本站的差异：只有 `gpt-image-2` 一个模型，不按尺寸切换线路。
