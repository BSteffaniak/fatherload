#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
command -v emcc >/dev/null || { echo 'Run with: nix develop .#web --command bash scripts/build-web.sh' >&2; exit 1; }
export EM_CACHE="${EM_CACHE:-$PWD/target/emscripten-cache}"
export EMCC_CFLAGS='-O2 -sUSE_GLFW=3 -sGL_ENABLE_GET_PROC_ADDRESS=1'
export CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER=emcc
export CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_RUSTFLAGS="-C link-arg=-sUSE_GLFW=3 -C link-arg=-sALLOW_MEMORY_GROWTH=1 -C link-arg=-sSTACK_SIZE=8388608 -C link-arg=-sASYNCIFY -C link-arg=-sASYNCIFY_STACK_SIZE=1048576 -C link-arg=-sFORCE_FILESYSTEM=1 -C link-arg=-lidbfs.js -C link-arg=-sEXPORTED_RUNTIME_METHODS=callMain -C link-arg=--pre-js -C link-arg=$PWD/web/bootstrap.js"
unset SDKROOT MACOSX_DEPLOYMENT_TARGET RUSTFLAGS CFLAGS CXXFLAGS LDFLAGS BINDGEN_EXTRA_CLANG_ARGS
export CMAKE_TOOLCHAIN_FILE="$(em-config EMSCRIPTEN_ROOT)/cmake/Modules/Platform/Emscripten.cmake"
export CC_wasm32_unknown_emscripten=emcc
export CXX_wasm32_unknown_emscripten=em++
export AR_wasm32_unknown_emscripten=emar
export BINDGEN_EXTRA_CLANG_ARGS="--sysroot=$EM_CACHE/sysroot"
cargo build --locked --release -p fatherload_game --target wasm32-unknown-emscripten
mkdir -p target/web
cp target/wasm32-unknown-emscripten/release/fatherload.js target/wasm32-unknown-emscripten/release/fatherload.wasm target/web/
cp web/index.html target/web/index.html
printf '\nServe with: python3 -m http.server 8080 --directory target/web\n'
