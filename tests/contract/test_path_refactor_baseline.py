from __future__ import annotations

import json
import os
import sys
from pathlib import Path

import pytest

from tests.contract.capture_python_fixtures import collect
from tests.contract.contract_cases import REPO_ROOT, case_names, run_case
from tests.contract.differential import assert_equal, normalize, normalize_stdio


BEFORE = REPO_ROOT / "tests" / "fixtures"
RUST_BINARY = REPO_ROOT / "target" / "debug" / (
    "twgga-image-mcp.exe" if sys.platform == "win32" else "twgga-image-mcp"
)


def _before(name: str) -> object:
    return json.loads((BEFORE / name).read_text(encoding="utf-8"))


# tests/fixtures 里的基线是在 POSIX 机器上抓的，其中一条用例把客户端传来的
# "/definitely/missing.png" 解析后回显进报错文案。那串字符没有跨平台的等价物：
# 在 Windows 上同一个输入会解析成当前盘符下的 "W:\definitely\missing.png"。
# 这不是实现分歧，是同一个逻辑路径在两个平台上的固有渲染差异，归一化解决不了。
#
# 因此在 Windows 上把这一条从两侧一起摘掉，其余断言照常严格比对——摘掉整个
# 文件或整条测试会顺带放掉真正该守的 schema 与校验契约。
_PLATFORM_DEPENDENT_CASES = ("edit_missing_image",) if sys.platform == "win32" else ()


def _drop_platform_dependent(name: str, value: object) -> object:
    if name != "validation-calls.json" or not _PLATFORM_DEPENDENT_CASES:
        return value
    if not isinstance(value, dict):
        return value
    return {k: v for k, v in value.items() if k not in _PLATFORM_DEPENDENT_CASES}


@pytest.mark.skipif(not RUST_BINARY.is_file(), reason="cargo build is required")
def test_path_refactor_preserves_initialize_tools_schema_validation_and_public_server_info() -> None:
    current = collect([str(RUST_BINARY)])
    for current_name, baseline_name in (
        ("initialize-2024-11-05.json", "initialize-before-path-refactor.json"),
        ("tools-list.json", "tools-list-before-path-refactor.json"),
        ("validation-calls.json", "validation-calls-before-path-refactor.json"),
    ):
        expected = _drop_platform_dependent(current_name, _before(baseline_name))
        actual = _drop_platform_dependent(current_name, current[current_name])
        if current_name == "initialize-2024-11-05.json":
            expected = normalize_stdio(current_name, expected)
            actual = normalize_stdio(current_name, actual)
        assert_equal(expected, actual, current_name)

    # Only the two explicitly requested runtime path descriptions may change. Keys, field types,
    # and all other server_info values remain exact.
    expected_info = normalize_stdio(
        "server-info.json", _before("server-info-before-path-refactor.json")
    )
    current_info = normalize_stdio("server-info.json", current["server-info.json"])
    for value in (expected_info, current_info):
        value["result"]["structuredContent"]["version"] = "<PROJECT_VERSION>"
        value["result"]["content"][0]["text"]["version"] = "<PROJECT_VERSION>"
    assert_equal(expected_info, current_info, "server_info path refactor")


@pytest.mark.skipif(
    os.environ.get("TWGGA_RUN_CONTRACT_TESTS") != "1",
    reason="set TWGGA_RUN_CONTRACT_TESTS=1 to compare all 36 before/after mock cases",
)
def test_path_refactor_preserves_all_mock_http_multipart_retry_and_output_cases() -> None:
    assert RUST_BINARY.is_file(), f"build Rust first: {RUST_BINARY}"
    baseline = _before("mock-cases-before-path-refactor.json")
    assert isinstance(baseline, dict)
    assert set(baseline).issubset(case_names())
    for name in baseline:
        assert_equal(
            normalize(baseline[name]),
            normalize(run_case([str(RUST_BINARY)], name)),
            f"path refactor before/after case {name}",
        )
