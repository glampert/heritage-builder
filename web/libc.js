// C standard library shims for imgui-sys compiled to WASM.
//
// imgui-sys (Dear ImGui C++) imports these functions from the "env" module.
// Most are not actually called at runtime — imgui's file I/O, printf, and
// assert are unused in our configuration. The critical ones are malloc/free
// (memory allocation) and string/memory functions.
//
// Memory and instance references are set by bootstrap.js via setWasm()
// before the WASM main() is called.

let wasm = null;

// Called by bootstrap.js after WASM instantiation.
export function setWasm(exports) {
    wasm = exports;
}

function mem() {
    return new Uint8Array(wasm.memory.buffer);
}

// -----------------------------------------------
// Memory allocation — uses c_malloc/c_free exported
// from Rust (see game/src/wasm/libc.rs).
// These pair correctly with Rust's global allocator.
// -----------------------------------------------

export function malloc(size) {
    return wasm.c_malloc(size);
}

export function free(ptr) {
    wasm.c_free(ptr);
}

// -----------------------------------------------
// String / memory functions
// -----------------------------------------------

export function memchr(ptr, value, num) {
    const m = mem();
    const needle = value & 0xFF;
    for (let i = 0; i < num; i++) {
        if (m[ptr + i] === needle) return ptr + i;
    }
    return 0;
}

export function strncpy(dst, src, num) {
    const m = mem();
    let i = 0;
    for (; i < num; i++) {
        const b = m[src + i];
        m[dst + i] = b;
        if (b === 0) break;
    }
    for (; i < num; i++) {
        m[dst + i] = 0;
    }
    return dst;
}

export function strcmp(s1, s2) {
    const m = mem();
    let i = 0;
    while (true) {
        const a = m[s1 + i];
        const b = m[s2 + i];
        if (a !== b) return a < b ? -1 : 1;
        if (a === 0) return 0;
        i++;
    }
}

export function strncmp(s1, s2, n) {
    const m = mem();
    for (let i = 0; i < n; i++) {
        const a = m[s1 + i];
        const b = m[s2 + i];
        if (a !== b) return a < b ? -1 : 1;
        if (a === 0) return 0;
    }
    return 0;
}

export function strstr(haystack, needle) {
    const m = mem();
    const needleBytes = [];
    for (let i = 0; ; i++) {
        const b = m[needle + i];
        if (b === 0) break;
        needleBytes.push(b);
    }
    if (needleBytes.length === 0) return haystack;

    outer:
    for (let i = 0; ; i++) {
        if (m[haystack + i] === 0) return 0;
        for (let j = 0; j < needleBytes.length; j++) {
            if (m[haystack + i + j] !== needleBytes[j]) continue outer;
        }
        return haystack + i;
    }
}

// -----------------------------------------------
// Formatting / I/O — stubs
// -----------------------------------------------

export function printf(_fmt) { return 0; } // No stdout.

export function vsnprintf(buf, size, fmt, va_list) {
    return wasm.c_vsnprintf(buf, size, fmt, va_list);
}

export function sscanf(str, fmt, ...args) {
    if (args.length !== 1) {
        // We only have to handle one output argument for ImGui.
        throw new Error(`sscanf: expected 1 output argument, got ${args.length}`);
    }
    return wasm.c_sscanf(str, fmt, args[0]);
}

export function atof(strPtr) {
    const m = mem();
    let s = "";
    for (let i = 0; m[strPtr + i] !== 0 && i < 64; i++) {
        s += String.fromCharCode(m[strPtr + i]);
    }
    return parseFloat(s) || 0.0;
}

// -----------------------------------------------
// File I/O — stubs (never called)
// -----------------------------------------------

export function fopen()  { console.error("Called unsupported fopen()!");  return 0;  }
export function fclose() { console.error("Called unsupported fclose()!"); return 0;  }
export function fread()  { console.error("Called unsupported fread()!");  return 0;  }
export function fwrite() { console.error("Called unsupported fwrite()!"); return 0;  }
export function fseek()  { console.error("Called unsupported fseek()!");  return -1; }
export function ftell()  { console.error("Called unsupported ftell()!");  return -1; }
export function fflush() { console.error("Called unsupported fflush()!"); return 0;  }

// -----------------------------------------------
// qsort — delegates to Rust (needs function table
// access to invoke the C comparator callback)
// -----------------------------------------------

export function qsort(base, nmemb, size, compar) {
    wasm.c_qsort(base, nmemb, size, compar);
}

// -----------------------------------------------
// Assert
// -----------------------------------------------

export function __assert_fail(assertion, file, line) {
    const m = mem();
    const readStr = (ptr) => {
        let s = "";
        for (let i = 0; m[ptr + i] !== 0 && i < 256; i++) {
            s += String.fromCharCode(m[ptr + i]);
        }
        return s;
    };
    console.error(`Assertion failed: ${readStr(assertion)} at ${readStr(file)}:${line}`);
    throw new Error("C assertion failed!");
}
