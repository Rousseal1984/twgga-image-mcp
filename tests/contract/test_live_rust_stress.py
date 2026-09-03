from __future__ import annotations

import json
import os
import sys
import tempfile
import time
from pathlib import Path

import pytest

from tests.contract.contract_cases import REPO_ROOT
from tests.contract.stdio_driver import StdioSession, text_content_json


RUST_BINARY = REPO_ROOT / "target" / "release" / (
    "twgga-image-mcp.exe" if sys.platform == "win32" else "twgga-image-mcp"
)


@pytest.mark.skipif(
    not (
        os.environ.get("TWGGA_RUN_LIVE_TESTS") == "1"
        and os.environ.get("TWGGA_RUN_LIVE_STRESS") == "1"
    ),
    reason="paid live stress requires TWGGA_RUN_LIVE_TESTS=1 and TWGGA_RUN_LIVE_STRESS=1",
)
def test_real_rust_standard_route_five_way_concurrency() -> None:
    assert RUST_BINARY.is_file(), f"release binary missing: {RUST_BINARY}"
    with tempfile.TemporaryDirectory(prefix="twgga-rust-live-stress-") as temp_dir:
        save_root = Path(temp_dir).resolve()
        env = os.environ.copy()
        env.update(
            {
                "TWGGA_BASEURL": "https://twgga.work",
                "TWGGA_SAVE_DIR": str(save_root),
                "TWGGA_SAVE_DIR_ROOT": str(save_root),
                "TWGGA_MODEL": "gpt-image-2",
                "TWGGA_RUN_LIVE_TESTS": "1",
                "TWGGA_RUN_LIVE_STRESS": "1",
            }
        )
        started = time.perf_counter()
        with StdioSession([str(RUST_BINARY)], env, REPO_ROOT, timeout=900) as session:
            session.initialize("2024-11-05")
            response = session.request(
                2,
                "tools/call",
                {
                    "name": "image_generate",
                    "arguments": {
                        "prompt": (
                            "A minimal blue circle centered on a white background, "
                            "clean flat vector test image"
                        ),
                        "size": "1024x1024",
                        "n": 5,
                        "model": "gpt-image-2",
                        "basename": "live_rust_stress",
                    },
                },
            )
        wall_seconds = round(time.perf_counter() - started, 3)
        result = text_content_json(response)
        assert result["ok"] is True, result
        assert result["requested_n"] == 5
        assert result["errors"] == []
        assert len(result["saved"]) == 5
        paths = [Path(item["path"]) for item in result["saved"]]
        assert len(set(paths)) == 5
        assert all(path.is_file() and path.is_relative_to(save_root) for path in paths)
        assert all(item["size_bytes"] > 0 for item in result["saved"])

        report_path = os.environ.get("TWGGA_LIVE_STRESS_REPORT")
        if report_path:
            sanitized = {
                "ok": result["ok"],
                "model": result["model"],
                "size": result["size"],
                "requested_n": result["requested_n"],
                "saved": [
                    {
                        "size_bytes": item["size_bytes"],
                        "actual_size": item["actual_size"],
                        "actual_megapixels": item["actual_megapixels"],
                    }
                    for item in result["saved"]
                ],
                "errors": result["errors"],
                "notes": result.get("notes", []),
                "wall_seconds": wall_seconds,
            }
            Path(report_path).write_text(
                json.dumps(sanitized, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )
