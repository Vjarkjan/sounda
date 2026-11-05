#!/usr/bin/env python3
"""Wrap an explicit ARM memory range in ELF for static Ghidra import only."""
import argparse
from pathlib import Path
import struct

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('input', type=Path)
p.add_argument('output', type=Path)
p.add_argument('--base', type=lambda s: int(s, 0), required=True)
p.add_argument('--entry', type=lambda s: int(s, 0), required=True)
p.add_argument('--offset', type=lambda s: int(s, 0), default=0)
p.add_argument('--size', type=lambda s: int(s, 0))
a = p.parse_args()
b = a.input.read_bytes()[a.offset:]
if a.size is not None:
    b = b[:a.size]
if not b or not a.base <= a.entry < a.base + len(b):
    raise SystemExit('Entry must lie inside the selected nonempty byte range')
h = struct.pack('<16sHHIIIIIHHHHHH', b'\x7fELF'+bytes([1, 1, 1])+bytes(9), 2, 40, 1, a.entry, 52, 0, 0x05000000, 52, 32, 1, 0, 0, 0)
ph = struct.pack('<8I', 1, 0x1000, a.base, a.base, len(b), len(b), 7, 0x1000)
a.output.parent.mkdir(parents=True, exist_ok=True)
a.output.write_bytes(h+ph+bytes(0x1000-len(h)-len(ph))+b)
