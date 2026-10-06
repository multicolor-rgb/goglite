#!/usr/bin/env python3
"""Generuje prostą ikonę PNG (256×256) dla goglite."""
import struct, zlib, sys

W, H = 256, 256

def row_pixels(y):
    """Zwraca wiersz RGBA dla danego y."""
    row = bytes([0])  # filter byte (none)
    for x in range(W):
        cx, cy = x / W, y / H
        # gradient od #5a76f0 (akcent) w lewym górnym do #1b1e2b (tło)
        r = int(0x5a * (1 - cx * 0.6) + 0x1b * (cx * 0.6))
        g = int(0x76 * (1 - cy * 0.5) + 0x1e * (cy * 0.5))
        b = int(0xf0 * (1 - (cx + cy) * 0.3) + 0x2b * ((cx + cy) * 0.3))
        row += struct.pack('BBBB', r, g, b, 255)
    return row

def make_png():
    raw = b''.join(row_pixels(y) for y in range(H))
    compressed = zlib.compress(raw)

    def chunk(ctype, data):
        c = ctype + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)

    sig = b'\x89PNG\r\n\x1a\n'
    ihdr = struct.pack('>IIBBBBB', W, H, 8, 6, 0, 0, 0)  # 8-bit RGBA
    return sig + chunk(b'IHDR', ihdr) + chunk(b'IDAT', compressed) + chunk(b'IEND', b'')

def main():
    data = make_png()
    out = sys.argv[1] if len(sys.argv) > 1 else 'icon.png'
    with open(out, 'wb') as f:
        f.write(data)
    print(f'Ikona {W}×{H} zapisana: {out} ({len(data)} bajtów)')

if __name__ == '__main__':
    main()