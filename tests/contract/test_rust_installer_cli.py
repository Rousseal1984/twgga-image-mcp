from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10
    import tomli as tomllib
from pathlib import Path

import pytest

from tests.contract.contract_cases import REPO_ROOT


RUST_BINARY = REPO_ROOT / "target" / "debug" / (
    "twgga-image-mcp.exe" if sys.platform == "win32" else "twgga-image-mcp"
)


@pytest.mark.skipif(not RUST_BINARY.is_file(), reason="cargo build is required")
def test_rust_installer_and_reset_preserve_unrelated_configuration(tmp_path: Path) -> None:
    home = tmp_path / "home"
    save_dir = tmp_path / "images"
    codex_dir = home / ".codex"
    home.mkdir()
    codex_dir.mkdir()
    claude_path = home / ".claude.json"
    codex_path = codex_dir / "config.toml"
    claude_path.write_text(
        json.dumps({"theme": "dark", "mcpServers": {"other": {"command": "other"}}}),
        encoding="utf-8",
    )
    codex_path.write_text(
        "model = 'gpt-test'\n\n[mcp_servers.other]\ncommand = 'other'\n",
        encoding="utf-8",
    )
    secret = "contract-installer-secret-key"
    env = {
        **os.environ,
        "HOME": str(home),
        "USERPROFILE": str(home),
        "TWGGA_API_KEY": secret,
        "TWGGA_SAVE_DIR": str(save_dir),
        "TWGGA_SAVE_DIR_ROOT": str(save_dir),
        "TWGGA_RUN_LIVE_TESTS": "0",
    }
    installed = subprocess.run(
        [str(RUST_BINARY), "install", "--yes"],
        env=env,
        check=False,
        capture_output=True,
        text=True, encoding="utf-8", errors="replace",
    )
    assert installed.returncode == 0, installed.stderr
    assert secret not in installed.stdout + installed.stderr
    claude = json.loads(claude_path.read_text(encoding="utf-8"))
    assert claude["theme"] == "dark"
    assert claude["mcpServers"]["other"]["command"] == "other"
    stable_binary = Path(claude["mcpServers"]["twgga-image"]["command"])
    assert stable_binary.is_file()
    assert stable_binary.name == ("twgga-image-mcp.exe" if os.name == "nt" else "twgga-image-mcp")
    assert not stable_binary.is_relative_to(REPO_ROOT / "target")
    assert claude["mcpServers"]["twgga-image"]["args"] == []
    assert "TWGGA_API_KEY" not in claude["mcpServers"]["twgga-image"]["env"]
    assert secret not in claude_path.read_text(encoding="utf-8")
    codex = codex_path.read_text(encoding="utf-8")
    assert "model = 'gpt-test'" in codex
    assert "[mcp_servers.other]" in codex
    assert "[mcp_servers.twgga-image]" in codex
    assert "[mcp_servers.twgga-image.env]" in codex
    parsed_codex = tomllib.loads(codex)
    installed_server = parsed_codex["mcp_servers"]["twgga-image"]
    assert Path(installed_server["command"]) == stable_binary
    assert installed_server["args"] == []
    assert "TWGGA_API_KEY" not in installed_server["env"]
    assert secret not in codex
    if os.name != "nt":
        assert stat.S_IMODE(claude_path.stat().st_mode) == 0o600
        assert stat.S_IMODE(codex_path.stat().st_mode) == 0o600

    doctor = subprocess.run(
        [str(RUST_BINARY), "doctor"],
        env=env,
        check=False,
        capture_output=True,
        text=True, encoding="utf-8", errors="replace",
    )
    assert doctor.returncode == 0, doctor.stderr
    assert "doctor: OK" in doctor.stderr
    assert secret not in doctor.stdout + doctor.stderr

    reset = subprocess.run(
        [str(RUST_BINARY), "reset", "--yes"],
        env=env,
        check=False,
        capture_output=True,
        text=True, encoding="utf-8", errors="replace",
    )
    assert reset.returncode == 0, reset.stderr
    claude = json.loads(claude_path.read_text(encoding="utf-8"))
    assert "twgga-image" not in claude["mcpServers"]
    assert claude["mcpServers"]["other"]["command"] == "other"
    codex = codex_path.read_text(encoding="utf-8")
    assert "[mcp_servers.other]" in codex
    assert "mcp_servers.twgga-image" not in codex
    assert list(home.glob(".claude.json.bak.*"))
    assert list(codex_dir.glob("config.toml.bak.*"))


@pytest.mark.skipif(not RUST_BINARY.is_file(), reason="cargo build is required")
def test_rust_installer_dev_mode_is_the_only_target_path_opt_out(tmp_path: Path) -> None:
    home = tmp_path / "home"
    home.mkdir()
    env = {
        **os.environ,
        "HOME": str(home),
        "USERPROFILE": str(home),
        "TWGGA_SAVE_DIR": str(tmp_path / "images"),
        "TWGGA_SAVE_DIR_ROOT": str(tmp_path / "images"),
        "TWGGA_RUN_LIVE_TESTS": "0",
    }
    installed = subprocess.run(
        [
            str(RUST_BINARY),
            "install",
            "--yes",
            "--no-claude",
            "--dev",
            "--binary-path",
            str(RUST_BINARY),
        ],
        env=env,
        check=False,
        capture_output=True,
        text=True, encoding="utf-8", errors="replace",
    )
    assert installed.returncode == 0, installed.stderr
    codex = tomllib.loads((home / ".codex" / "config.toml").read_text(encoding="utf-8"))
    server = codex["mcp_servers"]["twgga-image"]
    # Windows 上 canonicalize 给的是扩展形式 \\?\C:\...，安装器刻意原样写进配置：
    # 那是超过 MAX_PATH 的路径唯一能被打开的形式，剥掉会让长路径下的安装失效。
    # 而 Path.resolve() 并不会把它折回普通形式，所以按字符串比会假阴性。
    # 这里断言的是真正要紧的那件事——配置里那个路径指向的就是我们刚装的二进制。
    assert os.path.samefile(server["command"], RUST_BINARY)
    assert server["args"] == []
