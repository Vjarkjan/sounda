"""Compare the Rust demuxer's raw AAC output with FFmpeg on real M4A files.

All media is synthesized locally. These tests exercise demuxing, not DSP or DAC.
"""
import json
import shutil
import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "output" / "media-tests"
OUT.mkdir(parents=True, exist_ok=True)


def run(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, **kwargs)


def ffmpeg(*args):
    return run(["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-y", *map(str, args)])


def widen_offsets(blob):
    """Change stco into co64 in a moov-at-end fixture without moving mdat."""
    containers = {b"moov", b"trak", b"mdia", b"minf", b"stbl"}
    result = bytearray()
    pos = 0
    while pos < len(blob):
        size, kind = struct.unpack_from(">I4s", blob, pos)
        assert size >= 8 and pos + size <= len(blob)
        data = blob[pos + 8:pos + size]
        if kind in containers:
            data = widen_offsets(data)
        elif kind == b"stco":
            count = struct.unpack_from(">I", data, 4)[0]
            offsets = struct.unpack_from(f">{count}I", data, 8)
            data = data[:8] + struct.pack(f">{count}Q", *offsets)
            kind = b"co64"
        result += struct.pack(">I4s", len(data) + 8, kind) + data
        pos += size
    return bytes(result)


if not shutil.which("ffmpeg") or not shutil.which("ffprobe"):
    raise SystemExit("FFmpeg and ffprobe are required for media validation.")
run(["cargo", "+stable", "build", "--offline", "--bin", "inspect-m4a"], cwd=ROOT)
inspector = ROOT / "target" / "debug" / "inspect-m4a"
results = []
fixtures = []
for name, rate, channels, bitrate, fast in [
    ("stereo-44100", 44100, 2, "128k", False),
    ("stereo-48000-faststart", 48000, 2, "256k", True),
    ("mono-22050", 22050, 1, "48k", False),
    ("mono-8000", 8000, 1, "24k", False),
]:
    path = OUT / f"{name}.m4a"
    args = ["-f", "lavfi", "-i", f"sine=frequency=440:sample_rate={rate}:duration=1.5",
            "-ac", channels, "-c:a", "aac", "-profile:a", "aac_low", "-b:a", bitrate]
    if fast:
        args += ["-movflags", "+faststart"]
    ffmpeg(*args, path)
    fixtures.append((name, path))

wide = OUT / "stereo-co64.m4a"
wide.write_bytes(widen_offsets(fixtures[0][1].read_bytes()))
fixtures.append(("stereo-co64", wide))
interleaved = OUT / "interleaved.m4a"
ffmpeg("-f", "lavfi", "-i", "color=c=black:s=16x16:r=10:d=1.5",
       "-f", "lavfi", "-i", "sine=frequency=660:sample_rate=44100:duration=1.5",
       "-c:v", "mpeg4", "-c:a", "aac", "-b:a", "96k", "-f", "mp4", interleaved)
fixtures.append(("interleaved-audio-video", interleaved))

for name, path in fixtures:
    rust_raw, reference = OUT / f"{name}.rust.raw", OUT / f"{name}.ffmpeg.raw"
    meta = json.loads(run([str(inspector), str(path), str(rust_raw)]).stdout)
    ffmpeg("-i", path, "-map", "0:a:0", "-c:a", "copy", "-f", "data", reference)
    probe = json.loads(run(["ffprobe", "-v", "error", "-select_streams", "a:0",
                           "-show_entries", "stream=sample_rate,channels,nb_frames", "-of", "json", str(path)]).stdout)["streams"][0]
    assert rust_raw.read_bytes() == reference.read_bytes(), name
    assert meta["sample_rate"] == int(probe["sample_rate"]), name
    assert meta["channels"] == probe["channels"], name
    assert meta["frames"] == int(probe["nb_frames"]), name
    results.append({"fixture": name, "result": "PASS", **meta})
    print(f"PASS {name}: {meta['frames']} AAC frames, raw bytes match FFmpeg")

alac = OUT / "unsupported-alac.m4a"
ffmpeg("-f", "lavfi", "-i", "sine=duration=0.1", "-c:a", "alac", alac)
for name, path in [("ALAC", alac)]:
    check = subprocess.run([str(inspector), str(path)], capture_output=True, text=True)
    assert check.returncode != 0, name
    results.append({"fixture": name, "result": "REJECTED", "message": check.stderr.strip()})
    print(f"PASS {name}: rejected with a readable error")

data = fixtures[0][1].read_bytes()
for length in [0, 7, 15, 64, len(data) - 32]:
    path = OUT / f"truncated-{length}.m4a"
    path.write_bytes(data[:length])
    check = subprocess.run([str(inspector), str(path)], capture_output=True, text=True)
    assert check.returncode != 0, length
results.append({"fixture": "five truncated files", "result": "REJECTED"})
(OUT / "results.json").write_text(json.dumps(results, indent=2) + "\n")
print("PASS five truncated files; report: output/media-tests/results.json")
