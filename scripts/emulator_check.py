#!/usr/bin/env python3
"""Reproduce Sounda SD/UI/DSP tests using the official melonDS 1.1 core.

Firmware stays local. Runs use a private NAND copy and generated SD image.
LLE executes aac.a; the optional HLE mode is labelled and uses host FAAD.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import wave

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path.home() / ".local/src/sounda-melonds-1.1"
COMMIT = "b86390e4428bf38ce4c1ce0e9ca446d6d25955e8"
BUILD = ROOT / "output/emulator/build"
DIAGNOSTICS = [
    "magic", "version", "dsp_boots", "dsp_stops", "decoded_frames", "underruns",
    "queued_pcm_frames", "pcm_low_water", "max_decode_ms", "max_pump_gap_ms",
    "redraws", "max_draw_ms", "directory_entries", "directory_scan_ms", "unused",
    "dsp_psts", "scfg_ext", "app_ms", "pcm_callbacks", "max_ledger_entries",
    "pane", "queue_length", "current_folder_track", "elapsed_seconds", "duration_seconds",
    "position", "lid_state", "headphones", "searching", "visible_entries", "seeks", "paused",
    "expanded_listing", "selected_entry", "first_entry", "viewport_rows",
    "selected_queue_entry", "current_path_hash", "player_loaded", "next_queued_path_hash",
]


def run(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run([str(s) for s in args], check=True, **kwargs)


def setup(source: Path) -> None:
    if not source.exists():
        source.parent.mkdir(parents=True, exist_ok=True)
        run(["git", "clone", "--depth", "1", "--branch", "1.1",
             "https://github.com/melonDS-emu/melonDS.git", source])
    commit = subprocess.check_output(["git", "-C", source, "rev-parse", "HEAD"], text=True).strip()
    if commit != COMMIT:
        raise SystemExit(f"Expected official melonDS 1.1 commit {COMMIT}; found {commit}")
    run(["cmake", "-S", ROOT / "scripts/emulator", "-B", BUILD, "-G", "Ninja",
         f"-DMELONDS_SOURCE={source}", "-DCMAKE_BUILD_TYPE=RelWithDebInfo"])
    run(["cmake", "--build", BUILD, "-j", "3"])
    destination = Path.home() / ".local/bin/sounda-emulator"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(BUILD / "sounda-emulator", destination)
    print(f"Installed {destination} (official melonDS 1.1 core, ARM9/ARM7 GDB, Teakra)")


def fixture(folder: Path, *, short: bool = False, rates: bool = False) -> Path:
    music = folder / "sd/Music"
    music.mkdir(parents=True, exist_ok=True)
    if rates:
        for i, (rate, channels, frequency) in enumerate([(48000, 2, 440), (22050, 1, 660), (8000, 1, 880)]):
            track = music / f"Track {i:03}.m4a"
            if not track.exists():
                run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi",
                     "-i", f"sine=frequency={frequency}:sample_rate={rate}:duration=3",
                     "-ac", str(channels), "-c:a", "aac", "-profile:a", "aac_low",
                     "-b:a", "64k" if channels == 1 else "128k", track])
        return music.parent
    tone = folder / "tone.m4a"
    if not tone.exists():
        run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi",
             "-i", f"sine=frequency=440:sample_rate=44100:duration={3 if short else 30}",
             "-af", "pan=stereo|c0=c0|c1=c0", "-c:a", "aac", "-profile:a", "aac_low",
             "-b:a", "128k", tone])
    for i in range(3 if short else 200):
        track = music / f"Track {i:03}.m4a"
        if not track.exists():
            shutil.copyfile(tone, track)
    return music.parent


def events(path: Path, scenario: str) -> None:
    if scenario in ("controls", "rates"):
        sequence = """# Open folder, play, change tracks twice, pause/resume, then loop on EOF.
120 keys 1
123 keys 0
160 keys 1
163 keys 0
240 keys 256
243 keys 0
320 keys 256
323 keys 0
400 keys 8
403 keys 0
450 keys 8
453 keys 0
600 keys 1024
603 keys 0
608 keys 1024
611 keys 0
850 shot
"""
        if scenario == "rates":
            # Exercise the revised initial-buffer pause path before the first
            # stream has accumulated its normal 16K-frame prebuffer.
            sequence += "164 keys 8\n166 keys 0\n175 keys 8\n178 keys 0\n"
            for frame in range(520, 560, 8):
                sequence += f"{frame} keys 128\n{frame + 2} keys 0\n"
        path.write_text(sequence)
        return
    lines = ["# frame event pressed_button_mask / touch_x touch_y",
             "120 keys 1", "123 keys 0", "420 keys 1", "423 keys 0", "600 shot"]
    for frame in range(620, 920, 8):
        lines += [f"{frame} keys 128", f"{frame + 2} keys 0"]
    lines += ["1000 keys 1", "1003 keys 0", "1250 shot"]
    path.write_text("\n".join(lines) + "\n")


def analyze(out: Path, rom: Path | None = None) -> dict:
    import numpy as np
    from PIL import Image

    report = json.loads((out / "runner.json").read_text())
    scenario_file = out / "scenario.json"
    scenario = json.loads(scenario_file.read_text())["scenario"] if scenario_file.exists() else "browse"
    report["scenario"] = scenario
    if rom:
        report["rom_sha256"] = hashlib.sha256(rom.read_bytes()).hexdigest()
    pcm = np.fromfile(out / "audio.s16le", dtype="<i2").reshape(-1, 2)
    with wave.open(str(out / "audio.wav"), "wb") as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(48000)
        wav.writeframes(pcm.tobytes())
    for path in out.glob("frame-*.ppm"):
        Image.open(path).save(path.with_suffix(".png"))
    rows = list(csv.DictReader((out / "trace.csv").open()))
    command_file = out / "command.json"
    if command_file.exists():
        report["requested_frames"] = int(json.loads(command_file.read_text())[5])
    report["completed_frames"] = len(rows)
    # Compare each run to its own settled first row. The baseline and retained
    # renderer intentionally use different text labels and therefore CRCs.
    settled = [r for r in rows if 300 <= int(r["frame"]) <= 400]
    if settled:
        target = settled[-1].get("list_hash")
        ready = next((int(r["frame"]) for r in rows if int(r["frame"]) >= 120
                      and r.get("list_hash") == target), None)
        report["directory_first_visible_frame"] = ready
        report["directory_open_event_frame"] = 120
        if ready is not None:
            report["directory_display_delay_emulated_ms"] = round((ready - 120) * 1000 / 59.8261, 2)
    active = np.flatnonzero(np.max(np.abs(pcm.astype(np.int32)), axis=1) > 64)
    report["first_audible_sample"] = int(active[0]) if len(active) else None
    report["last_audible_sample"] = int(active[-1]) if len(active) else None
    # Every complete 1ms window of digital silence within the requested
    # browsing interval. This tone fixture has no musical rests.
    segments = [("steady", 540, 620), ("browsing", 620, 920), ("second_track", 1150, 1300)]
    if scenario in ("controls", "rates"):
        segments = [("before_pause", 360, 390), ("paused", 410, 440), ("resumed", 460, 500), ("after_loop", 800, 899)]
    if scenario == "rates":
        segments += [("stereo_48000", 190, 220), ("mono_22050", 260, 300), ("initial_pause", 166, 174)]
    for label, start, end in segments:
        begin = sum(int(r["audio_frames"]) for r in rows if int(r["frame"]) < start)
        finish = sum(int(r["audio_frames"]) for r in rows if int(r["frame"]) < end)
        part = pcm[begin:finish]
        count = len(part) // 48
        if count:
            windows = part[:count * 48].reshape(count, 48, 2)
            silent = np.all(windows == 0, axis=(1, 2))
            signal = part.astype(np.float64)
            spectrum = np.abs(np.fft.rfft(signal[:, 0] - signal[:, 0].mean()))
            frequency = float(np.argmax(spectrum[1:]) + 1) * 48000 / len(part)
            rms_windows = signal[:len(signal) // 960 * 960].reshape(-1, 960, 2)
            rms_20ms = np.sqrt(np.mean(rms_windows * rms_windows, axis=(1, 2)))
            report[label] = {"complete_ms": count, "digital_silence_ms": int(silent.sum()),
                             "peak": int(np.max(np.abs(part.astype(np.int32)))),
                             "rms": round(float(np.sqrt(np.mean(signal * signal))), 2),
                             "dominant_frequency_hz": round(frequency, 2),
                             "rms_20ms_min": round(float(rms_20ms.min()), 2),
                             "rms_20ms_max": round(float(rms_20ms.max()), 2),
                             "max_adjacent_sample_delta": int(np.abs(np.diff(signal, axis=0)).max())}
    diagnostic_path = out / "diagnostics.csv"
    if diagnostic_path.exists():
        diagnostics = list(csv.DictReader(diagnostic_path.open()))
        if diagnostics and "word0" in diagnostics[-1]:
            report["diagnostics"] = {name: int(diagnostics[-1][f"word{i}"])
                                     for i, name in enumerate(DIAGNOSTICS) if f"word{i}" in diagnostics[-1]}
            # The app resets this counter on each song. A clean final song
            # must not conceal an underrun observed earlier in the scenario.
            report["maximum_observed_underruns"] = max(int(row["word5"]) for row in diagnostics)
            watermarks = [int(row["word7"]) for row in diagnostics if 0 < int(row["word7"]) < 0xffffffff]
            report["pcm_low_water_observed"] = min(watermarks) if watermarks else None
            if scenario == "rates":
                report["initial_buffer_pause_telemetry"] = [
                    {"frame": int(row["frame"]), "decoded_frames": int(row["word4"]),
                     "pcm_callbacks": int(row["word18"]), "underruns": int(row["word5"])}
                    for row in diagnostics if int(row["frame"]) in (164, 165, 174, 175, 176, 178)]
    if report.get("diagnostics"):
        diag = report["diagnostics"]
        checks = {"core_no_exception": not report["core_exception"],
                  "completed_frames": len(rows) == report.get("requested_frames", len(rows)),
                  "audio_captured": len(active) > 0,
                  "no_pcm_underruns": report["maximum_observed_underruns"] == 0,
                  "dsp_restarts": diag["dsp_boots"] >= (5 if scenario in ("controls", "rates") else 2),
                  "directory_count": diag["directory_entries"] == (3 if scenario in ("controls", "rates") else 200)}
        if not report["hle_aac_decoder"]:
            checks["original_firmware_lle"] = report["host_aac_decode_calls"] == 0
        if scenario == "browse" and "browsing" in report:
            checks["continuous_while_browsing"] = report["browsing"]["digital_silence_ms"] == 0
            checks["tone_frequency"] = abs(report["browsing"]["dominant_frequency_hz"] - 440) < 2
        if scenario in ("controls", "rates") and "paused" in report and "resumed" in report:
            checks["pause_is_silent"] = report["paused"]["peak"] == 0
            checks["resume_has_audio"] = report["resumed"]["rms_20ms_min"] > 2000
        if scenario == "rates" and "stereo_48000" in report and "mono_22050" in report:
            checks["stereo_48000_tone"] = abs(report["stereo_48000"]["dominant_frequency_hz"] - 440) < 3
            checks["mono_22050_tone"] = abs(report["mono_22050"]["dominant_frequency_hz"] - 660) < 3
            checks["mono_8000_tone"] = abs(report["resumed"]["dominant_frequency_hz"] - 880) < 3
            initial = report.get("initial_buffer_pause_telemetry", [])
            at_pause = next((r for r in initial if r["frame"] == 164), None)
            checks["paused_before_prebuffer_complete"] = bool(at_pause and 0 < at_pause["decoded_frames"] < 16
                and at_pause["pcm_callbacks"] == 0 and report["initial_pause"]["peak"] == 0)
        report["checks"] = checks
        report["passed"] = all(checks.values())
    (out / "analysis.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--setup", action="store_true", help="clone/build/install the pinned official core runner")
    parser.add_argument("--source", type=Path, default=SOURCE)
    parser.add_argument("--rom", type=Path, default=ROOT / "output/sounda.nds")
    parser.add_argument("--elf", type=Path, default=ROOT / "target/armv5te-nintendo-ds/release/sounda.elf")
    parser.add_argument("--bios-dir", type=Path, default=Path.home() / "Downloads/DS firmware + DSi NAND and firmware")
    parser.add_argument("--output", type=Path, default=ROOT / "output/emulator/current-lle")
    parser.add_argument("--frames", type=int, default=1320)
    parser.add_argument("--dsp", choices=["lle", "hle"], default="lle")
    parser.add_argument("--scenario", choices=["browse", "controls", "rates"], default="browse")
    parser.add_argument("--gdb-check", action="store_true")
    parser.add_argument("--analyze-only", action="store_true")
    args = parser.parse_args()
    if args.setup:
        setup(args.source)
        return
    out = args.output.resolve()
    if args.analyze_only:
        print(json.dumps(analyze(out, args.rom.resolve()), indent=2))
        return
    for name in ["biosnds9.bin", "biosnds7.bin", "biosdsi9.bin", "biosdsi7.bin", "dsifirmware.bin", "dsinand.bin"]:
        if not (args.bios_dir / name).is_file():
            raise SystemExit(f"Local console dump missing: {args.bios_dir / name}")
    out.mkdir(parents=True, exist_ok=True)
    (out / "scenario.json").write_text(json.dumps({"scenario": args.scenario}) + "\n")
    fixture_dir = "output/emulator" if args.scenario == "browse" else "output/emulator-" + args.scenario
    sd = fixture(ROOT / fixture_dir, short=args.scenario != "browse", rates=args.scenario == "rates")
    event_file = out / "events.tsv"
    events(event_file, args.scenario)
    nm = shutil.which("arm-none-eabi-nm") or "/opt/devkitpro/devkitARM/bin/arm-none-eabi-nm"
    symbols = subprocess.check_output([nm, args.elf], text=True)
    address = next(("0x" + line.split()[0] for line in symbols.splitlines()
                    if line.endswith(" sounda_diagnostics")), "0")
    command = [str(BUILD / "sounda-emulator"), str(args.rom.resolve()), str(args.bios_dir.resolve()),
               str(sd), str(out), str(args.frames), str(event_file),
               "4333" if args.gdb_check else "0", address, args.dsp]
    (out / "command.json").write_text(json.dumps(command, indent=2) + "\n")
    env = os.environ.copy()
    if args.gdb_check:
        env["SOUNDA_GDB_BREAK"] = "1"
    with (out / "emulator.log").open("w") as logfile:
        process = subprocess.Popen(command, stdout=logfile, stderr=subprocess.STDOUT, env=env)
        if args.gdb_check:
            gdb = shutil.which("arm-none-eabi-gdb") or "/opt/devkitpro/devkitARM/bin/arm-none-eabi-gdb"
            # GDB's own remote connection retry handles core initialization.
            result = subprocess.run([gdb, "-q", "-batch", str(args.elf.resolve()),
                "-ex", "set remotetimeout 10", "-ex", "set tcp connect-timeout 10",
                "-ex", "target remote 127.0.0.1:4333", "-ex", "info registers pc sp",
                "-ex", "x/4wx 0x04004000", "-ex", "detach"],
                capture_output=True, text=True, timeout=30)
            (out / "gdb.log").write_text(result.stdout + result.stderr)
        returncode = process.wait()
    report = analyze(out, args.rom.resolve())
    report["process_exit_code"] = returncode
    (out / "analysis.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    if returncode or not report.get("passed", False):
        raise SystemExit(returncode or 1)


if __name__ == "__main__":
    main()
