#!/usr/bin/env python3
"""Extract local DSi Sound analysis images; never modify or distribute inputs.

The ARM9i/ARM7i ranges remain encrypted as stored. Runtime images must come
from the separately documented emulator run. No keys are embedded here.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path, default=Path('00000002.app'))
    parser.add_argument('--firmware', type=Path, default=Path('aac.a'))
    parser.add_argument('--output', type=Path, default=Path('output/research'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    app, firmware = args.app.read_bytes(), args.firmware.read_bytes()
    manifest = {'inputs': [], 'images': [], 'coff_sections': [], 'placements': []}
    for path, data in [(args.app, app), (args.firmware, firmware)]:
        manifest['inputs'].append({'path': str(path.resolve()), 'bytes': len(data), 'sha256': sha(data)})
    if app[12:16] != b'HNKE' or firmware[:2] != b'\xa4\x00':
        raise SystemExit('Inputs do not match the selected Nintendo DSi Sound / Teak COFF formats')
    for name, header in [('arm9', 0x20), ('arm7', 0x30), ('arm9i', 0x1c0), ('arm7i', 0x1d0)]:
        offset, entry, address, size = struct.unpack_from('<4I', app, header)
        if offset + size > len(app):
            raise SystemExit('Executable range outside supplied .app')
        data = app[offset:offset+size]
        path = args.output / f'dsisound-{name}.bin'
        path.write_bytes(data)
        manifest['images'].append({'path': str(path), 'file_offset': offset, 'load_address': hex(address), 'entry_field': hex(entry), 'bytes': size, 'sha256': sha(data), 'encrypted': name.endswith('i')})
        if name == 'arm9':
            data = data[0x800:]
            path = args.output / 'dsisound-arm9-main.bin'
            path.write_bytes(data)
            manifest['images'].append({'path': str(path), 'file_offset': offset+0x800, 'load_address': hex(address+0x800), 'entry_field': hex(entry), 'bytes': len(data), 'sha256': sha(data), 'encrypted': False, 'note': 'First 0x800-byte secure area excluded.'})
    count = struct.unpack_from('<H', firmware, 2)[0]
    optional = struct.unpack_from('<H', firmware, 16)[0]
    symbols, symbol_count = struct.unpack_from('<2I', firmware, 8)
    string_table = symbols + symbol_count*18
    images = {'code': bytearray(0x40000), 'data': bytearray(0x40000)}
    for i in range(count):
        header = 20 + optional + 40*i
        zero, name_offset = struct.unpack_from('<2I', firmware, header)
        if zero == 0:
            start = string_table + name_offset
            name = firmware[start:firmware.index(0, start)].decode('ascii')
        else:
            name = firmware[header:header+8].split(b'\0')[0].decode('ascii')
        code, data, size, offset = struct.unpack_from('<4I', firmware, header+8)
        flags = struct.unpack_from('<I', firmware, header+36)[0]
        section = {'index': i, 'name': name, 'code_word_address': code, 'data_word_address': data, 'bytes': size, 'file_offset': offset, 'flags': hex(flags)}
        manifest['coff_sections'].append(section)
        if flags & 0x80 or not size:
            continue
        for region, marker, mask, noload, address in [('code', '@c0', 8, 32, code), ('data', '@d0', 16, 64, data)]:
            # One name may contain BOTH @c0 and @d0. Each is a placement.
            if flags & mask and marker in name:
                if address*2+size > len(images[region]) or offset+size > len(firmware):
                    raise SystemExit('COFF placement outside admitted image')
                if not flags & noload:
                    images[region][address*2:address*2+size] = firmware[offset:offset+size]
                manifest['placements'].append({'section': i, 'region': region, 'word_address': hex(address), 'bytes': size, 'noload': bool(flags & noload)})
    for region, data in images.items():
        path = args.output / f'aac-{region}-image.bin'
        path.write_bytes(data)
        manifest['images'].append({'path': str(path), 'bytes': len(data), 'sha256': sha(data), 'note': 'Derived Teak memory image; addresses in words, bytes stored little endian.'})
    (args.output/'artifact-manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(json.dumps({'app_sha256': sha(app), 'firmware_sha256': sha(firmware), 'sections': count, 'placements': len(manifest['placements']), 'manifest': str(args.output/'artifact-manifest.json')}))


if __name__ == '__main__':
    main()
