#!/bin/bash
# Heritage Builder — MacOS App Bundle
#
# Usage:
#   ./bundle.sh                    Build the .app bundle (debug)
#   ./bundle.sh release            Build the .app bundle (release)
#   ./bundle.sh --install          Install cargo-bundle, then build (debug)
#   ./bundle.sh --install release  Install cargo-bundle, then build (release)

set -e

INSTALL=false
MODE="debug"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --install) INSTALL=true; shift ;;
        release)   MODE="release"; shift ;;
        debug)     MODE="debug"; shift ;;
        *)         echo "Unknown argument: $1"; echo "Usage: ./bundle.sh [--install] [debug|release]"; exit 1 ;;
    esac
done

if $INSTALL; then
    echo "📦 Installing cargo-bundle..."
    cargo install cargo-bundle
fi

echo "📦 Building MacOS app bundle ($MODE)..."
cd crates/launcher
cargo run -p bundler -- "$MODE"
