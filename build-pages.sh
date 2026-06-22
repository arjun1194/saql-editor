#!/usr/bin/env bash
# Rebuild the WebAssembly engine and refresh the static GitHub Pages bundle.
#
#   ./build-pages.sh
#
# docs/index.html, docs/app.js, docs/style.css are hand-written and committed.
# This script (re)generates docs/pkg/{saql_wasm.js, saql_wasm_bg.wasm} — the
# compiled engine the page loads. Push docs/ to GitHub Pages to deploy.
#
# One-time prerequisites:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.125   # must match the crate
#   the engine repo checked out as a sibling ../saql (the wasm crate path-deps to it)
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"
BINDGEN="${WASM_BINDGEN:-$HOME/.cargo/bin/wasm-bindgen}"
command -v "$CARGO" >/dev/null 2>&1 || CARGO="cargo"
command -v "$BINDGEN" >/dev/null 2>&1 || BINDGEN="wasm-bindgen"

# Build inside wasm/ so .cargo/config.toml (the getrandom backend cfg) applies.
echo "→ building saql-wasm (release, wasm32-unknown-unknown)…"
( cd "$DIR/wasm" && "$CARGO" build --release --target wasm32-unknown-unknown )

echo "→ generating browser bindings into docs/pkg…"
"$BINDGEN" --target web --no-typescript \
  --out-dir "$DIR/docs/pkg" --out-name saql_wasm \
  "$DIR/wasm/target/wasm32-unknown-unknown/release/saql_wasm.wasm"

echo "done — docs/pkg refreshed:"
ls -lh "$DIR/docs/pkg" | awk 'NR>1{print "    "$9"  "$5}'
