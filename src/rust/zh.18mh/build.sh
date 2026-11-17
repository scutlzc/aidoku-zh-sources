#!/bin/bash
set -e

cargo build --release --target wasm32-unknown-unknown

out=target/wasm32-unknown-unknown/release
mkdir -p "$out/Payload"
cp res/* "$out/Payload"
cp "$out"/*.wasm "$out/Payload/main.wasm"
rm -f ./package.aix
(cd "$out" && zip -qr package.aix Payload)
mv "$out/package.aix" ./package.aix
