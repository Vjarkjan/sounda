"""Generate an original 32x32 launcher icon. Requires only Python's stdlib."""
import struct
from pathlib import Path

pixels = []
palette = [(10, 21, 38), (32, 49, 67), (70, 222, 186), (232, 244, 248)]
for y in range(32):
    row = []
    for x in range(32):
        c = 1 if (x - 16) ** 2 + (y - 16) ** 2 < 182 else 0
        if (17 <= x <= 20 and 7 <= y <= 22) or (19 <= x <= 25 and 7 <= y <= 10):
            c = 2
        if ((x - 14) * 2) ** 2 + ((y - 23) * 3) ** 2 <= 65:
            c = 2
        row.append(bytes((palette[c][2], palette[c][1], palette[c][0])))
    pixels.append(b"".join(row))
data = b"".join(reversed(pixels))
header = struct.pack("<2sIHHI", b"BM", 54 + len(data), 0, 0, 54)
header += struct.pack("<IiiHHIIiiII", 40, 32, 32, 1, 24, 0, len(data), 2835, 2835, 0, 0)
Path("output").mkdir(exist_ok=True)
Path("output/icon.bmp").write_bytes(header + data)
