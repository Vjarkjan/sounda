# DSi emulator and audio validation

The revised ROM passed full Nintendo DSi emulation with the original supplied
`aac.a` executing in Teakra. The main run changed tracks and scrolled a
200-file directory without a PCM underrun. A second run covered 48 kHz stereo,
22.05 kHz mono and 8 kHz mono, five DSP boots, pause/resume, sequential EOF
wrap and repeat-one EOF restart.
These are emulator results; testing this revised ROM on the user's DSi and SD
card is still required.

The latest [library/seek/lid run](library-features.md) adds 1,860 frames, 2,598 decoded AAC frames, 28 DSP boots, ten backward shuffle steps, and repeated closed-lid jack transitions, with zero PCM underruns. Its results are separate from the historical audio-repair comparison below.

The current [compact UI and explicit upcoming queue](compact-ui-queue.md) has a separate current-ROM scenario; `scripts/verify_library.py` now runs that scenario by default. Historical commands and artifacts below retain their recorded hashes.

## Tools installed locally

- Official **melonDS 1.1**, tag commit
  `b86390e4428bf38ce4c1ce0e9ca446d6d25955e8`, built with GDB stubs and debug
  symbols. Source: `~/.local/src/sounda-melonds-1.1/`.
- GUI executable: `~/.local/libexec/sounda-melonds/melonDS`.
- Isolated GUI launcher: `~/.local/bin/melonDS-sounda-debug`.
- Reproducible headless frontend: `~/.local/bin/sounda-emulator`, built from
  [`scripts/emulator/`](../scripts/emulator/). It uses the unmodified official
  emulation core, software graphics, interpreted ARM9/ARM7, SD/MMC emulation,
  full DSP LLE, PCM capture and the official GDB remote stubs.
- `arm-none-eabi-gdb` 14.1 was already installed and was used successfully.
- ENet 1.3.18 was built from the official upstream tag and installed under
  `~/.local/` to build the GUI. No system packages or emulator settings were
  replaced.

The local BIOS, firmware and NAND files were found in
`~/Downloads/DS firmware + DSi NAND and firmware/`. No Nintendo firmware was
downloaded. Each automated run opens a separate NAND copy and generated SD
image. The GUI uses a separate configuration under
`~/.local/share/sounda-emulator/gui/`; the existing `~/.config/melonDS/` was
preserved.

Build instructions and BIOS requirements were checked against the
[official melonDS build documentation](https://github.com/melonDS-emu/melonDS/blob/1.1/BUILD.md)
and [official DSi setup instructions](https://melonds.kuribo64.net/faq.php).

## Reproduce

```sh
# Build/install the pinned headless runner, if needed.
python scripts/emulator_check.py --setup

# 200 valid AAC-LC files; folder open, play, repeated scrolling, second song.
python scripts/emulator_check.py --dsp lle \
  --output output/emulator/current-lle

# Three short AAC-LC files at different rates/channels; two manual changes,
# pause during initial buffering, playing pause/resume, EOF wrap and repeat.
python scripts/emulator_check.py --dsp lle --scenario rates --frames 900 \
  --gdb-check --output output/emulator/current-rates

# Unicode filenames, queue edits, fuzzy search, seeks, clock and lid/jack.
python3 scripts/verify_library.py

# Optional separate protocol/ARM audio comparison using melonDS AAC HLE/FAAD.
python scripts/emulator_check.py --dsp hle \
  --output output/emulator/current-hle

# Interactive debugging, isolated from the user's regular emulator settings.
melonDS-sounda-debug output/sounda.nds
arm-none-eabi-gdb target/armv5te-nintendo-ds/release/sounda.elf \
  -ex 'target remote 127.0.0.1:4333'
```

Use `--bios-dir /path/to/local/dumps` if needed. The expected local filenames
are `biosnds9.bin`, `biosnds7.bin`, `biosdsi9.bin`, `biosdsi7.bin`,
`dsifirmware.bin` and `dsinand.bin`. The GUI also supports `SOUNDA_BIOS_DIR`.
Headless runs have no network, camera or microphone access.

`--dsp lle` blocks use of the host AAC decoder and records a decode-call count
of zero. `--dsp hle` is an explicitly separate test: it uses the emulator's
AAC protocol implementation and FAAD rather than executing the original
decoder. An unrecognized HLE firmware can fall back to LLE; check the log and
`host_aac_decode_calls` before interpreting an HLE result.

Each result directory contains `analysis.json`, the exact command, input
events, per-frame timing/PCM/DSP trace, app counters, `audio.wav`, screenshots,
ARM9 RAM, ITCM and NWRAM dumps. `analysis.json` records the ROM hash and checks.
`--analyze-only --output ... --rom ...` regenerates derived audio metrics and
PNG screenshots without rerunning emulation. WAV audio is signed 16-bit,
stereo, 48 kHz captured from the emulated sound output.

## Recorded results, 2026-10-15

The tested final revised ROM is `output/emulator/fixed-v3.nds`, SHA-256:

```text
0fd54df107e5acf5ad44045d14421163b679ce4fd3e4d6866fc275cd7eea8ffb
```

The directory fixture uses FFmpeg AAC-LC, 44.1 kHz stereo, 128 kbit/s, a 440 Hz
tone, in 200 real, valid 30-second M4A files. The rates/controls case contains
three valid 3-second files: 48 kHz stereo/440 Hz, 22.05 kHz mono/660 Hz and
8 kHz mono/880 Hz. These fixtures exercise directory size, codec configuration
and playback timing; they are not a representative music corpus.

| Actual full-DSP-LLE check | 200-file browse run | Rates/controls run |
|---|---:|---:|
| Emulated video frames | 1320 | 900 |
| App clock at last telemetry | 22.032 s | 15.009 s |
| Original-firmware AAC frames decoded | 765 | 487 |
| DSP boots / stops | 2 / 1 | 5 / 4 |
| Host AAC decode calls | 0 | 0 |
| PCM underruns | 0 | 0 |
| Longest DSP decode response | 4 ms | 4 ms |
| Longest audio-pump gap | 17 ms | 17 ms |
| Lowest queued PCM observed | 16,250 stereo frames | 18 frames during EOF drain |
| Maximum GUI draw time, including initial draw | 16 ms | 16 ms |

The ARM9 GDB remote connection was exercised on the tested ROM. It read
`pc=0x02000800`, `sp=0x03002f7c` at `_start`, inspected SCFG registers and detached
successfully before the controls scenario continued. ARM7 has a separate stub
on the next port.

The 200-file run repeatedly pressed Down between emulated frames 620 and 918,
then selected another song at frame 1000. Across the 5.014-second browsing
interval, the captured signal had:

- **0 complete 1 ms windows of digital silence**;
- dominant tone **439.92 Hz**;
- RMS amplitude **2888.84**, with 20 ms RMS windows between **2803.30 and
  2968.52**;
- largest adjacent sample change **273**, against peak amplitude **4207**.

The second-track interval also contained zero complete 1 ms windows of digital
silence. Together with zero reported ring underruns and continuous tone
amplitude, this rejects the reported repeated millisecond cutoffs for these
emulated scenarios. It is not a proof against every possible audible artifact.

The rates/controls test used R at frames 240 and 320, paused with START at
frame 400, resumed at 450, moved the selection while playing at 520–554, and
selected LOOP 1 with X at 600 and 608. A checked 501 ms interval during pause
was entirely silent; a 668 ms interval after resume had continuous tone audio.
The five DSP boots include sequential EOF wrap and repeat-one EOF restart.
The frame-850 screenshot shows `TRACK 000`, `1/3` and `LOOP 1` active. Track
changes and loops still have an intentional rebuffering gap; this player does
not implement gapless playback.

This run also paused at frame 164 during the initial buffer fill and resumed
at 175. Telemetry recorded nine decoded frames and zero PCM callbacks at
frame 164, ten decoded frames/zero callbacks at 165 and 174, then sixteen
decoded frames/zero callbacks at 176, followed by active callbacks at 178.
The checked initial-pause interval was 133 ms of silence, and the run had no
PCM underrun. This directly exercises the final revision's fix that prevents
resume during startup from bypassing the normal 16,384-frame prebuffer.

Measured dominant tones were 440.71 Hz for 48 kHz stereo, 659.58 Hz for
22.05 kHz mono and 879.44 Hz for 8 kHz mono. The short analysis windows have
finite FFT-bin resolution. All three contained nonzero, continuous PCM in
their checked playback intervals.

Definitive reports:

- `output/emulator/final-browse-lle/analysis.json`
- `output/emulator/final-rates-lle/analysis.json`
- `output/emulator/fixed-v2-hle/analysis.json` (supplementary earlier-build HLE result)
- `output/emulator/summary.json`

## Directory baseline and additional DSP loader defect

The saved original local ROM (`output/emulator/baseline/sounda.nds`, SHA-256
`57fadb27ff2dc86ebcdc8ab393e76d5a303373847da4349a11a135f350b773ce`) displayed
the opened 200-file directory at frame **149**, after the A press at frame
**120**. The revised ROM displayed it at frame **123**. That is **484.74 ms
versus 50.15 ms of emulated display latency**, about **9.7 times faster**.
The final revised native directory-scan counter measured **33 ms**. This is a
controlled emulator comparison; it does not establish the same SD throughput
or speedup on the physical DSi.

The same emulator uncovered another real loader defect before baseline audio
could play: `.data@c0@d0` and `const_data@c0@d0` need placements in both program
and data memory. The old suffix check omitted their program placements.
Teakra then entered firmware `_exit` and executed TRAP at program word
`0x0ff3`; the caller return word was `0x0ebf`. The revised parser emits all
**107** placements. Full DSP startup and audio decoding then passed. The
original ROM's HLE program hash was unrecognized and likewise fell back to
this trap. No original-ROM dropout rate is claimed from that failed boot.

## Stock Sound application inspection

The supplied `00000002.app` was also booted directly in an isolated DSi core
for 300 frames. Official melonDS direct-boot code decrypted its ARM9i segment;
ARM9 RAM and ITCM were captured for REA analysis of relocated DSP routines.
This stock application encountered an ARM9 data abort at `0x0205f970` in the
cartridge-style direct-boot environment. That run supports decrypted runtime
code inspection, not a claim that stock Sound playback was validated.

## Remaining physical-console checks

Run the revised release on the affected DSi using the same loader and SD card.
Try the original 200-file folder, several full songs, repeated track changes,
and browsing during playback. The automated runs cover AAC-LC 44.1/48 kHz
stereo and 22.05/8 kHz mono. Headphones/speakers, lid sleep, prolonged SD
stalls, and all these codec configurations still need physical playback
checks. Emulator timing and DSP LLE do not replace those observations.
