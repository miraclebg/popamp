#!/bin/sh
# Builds the test plug-in and puts its module next to its manifest.
# Needs: rustup target add wasm32-unknown-unknown
set -e
here=$(cd "$(dirname "$0")" && pwd)
cargo build --release --target wasm32-unknown-unknown --manifest-path "$here/test-plugin/Cargo.toml"
cp "$here/test-plugin/target/wasm32-unknown-unknown/release/popamp_test_plugin.wasm" "$here/fixtures/test/plugin.wasm"
