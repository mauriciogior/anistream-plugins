#!/usr/bin/env bash
# Anistream's official store journey, scoped to a disposable local Worker only.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
scratch="$(mktemp -d)"
port="${ANI_TEST_PORT:-8789}"
server=""
cleanup() {
  if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; fi
  rm -rf "$scratch"
}
trap cleanup EXIT
[[ -f "$root/dist/ani-cli.wasm" ]] || { echo 'Build the plugin first.' >&2; exit 1; }
code="local-$(openssl rand -hex 16)"
(cd "$root/store"; exec ./node_modules/.bin/wrangler dev --local --port "$port" --persist-to "$scratch/state" --var "ANISTREAM_STORE_SETUP_CODE:$code") > "$scratch/worker.log" 2>&1 &
server=$!
ready=false
for ((attempt = 0; attempt < 80; attempt++)); do
  if curl -fsS "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; then ready=true; break; fi
  kill -0 "$server" 2>/dev/null || { cat "$scratch/worker.log"; exit 1; }
  sleep 0.25
done
[[ "$ready" == true ]] || { cat "$scratch/worker.log"; exit 1; }
ANISTREAM_TEST_STORE="http://127.0.0.1:$port" ANISTREAM_TEST_SETUP_CODE="$code" ANISTREAM_TEST_PLUGIN="$root/dist/ani-cli.wasm" \
  cargo test --locked --manifest-path "$root/tools/Cargo.toml" --test store -- --nocapture
