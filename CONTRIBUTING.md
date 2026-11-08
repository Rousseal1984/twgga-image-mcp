# Contributing

`main` is the only maintained Rust implementation. Keep product code, tests and engineering tools in Rust; retain necessary Shell/PowerShell platform bootstrap scripts. Main must contain no Python runtime, packaging, interpreter-driven tests or inline Python/JavaScript workflows. Static historical fixtures may retain their original names.

Preserve the five MCP tools and public schemas, persistent sessions, client configuration and credential/path/image/response limits. Never print or persist API credentials. Offline acceptance uses isolated child processes and local HTTP fixtures; paid image or model validation requires explicit authorization and the corresponding opt-in switches.

Run formatting, workspace Clippy, build and the complete Rust tests as documented in [Rust-only development](docs/rust-only-development.md). Black-box tests must execute the actual Cargo-built server; missing binaries are failures, not skips. Validate OS-specific behavior on native runners.

Only release the root Rust package from main. Version tags must match Cargo.toml and docs/releases/<tag>.md. Assets remain the four native binaries and SHA256SUMS. Do not publish legacy packages or delete historical tags/releases. Manual release workflow runs are build-only.

This repository tracks the open-source micu-image-mcp and keeps TWGGA's own differences on top: one image model (gpt-image), no route switching by size, and no wording that exposes how the service is built. When syncing a newer upstream release, merge it three-way against the upstream version this tree was last synced from, then re-check the tool and server_info contracts in `src/contracts/` for claims that are only true for the upstream service.
