#!/bin/sh
# Builds the browser version into www/ (open www/index.html through a web server).
#
# Needs: rustup target add wasm32-unknown-unknown
#        cargo install wasm-bindgen-cli --version 0.2.117   (must match Cargo.toml)
# Extra cargo flags can be passed in CARGO_WASM_FLAGS.
set -e
cd "$(dirname "$0")"
# The commit being built, shown in the page's menu so a build can be identified
# ("82da896, 2026-11-02"; "+" if there are uncommitted changes).
if [ -z "$RS_EMPIRE_BUILD" ] && git rev-parse --git-dir >/dev/null 2>&1; then
    RS_EMPIRE_BUILD="$(git log -1 --format='%h, %cs')"
    git diff --quiet HEAD -- .. 2>/dev/null || RS_EMPIRE_BUILD="$RS_EMPIRE_BUILD +"
    export RS_EMPIRE_BUILD
fi
cargo build --release --target wasm32-unknown-unknown $CARGO_WASM_FLAGS
wasm-bindgen --target web --no-typescript --out-dir www/pkg \
    target/wasm32-unknown-unknown/release/rs_empire_web.wasm
echo "Built: $(du -h www/pkg/rs_empire_web_bg.wasm | cut -f1) of WebAssembly in web/www/"
