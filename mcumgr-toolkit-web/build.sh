#!/bin/bash
set -e

if ! command -v wasm-pack &> /dev/null; then
    echo "wasm-pack not found. Install with: cargo install wasm-pack"
    exit 1
fi

wasm-pack build --target web --out-dir pkg .

echo ""
echo "Build complete! To serve:"
echo "  cd mcumgr-toolkit-web && python3 -m http.server 8080"
echo "  Then open http://localhost:8080 in Chrome/Edge"
