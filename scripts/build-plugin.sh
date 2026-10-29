#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p dist
cargo build --locked --release --target wasm32-wasip2 --manifest-path plugin/Cargo.toml
plugin_target="${CARGO_TARGET_DIR:-plugin/target}"
cargo run --locked --manifest-path tools/Cargo.toml -- \
  "$plugin_target/wasm32-wasip2/release/anistream_ani_cli.wasm" dist/ani-cli.wasm "$@"
shasum -a 256 dist/ani-cli.wasm > dist/ani-cli.wasm.sha256
