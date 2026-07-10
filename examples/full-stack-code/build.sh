#!/usr/bin/env bash
# Build the WASM frontend into static/pkg, then build the backend.
# Run from anywhere; paths are resolved relative to this script.
set -euo pipefail

cd "$(dirname "$0")"

echo "==> Building WASM frontend (wasm-pack, --target web)"
( cd frontend && wasm-pack build --target web --out-dir ../static/pkg --no-typescript )

echo "==> Building native backend"
cargo build -p backend

echo "==> Done. Start the server with:"
echo "    cargo run -p backend"
echo "    # then open http://127.0.0.1:3000"
