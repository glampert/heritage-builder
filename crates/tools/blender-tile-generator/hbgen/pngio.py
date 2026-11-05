"""Minimal PNG reading and writing with the standard library only, for the
tools that run outside Blender (validate.py, sync_tileset.py).

Reads 8-bit, non-interlaced greyscale, RGB, grey+alpha and RGBA images, which
covers Blender's output and the hand-made art. Pixels come back as one
bytearray of RGBA rows, top row first."""

import struct
import zlib

_SIG = b"\x89PNG\r\n\x1a\n"
_CHANNELS = {0: 1, 2: 3, 4: 2, 6: 4}


def read_size(path):
    """(width, height) from the header, without decoding."""
    with open(path, "rb") as f:
        head = f.read(24)
    if head[:8] != _SIG or head[12:16] != b"IHDR":
        raise ValueError(f"{path}: not a PNG")
    return struct.unpack(">II", head[16:24])


def _chunks(data):
    pos = 8
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        yield kind, data[pos + 8:pos + 8 + n]
        pos += 12 + n


def _unfilter(raw, w, h, bpp):
    stride = w * bpp
    out = bytearray(stride * h)
    prev = bytearray(stride)
    pos = 0
    for y in range(h):
        ft = raw[pos]; row = bytearray(raw[pos + 1:pos + 1 + stride]); pos += 1 + stride
        if ft == 1:                                   # Sub
            for i in range(bpp, stride):
                row[i] = (row[i] + row[i - bpp]) & 255
        elif ft == 2:                                 # Up
            row = bytearray(map(lambda a, b: (a + b) & 255, row, prev))
        elif ft == 3:                                 # Average
            for i in range(stride):
                left = row[i - bpp] if i >= bpp else 0
                row[i] = (row[i] + ((left + prev[i]) >> 1)) & 255
        elif ft == 4:                                 # Paeth
            for i in range(stride):
                if i >= bpp:
                    a = row[i - bpp]; c = prev[i - bpp]
                else:
                    a = c = 0
                b = prev[i]
                p = a + b - c
                pa = p - a if p > a else a - p
                pb = p - b if p > b else b - p
                pc = p - c if p > c else c - p
                row[i] = (row[i] + (a if pa <= pb and pa <= pc else (b if pb <= pc else c))) & 255
        elif ft != 0:
            raise ValueError(f"bad PNG filter type {ft}")
        out[y * stride:(y + 1) * stride] = row
        prev = row
    return out


def read_rgba(path):
    """(width, height, pixels): pixels is a bytearray of RGBA, top row first."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != _SIG:
        raise ValueError(f"{path}: not a PNG")
    idat = []
    for kind, body in _chunks(data):
        if kind == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or interlace or ctype not in _CHANNELS:
                raise ValueError(f"{path}: unsupported PNG (depth {depth}, colour type {ctype}, interlace {interlace})")
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    ch = _CHANNELS[ctype]
    px = _unfilter(zlib.decompress(b"".join(idat)), w, h, ch)
    if ch == 4:
        return w, h, px
    out = bytearray(w * h * 4)
    for i in range(w * h):
        if ch == 3:   out[i * 4:i * 4 + 4] = px[i * 3:i * 3 + 3] + b"\xff"
        elif ch == 2: g = px[i * 2]; out[i * 4:i * 4 + 4] = bytes((g, g, g, px[i * 2 + 1]))
        else:         g = px[i]; out[i * 4:i * 4 + 4] = bytes((g, g, g, 255))
    return w, h, out


def write_rgba(path, w, h, pixels):
    """Write RGBA pixels (top row first) as an 8-bit PNG, unfiltered."""
    stride = w * 4
    raw = b"".join(b"\x00" + bytes(pixels[y * stride:(y + 1) * stride]) for y in range(h))
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    with open(path, "wb") as f:
        f.write(_SIG + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))
