#!/bin/bash

# ==============================
# MacOS heap corruption debugger
# ==============================

# Usage:
#   ./run_heap_debug.sh path/to/your/binary [args...]

BIN="$1"
shift

if [ -z "$BIN" ]; then
  echo "Usage: $0 <binary> [args...]"
  exit 1
fi

echo "=== MacOS Heap Debugging Enabled ==="
echo

# Disable nano allocator (gives clearer metadata for small allocations)
export MallocNanoZone=0
echo "MallocNanoZone=0"

# Scribble freed memory with 0x55/0xAA (finds UAF faster)
export MallocScribble=1
echo "MallocScribble=1"

# Log allocation stack traces. This lets you run malloc_history later.
export MallocStackLogging=1
echo "MallocStackLogging=1"

# Extra debug switches, slower but might catch more bugs.
export MallocGuardEdges=1
echo "MallocGuardEdges=1"

export MallocCheckHeapStart=1
echo "MallocCheckHeapStart=1"

# Insert GuardMalloc (heavy but extremely good at catching OOB writes)
# Comment this out if it makes your program too slow:
export DYLD_INSERT_LIBRARIES=/usr/lib/libgmalloc.dylib
echo "DYLD_INSERT_LIBRARIES=/usr/lib/libgmalloc.dylib"

echo
echo ">>> Running: $BIN $@"
echo ">>> When it crashes, note the corrupted pointer printed by malloc"
echo ">>> Then run:"
echo "      malloc_history <PID> <hex-address>"
echo

# Run with `exec` so crash output is not swallowed
exec "$BIN" "$@"
