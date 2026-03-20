// C standard library functions for imgui-sys on WASM.
//
// Exported as `c_malloc`, `c_free`, `c_qsort`, `c_vsnprintf`, `c_sscanf`
// so that web/env.js can forward to them. See mod.rs for the full
// explanation of how the JS <-> Rust shim layer works.

use std::collections::HashMap;
use std::cell::RefCell;

// ----------------------------------------------
// Memory allocation with size tracking
// ----------------------------------------------
// C's free(ptr) doesn't take a size, but Rust's dealloc needs one.
// We track allocation sizes in a thread-local map.

thread_local! {
    static ALLOC_SIZES: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

#[unsafe(no_mangle)]
pub extern "C" fn c_malloc(size: usize) -> usize {
    if size == 0 { return 0; }
    let layout = unsafe { std::alloc::Layout::from_size_align_unchecked(size, 8) };
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() { return 0; }
    let addr = ptr as usize;
    ALLOC_SIZES.with(|map| map.borrow_mut().insert(addr, size));
    addr
}

#[unsafe(no_mangle)]
pub extern "C" fn c_free(ptr: usize) {
    if ptr == 0 { return; }
    let size = ALLOC_SIZES.with(|map| map.borrow_mut().remove(&ptr));
    if let Some(size) = size {
        let layout = unsafe { std::alloc::Layout::from_size_align_unchecked(size, 8) };
        unsafe { std::alloc::dealloc(ptr as *mut u8, layout); }
    }
}

// ----------------------------------------------
// qsort
// ----------------------------------------------
// Implemented in Rust because it needs to invoke the comparator via
// function pointer, which requires access to the WASM indirect call
// table (not accessible from JS).

type QsortCmp = unsafe extern "C" fn(*const u8, *const u8) -> i32;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn c_qsort(base: *mut u8,
                                 nmemb: usize,
                                 size: usize,
                                 compar: QsortCmp)
{
    if nmemb <= 1 || size == 0 { return; }

    // Create index array, sort indices by comparator, then permute in place.
    let mut indices: Vec<usize> = (0..nmemb).collect();
    indices.sort_by(|&a, &b| {
        let pa = unsafe { base.add(a * size) };
        let pb = unsafe { base.add(b * size) };
        let result = unsafe { compar(pa, pb) };
        result.cmp(&0)
    });

    // Permute elements in place using a temp buffer.
    let mut tmp = vec![0u8; size];
    let mut done = vec![false; nmemb];
    for i in 0..nmemb {
        if done[i] || indices[i] == i { done[i] = true; continue; }
        // Follow the permutation cycle.
        let mut j = i;
        unsafe {
            std::ptr::copy_nonoverlapping(base.add(i * size), tmp.as_mut_ptr(), size);
            loop {
                let k = indices[j];
                done[j] = true;
                if k == i {
                    std::ptr::copy_nonoverlapping(tmp.as_ptr(), base.add(j * size), size);
                    break;
                }
                std::ptr::copy_nonoverlapping(base.add(k * size), base.add(j * size), size);
                j = k;
            }
        }
    }
}

// ----------------------------------------------
// vsnprintf / sscanf
// ----------------------------------------------
// Implemented in Rust because they need direct access to WASM memory
// for reading va_list arguments and writing output buffers.
//
// On wasm32 (clang ABI), va_list is a void* pointing directly to the
// variadic argument area. Arguments are laid out sequentially:
//   - i32/pointers: 4-byte aligned, 4 bytes
//   - f64 (float promoted to double in varargs): 8-byte aligned, 8 bytes

/// Length of a null-terminated C string.
unsafe fn c_str_len(ptr: *const u8) -> usize {
    unsafe {
        let mut len = 0;
        while *ptr.add(len) != 0 { len += 1; }
        len
    }
}

/// Read a null-terminated C string as a byte slice.
unsafe fn c_str_bytes<'a>(ptr: *const u8) -> &'a [u8] {
    unsafe { std::slice::from_raw_parts(ptr, c_str_len(ptr)) }
}

// -- Output buffer writer --
// Tracks total logical length even when the buffer is full,
// matching C's vsnprintf semantics (returns what *would* have been written).

struct BufWriter {
    buf: *mut u8,
    cap: usize, // max bytes to write (excluding null terminator)
    len: usize, // total logical bytes written
}

impl BufWriter {
    fn new(buf: *mut u8, size: usize) -> Self {
        Self { buf, cap: if size > 0 { size - 1 } else { 0 }, len: 0 }
    }

    fn push(&mut self, byte: u8) {
        if self.len < self.cap {
            unsafe { *self.buf.add(self.len) = byte; }
        }
        self.len += 1;
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes { self.push(b); }
    }

    /// Null-terminate and return the logical length (what C vsnprintf returns).
    fn finish(self) -> i32 {
        let null_pos = self.len.min(self.cap);
        unsafe { *self.buf.add(null_pos) = 0; }
        self.len as i32
    }
}

// -- va_list argument reader (wasm32 ABI) --

struct VaReader {
    ptr: *const u8,
}

impl VaReader {
    fn new(ptr: *const u8) -> Self { Self { ptr } }

    unsafe fn align_to(&mut self, align: usize) {
        let addr = self.ptr as usize;
        self.ptr = ((addr + align - 1) & !(align - 1)) as *const u8;
    }

    unsafe fn read_i32(&mut self) -> i32 {
        unsafe {
            self.align_to(4);
            let val = (self.ptr as *const i32).read_unaligned();
            self.ptr = self.ptr.add(4);
            val
        }
    }

    unsafe fn read_i64(&mut self) -> i64 {
        unsafe {
            self.align_to(8);
            let val = (self.ptr as *const i64).read_unaligned();
            self.ptr = self.ptr.add(8);
            val
        }
    }

    unsafe fn read_f64(&mut self) -> f64 {
        unsafe {
            self.align_to(8);
            let val = (self.ptr as *const f64).read_unaligned();
            self.ptr = self.ptr.add(8);
            val
        }
    }

    unsafe fn read_ptr(&mut self) -> *const u8 {
        unsafe { self.read_i32() as usize as *const u8 }
    }
}

// -- Formatting helpers --

fn push_padded(out: &mut BufWriter, content: &[u8], width: usize, left: bool, pad: u8) {
    let len = content.len();
    if width > len && !left {
        // For zero-padding, preserve leading sign character.
        if pad == b'0' && !content.is_empty()
            && (content[0] == b'-' || content[0] == b'+' || content[0] == b' ')
        {
            out.push(content[0]);
            for _ in 0..(width - len) { out.push(pad); }
            out.push_bytes(&content[1..]);
            return;
        }
        for _ in 0..(width - len) { out.push(pad); }
    }
    out.push_bytes(content);
    if width > len && left {
        for _ in 0..(width - len) { out.push(b' '); }
    }
}

fn fmt_signed(val: i64, force_sign: bool, space_sign: bool) -> String {
    if val < 0 {
        format!("{val}")
    } else if force_sign {
        format!("+{val}")
    } else if space_sign {
        format!(" {val}")
    } else {
        format!("{val}")
    }
}

fn fmt_float_fixed(val: f64, prec: usize, force_sign: bool, space_sign: bool) -> String {
    if !val.is_sign_negative() && force_sign {
        format!("+{val:.prec$}")
    } else if !val.is_sign_negative() && space_sign {
        format!(" {val:.prec$}")
    } else {
        format!("{val:.prec$}")
    }
}

fn fmt_float_sci(val: f64, prec: usize, upper: bool, force_sign: bool, space_sign: bool) -> String {
    let e = if upper { 'E' } else { 'e' };
    if val == 0.0 {
        let sign = if force_sign { "+" } else if space_sign { " " } else { "" };
        return format!("{sign}0.{:0>w$}{e}+00", "", w = prec);
    }
    let abs = val.abs();
    let exp = abs.log10().floor() as i32;
    let mant = abs / 10f64.powi(exp);
    let sign = if val < 0.0 { "-" }
               else if force_sign { "+" }
               else if space_sign { " " }
               else { "" };
    let exp_sign = if exp >= 0 { '+' } else { '-' };
    format!("{sign}{mant:.prec$}{e}{exp_sign}{:02}", exp.unsigned_abs())
}

fn fmt_float_general(val: f64, prec: usize, upper: bool, force_sign: bool, space_sign: bool) -> String {
    let prec = prec.max(1);
    if val == 0.0 {
        let sign = if force_sign { "+" } else if space_sign { " " } else { "" };
        return format!("{sign}0");
    }
    let abs = val.abs();
    let exp = abs.log10().floor() as i32;
    let mut s = if exp >= -1 && exp < prec as i32 {
        let dp = (prec as i32 - 1 - exp).max(0) as usize;
        fmt_float_fixed(val, dp, force_sign, space_sign)
    } else {
        fmt_float_sci(val, prec.saturating_sub(1), upper, force_sign, space_sign)
    };
    // %g strips trailing zeros.
    if let Some(dot) = s.find('.') {
        let e_pos = s.find(if upper { 'E' } else { 'e' });
        let end = e_pos.unwrap_or(s.len());
        let trail_start = s[dot..end].trim_end_matches('0').len() + dot;
        let trail_start = if s.as_bytes().get(trail_start - 1) == Some(&b'.') {
            trail_start - 1
        } else {
            trail_start
        };
        if trail_start < end {
            s.replace_range(trail_start..end, "");
        }
    }
    s
}

// ----------------------------------------------
// c_vsnprintf
// ----------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn c_vsnprintf(buf: *mut u8,
                                     size: usize,
                                     fmt: *const u8,
                                     ap: *const u8) -> i32
{
    unsafe {
        if buf.is_null() || size == 0 { return 0; }

        let mut out = BufWriter::new(buf, size);
        let mut va = VaReader::new(ap);
        let mut p = fmt;

        while *p != 0 {
            if *p != b'%' {
                out.push(*p);
                p = p.add(1);
                continue;
            }
            p = p.add(1);
            if *p == 0 { break; }
            if *p == b'%' { out.push(b'%'); p = p.add(1); continue; }

            // -- Flags --
            let mut left = false;
            let mut sign = false;
            let mut space = false;
            let mut zero = false;
            let mut alt = false;
            loop {
                match *p {
                    b'-' => left  = true,
                    b'+' => sign  = true,
                    b' ' => space = true,
                    b'0' => zero  = true,
                    b'#' => alt   = true,
                    _    => break,
                }
                p = p.add(1);
            }

            // -- Width --
            let mut width: usize = 0;
            if *p == b'*' {
                width = va.read_i32().max(0) as usize;
                p = p.add(1);
            } else {
                while (*p).is_ascii_digit() {
                    width = width * 10 + (*p - b'0') as usize;
                    p = p.add(1);
                }
            }

            // -- Precision --
            let precision = if *p == b'.' {
                p = p.add(1);
                if *p == b'*' {
                    let v = va.read_i32().max(0) as usize;
                    p = p.add(1);
                    Some(v)
                } else {
                    let mut v: usize = 0;
                    while (*p).is_ascii_digit() {
                        v = v * 10 + (*p - b'0') as usize;
                        p = p.add(1);
                    }
                    Some(v)
                }
            } else {
                None
            };

            // -- Length modifier --
            let mut long_count: u8 = 0;
            loop {
                match *p {
                    b'l' => { long_count += 1; p = p.add(1); }
                    b'h' | b'z' | b'j' | b't' => { p = p.add(1); }
                    _ => break,
                }
            }

            // -- Conversion --
            let conv = *p;
            if conv == 0 { break; }
            p = p.add(1);

            let pad = if zero && !left { b'0' } else { b' ' };

            match conv {
                b'd' | b'i' => {
                    let val = if long_count >= 2 { va.read_i64() } else { va.read_i32() as i64 };
                    let s = fmt_signed(val, sign, space);
                    push_padded(&mut out, s.as_bytes(), width, left, pad);
                }
                b'u' => {
                    let val = if long_count >= 2 { va.read_i64() as u64 } else { va.read_i32() as u32 as u64 };
                    let s = format!("{val}");
                    push_padded(&mut out, s.as_bytes(), width, left, pad);
                }
                b'x' | b'X' => {
                    let val = if long_count >= 2 { va.read_i64() as u64 } else { va.read_i32() as u32 as u64 };
                    let hex = if conv == b'x' { format!("{val:x}") } else { format!("{val:X}") };
                    let s = if alt && val != 0 {
                        let prefix = if conv == b'x' { "0x" } else { "0X" };
                        format!("{prefix}{hex}")
                    } else {
                        hex
                    };
                    push_padded(&mut out, s.as_bytes(), width, left, pad);
                }
                b'o' => {
                    let val = if long_count >= 2 { va.read_i64() as u64 } else { va.read_i32() as u32 as u64 };
                    let s = format!("{val:o}");
                    push_padded(&mut out, s.as_bytes(), width, left, pad);
                }
                b'f' | b'F' => {
                    let val = va.read_f64();
                    let prec = precision.unwrap_or(6);
                    let s = fmt_float_fixed(val, prec, sign, space);
                    push_padded(&mut out, s.as_bytes(), width, left, pad);
                }
                b'e' | b'E' => {
                    let val = va.read_f64();
                    let prec = precision.unwrap_or(6);
                    let s = fmt_float_sci(val, prec, conv == b'E', sign, space);
                    push_padded(&mut out, s.as_bytes(), width, left, pad);
                }
                b'g' | b'G' => {
                    let val = va.read_f64();
                    let prec = precision.unwrap_or(6);
                    let s = fmt_float_general(val, prec, conv == b'G', sign, space);
                    push_padded(&mut out, s.as_bytes(), width, left, pad);
                }
                b's' => {
                    let str_ptr = va.read_ptr();
                    let bytes = c_str_bytes(str_ptr);
                    let max = precision.unwrap_or(bytes.len()).min(bytes.len());
                    let slice = &bytes[..max];
                    push_padded(&mut out, slice, width, left, b' ');
                }
                b'c' => {
                    let val = va.read_i32() as u8;
                    out.push(val);
                }
                b'p' => {
                    let val = va.read_i32() as u32;
                    let s = format!("0x{val:x}");
                    push_padded(&mut out, s.as_bytes(), width, left, b' ');
                }
                b'n' => {
                    let ptr = va.read_ptr() as *mut i32;
                    *ptr = out.len as i32;
                }
                _ => {
                    out.push(b'%');
                    out.push(conv);
                }
            }
        }

        out.finish()
    }
}

// ----------------------------------------------
// c_sscanf
// ----------------------------------------------
// imgui calls sscanf with exactly 1 output pointer (e.g. sscanf(buf, "%d", &val)).
// On wasm32, variadic args are passed as extra WASM function parameters.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn c_sscanf(str_ptr: *const u8,
                                  fmt_ptr: *const u8,
                                  out_ptr: *mut u8) -> i32
{
    unsafe {
        let input = c_str_bytes(str_ptr);
        let fmt = c_str_bytes(fmt_ptr);

        // Skip leading whitespace in input.
        let input = &input[input.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(input.len())..];

        // Parse the format string to find the conversion specifier.
        let mut fp = 0;

        // Skip literal chars / whitespace in format.
        while fp < fmt.len() && fmt[fp] != b'%' { fp += 1; }
        if fp >= fmt.len() { return 0; }
        fp += 1; // skip '%'

        // Skip flags/width (not typically used in sscanf by imgui, but be safe).
        while fp < fmt.len() && (fmt[fp] == b'*' || fmt[fp].is_ascii_digit()) { fp += 1; }

        // Length modifier.
        let mut is_long = false;
        if fp < fmt.len() && fmt[fp] == b'l' { is_long = true; fp += 1; }
        if fp < fmt.len() && fmt[fp] == b'l' { fp += 1; } // skip second 'l'

        if fp >= fmt.len() { return 0; }
        let conv = fmt[fp];

        // Convert input bytes to a str for parsing.
        let input_str = match std::str::from_utf8(input) {
            Ok(s) => s.trim(),
            Err(_) => return 0,
        };

        match conv {
            b'd' | b'i' => {
                if let Ok(val) = input_str.parse::<i32>() {
                    *(out_ptr as *mut i32) = val;
                    return 1;
                }
                // Try parsing hex (0x prefix) for %i.
                if conv == b'i' {
                    if let Some(hex) = input_str.strip_prefix("0x").or_else(|| input_str.strip_prefix("0X")) {
                        if let Ok(val) = i32::from_str_radix(hex, 16) {
                            *(out_ptr as *mut i32) = val;
                            return 1;
                        }
                    }
                }
                0
            }
            b'u' => {
                if let Ok(val) = input_str.parse::<u32>() {
                    *(out_ptr as *mut u32) = val;
                    return 1;
                }
                0
            }
            b'x' | b'X' => {
                let hex_str = input_str.strip_prefix("0x")
                    .or_else(|| input_str.strip_prefix("0X"))
                    .unwrap_or(input_str);
                if let Ok(val) = u32::from_str_radix(hex_str, 16) {
                    *(out_ptr as *mut u32) = val;
                    return 1;
                }
                0
            }
            b'f' | b'e' | b'g' => {
                if is_long {
                    // %lf → f64
                    if let Ok(val) = input_str.parse::<f64>() {
                        *(out_ptr as *mut f64) = val;
                        return 1;
                    }
                } else {
                    // %f → f32
                    if let Ok(val) = input_str.parse::<f32>() {
                        *(out_ptr as *mut f32) = val;
                        return 1;
                    }
                }
                0
            }
            _ => 0,
        }
    }
}
