"""Guard the HEAD-era Python reference fixtures against accidental drift."""
from __future__ import annotations

import json
import sys
from pathlib import Path

from tests.contract.capture_python_fixtures import FIXTURE_ROOT, REPO_ROOT, collect


# 冻结的基线以 CI 所在的 POSIX 为准。其中一条用例把解析后的输入路径回显进报错，
# 而 "/definitely/missing.png" 在 Windows 上会解析成当前盘符下的路径 ——
# 同一个逻辑输入的两种平台渲染，没有共同的期望值，归一化也解决不了。
# 因此在 Windows 上只摘掉这一条，其余仍然严格比对。
_PLATFORM_DEPENDENT = (
    {"validation-calls.json": ("edit_missing_image",)} if sys.platform == "win32" else {}
)


def _drop_platform_dependent(name: str, value: object) -> object:
    keys = _PLATFORM_DEPENDENT.get(name)
    if not keys or not isinstance(value, dict):
        return value
    return {k: v for k, v in value.items() if k not in keys}


def test_python_reference_matches_frozen_stdio_contract() -> None:
    actual = collect([sys.executable, str(REPO_ROOT / "server.py")])
    for name, raw_actual in actual.items():
        actual_value = _drop_platform_dependent(name, raw_actual)
        expected = _drop_platform_dependent(
            name, json.loads((FIXTURE_ROOT / name).read_text(encoding="utf-8"))
        )
        if name == "initialize-2024-11-05.json":
            # The SDK implementation version may move from 1.28.x to 1.29.x without changing the
            # lifecycle contract. Protocol version, capabilities, server name, schemas, defaults,
            # descriptions, validations, and server_info remain exact below.
            actual_value["result"]["serverInfo"]["version"] = "<IMPLEMENTATION_VERSION>"
            expected["result"]["serverInfo"]["version"] = "<IMPLEMENTATION_VERSION>"
        assert actual_value == expected, f"Python STDIO contract drifted: {name}"
