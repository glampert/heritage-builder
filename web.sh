#!/bin/bash
# Heritage Builder — Web Build & Serve
#
# Usage:
#   ./web.sh --build              Build the web WASM binary (debug)
#   ./web.sh --build release      Build the web WASM binary (release + wasm-opt)
#   ./web.sh --serve              Start a local dev server on port 8080
#   ./web.sh --build --serve      Build, then serve

set -e

BUILD=false
SERVE=false
MODE="debug"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --build)  BUILD=true; shift ;;
        --serve)  SERVE=true; shift ;;
        release)  MODE="release"; shift ;;
        debug)    MODE="debug"; shift ;;
        *)        echo "Unknown argument: $1"; echo "Usage: ./web.sh --build [debug|release] --serve"; exit 1 ;;
    esac
done

if ! $BUILD && ! $SERVE; then
    echo "Usage: ./web.sh --build [debug|release] --serve"
    exit 1
fi

if $BUILD; then
    cargo run -p web-builder -- "$MODE"
fi

if $SERVE; then
    echo ""
    echo "Serving at http://localhost:8080"
    cd web && python3 -m http.server 8080
fi
