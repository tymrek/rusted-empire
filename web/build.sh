#!/bin/sh
# Builds the browser version into www/ (open www/index.html through a web server).
#
# Needs: rustup target add wasm32-unknown-unknown
#        cargo install wasm-bindgen-cli --version 0.2.117   (must match Cargo.toml)
# Extra cargo flags can be passed in CARGO_WASM_FLAGS.
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown $CARGO_WASM_FLAGS
wasm-bindgen --target web --no-typescript --out-dir www/pkg \
    target/wasm32-unknown-unknown/release/rs_empire_web.wasm
echo "Built: $(du -h www/pkg/rs_empire_web_bg.wasm | cut -f1) of WebAssembly in web/www/"
