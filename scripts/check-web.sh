#!/usr/bin/env bash
# Builds the web bundle and runs strict Clippy for the web target, as CI does.
# Run with: nix develop .#web --command ./scripts/check-web.sh
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
source scripts/build-web.sh
cargo clippy --locked -p fatherload_game --target wasm32-unknown-emscripten -- -D warnings
