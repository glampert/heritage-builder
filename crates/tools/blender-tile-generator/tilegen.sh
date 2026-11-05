#!/usr/bin/env bash
# Render the tile catalog, sync tile_set.json, and refresh the web build's asset list.
#   crates/tools/blender-tile-generator/tilegen.sh [TILE ...] [build.py options]
# Set BLENDER to the Blender binary if it isn't in the default place.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
blender="${BLENDER:-}"
if [[ -z "$blender" ]]; then
    for b in /Applications/Blender.app/Contents/MacOS/Blender "$(command -v blender || true)"; do
        if [[ -n "$b" && -x "$b" ]]; then blender="$b"; break; fi
    done
fi
[[ -n "$blender" ]] || { echo "tilegen.sh: Blender not found; set BLENDER=/path/to/blender" >&2; exit 2; }

status=0
"$blender" -b --factory-startup --python-exit-code 1 -P "$here/build.py" -- "$@" || status=$?
case " $* " in *" --dry-run "*|*" --no-sync "*) exit $status ;; esac
if [[ $status -le 1 ]]; then
    (cd "$root" && cargo run -q -p web-builder -- manifest)
    python3 "$here/sync_tileset.py" --check >/dev/null || { python3 "$here/sync_tileset.py" --check; status=1; }
fi
exit $status
