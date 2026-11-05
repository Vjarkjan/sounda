#!/usr/bin/env python3
"""Compare firmware-produced PCM with an independent FFmpeg AAC decode.

This reference decoder is not linked into sounda-dsp-probe or the Nintendo ROM.
ADTS retains encoder priming so both sides are compared without MP4 edit lists.
"""
import argparse
from array import array
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('adts', nargs='+', type=Path)
p.add_argument('--output', type=Path, default=Path('output/research/teakra-pcm-comparison.json'))
a = p.parse_args()
reports = []
for path in a.adts:
    reference_path = Path(str(path)+'.reference.s16le')
    dsp_path = Path(str(path)+'.dsp.s16le')
    subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-i', str(path), '-c:a', 'pcm_s16le', '-f', 's16le', str(reference_path)], check=True)
    reference_bytes, dsp_bytes = reference_path.read_bytes(), dsp_path.read_bytes()
    reference, dsp = array('h'), array('h')
    reference.frombytes(reference_bytes)
    dsp.frombytes(dsp_bytes)
    if sys.byteorder != 'little':
        reference.byteswap()
        dsp.byteswap()
    if len(dsp) != len(reference) or not dsp:
        raise SystemExit(f'PCM length mismatch: {path}: {len(dsp)} / {len(reference)}')
    xx = sum(x*x for x in dsp)
    yy = sum(y*y for y in reference)
    xy = sum(x*y for x, y in zip(dsp, reference))
    error = sum((x-y)**2 for x, y in zip(dsp, reference))
    reports.append({'adts': str(path), 'sample_values': len(dsp), 'dsp_sha256': hashlib.sha256(dsp_bytes).hexdigest(), 'reference_sha256': hashlib.sha256(reference_bytes).hexdigest(), 'rmse_s16': math.sqrt(error/len(dsp)), 'max_absolute_error_s16': max(abs(x-y) for x, y in zip(dsp, reference)), 'snr_db': 10*math.log10(yy/error) if error else None, 'gain_relative_to_reference': xy/yy, 'correlation': xy/math.sqrt(xx*yy)})
a.output.write_text(json.dumps(reports, indent=2)+'\n')
print(json.dumps(reports, indent=2))
