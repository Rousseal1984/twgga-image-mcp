#!/bin/sh
set -eu

if [ "$(/usr/bin/uname -s)" != Darwin ]; then
  echo 'twgga-image MCP: this launcher requires macOS' >&2
  exit 1
fi

rust_binary=${TWGGA_MCP_BINARY:-"$HOME/Library/Application Support/twgga-image-mcp/bin/twgga-image-mcp"}
if [ ! -x "$rust_binary" ]; then
  echo 'twgga-image MCP: install the Rust binary or set TWGGA_MCP_BINARY' >&2
  exit 1
fi
# The Rust configuration loader reads Keychain without exposing the key to this shell.
exec "$rust_binary" serve
