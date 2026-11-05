# Verification

Checked on 2026-10-15. The user tested the original ROM on a physical Nintendo DSi and reported repeated audio cuts, worse cuts during navigation, slow directory opening, and a DSP access error on the second song. **The revised ROM still requires a physical DSi retest.** Host checks and emulator observations below have separate validation boundaries.

## Completed

- Built an ARM9 Rust `no_std` release ELF and packaged a DSi-only ROM with a small Maxmod ARM7 core adapted from BlocksDS, including DSi headphone GPIO reporting. No runtime Cargo dependencies or replacement software AAC decoder.
- Inspected the NDS header and banner: DSi unit code `3`, valid header/logo/banner CRCs, ARM9/ARM7 and DSi sections present.
- Confirmed that the ROM includes exactly one unchanged copy of the provided 694904-byte `aac.a`. The corrected parser selects 107 COFF placements and validates the code/data ranges before loading. Two mixed `@c0@d0` sections must be copied into both spaces; their program copies were missing previously. Firmware CRT startup copies those constants back into data memory, so omitting them caused an observed DSP startup trap in full emulation.
- Rust host tests cover supplied firmware layout, malformed firmware, malformed MP4 boxes, AAC-LC/HE-AAC channel/rate configuration, SD path handling, playback mode transitions, shuffle history, and touch targets.
- The actual C PCM transport is compiled on the host with address/undefined-behavior sanitizers and a simulated Maxmod clock. Tests cover stereo pause/resume, mono duplication, maximum software volume, short-file EOF, full-buffer overflow, ring wraparound, underrun silence, and pausing with audio queued after an underrun. This exercises buffer accounting, not ARM7/DSP execution or sound hardware. LeakSanitizer is disabled because this workspace runs under ptrace; the transport uses static buffers.
- Six generated AAC fixtures have been demuxed by Rust and FFmpeg. **Every compressed AAC byte matches**, as do rate, channel count and frame count: stereo 44.1 kHz, stereo 48 kHz with faststart, mono 22.05 kHz, mono 8 kHz, 64-bit chunk offsets, and audio interleaved with video. ALAC and five truncated files are rejected. Reproducible report: `output/media-tests/results.json`.
- The PCM regression test simulates interrupt-driven refill while the main loop does no audio servicing for more than half a second. It also exercises thousands of four-frame callbacks: contiguous ledger segments coalesce rather than exhausting the old 128-entry ledger and inserting silence despite queued PCM.
- Retained rendering is checked against the complete renderer after uploading only the reported scanlines, including navigation through a 200-song list. An ordinary visible selection move uploads 18,432 bytes instead of 196,608; an unchanged tick uploads zero. These are tested transfer sizes, not measured device FPS.
- Compressed-data read-ahead tests check every returned byte across cache boundaries and nonsequential seeks; 300 adjacent 777-byte frame reads use four underlying 64 KiB reads.
- Pausing/resuming during initial buffering retains the startup threshold; a one-frame queue no longer triggers an 8176-frame request padded with silence.
- The actual Rust framebuffer renderer was used to generate and visually inspect the normal and error screens: `output/preview.png` and `output/preview-error.png`. Those are host previews.

## Compact browsing and explicit upcoming queue

The latest update adds six touch-screen rows, eight additional top-screen list rows during browsing (13 total with the search keyboard), a ten-second return to now-playing, a draggable list scrollbar, and TOP/2ND/BOTTOM queue moves. Only explicitly added upcoming songs appear in the queue; automatic folder playback opens one file at a time and yields to these songs. The 33 Rust tests, sanitizer-backed transport tests, media/seek comparisons, host/ARM Clippy and release build pass. Latest emulator evidence and exact ROM hash are in [compact UI and queue validation](compact-ui-queue.md).

The Unicode/queue update below describes the preceding ROM; its original whole-folder queue behavior is superseded by this change.

## Previous Unicode, queue, seek, clock and lid update

The preceding ROM (`94eb2dd3799933bdf0b5ed98d6134c539553340de1d3d163345223286df1bcf9`) adds multilingual rendering/search, an editable queue, ten previous shuffle visits, a seek slider, an RTC clock and headphone-aware lid suspension. **25 Rust tests**, six sanitizer-backed PCM transport tests, six real M4A/FFmpeg byte comparisons (each including six random/backward/EOF seek checks), host/ARM Clippy and the release build pass.

The new 1,860-frame full DSi/DSP LLE run uses a 200-file Unicode folder and the original embedded firmware. It decoded **2,598 AAC frames**, booted the DSP **28 times**, and had **zero PCM underruns** over the whole trace. It checks accent-insensitive keyboard search, add/reorder/remove without resetting the playing track, ten backward shuffle visits and forward replay, once-per-release seeking including a paused seek, clock updates without erasing the logo, and repeated lid/jack transitions. Captured PCM confirms both seek positions using a chirp, silence while suspended and continuing audio with closed-lid headphones. The directory scan measured 32 ms in emulated time. See [feature evidence and remaining physical checks](library-features.md).

The earlier audio repair results below remain a separate historical comparison. The rate/control regression also passed after introducing Unicode/UI and ARM7 GPIO support; its artifact hash is recorded in `output/emulator/library-rates-lle/analysis.json`.

## Full DSi emulator results

The final revised ROM was run through the official melonDS 1.1 core with the
original firmware executing in DSP LLE and **zero host AAC decoder calls**.
The 22-second 200-file browsing scenario decoded 765 AAC frames, booted the
DSP twice, and recorded zero PCM underruns. The 15-second control/rate scenario
booted the DSP five times and decoded 487 frames with zero underruns, including
48 kHz stereo, 22.05 kHz mono and 8 kHz mono, pause during initial buffering,
normal pause/resume, sequential EOF wrap and repeat-one restart.

Captured tone audio remained continuous while scrolling. The controlled
200-file directory comparison displayed the list after 50.15 ms, versus
484.74 ms with the original ROM; the final native scan measured 33 ms.
These are emulator measurements, not physical SD-card throughput or a claim
that the revised player matches DSi Sound's analog output quality.

See [emulator commands, measurements and artifacts](emulator-validation.md)
and [Devy/MoonShell rendering and directory findings](ui-browser-performance.md).
[REA/Ghidra and Teakra findings](research/dsp-investigation.md) include the
stock application's loader, decoder, ring-buffer and ARM7 output evidence.

A separate vendored-Teakra DSP probe also ran the unchanged firmware across
251 AAC frames in five streams, including a same-rate command-zero reset and
four sample-rate/channel configurations, without a decode error. Its PCM had
the same lengths as independently decoded FFmpeg references, with waveform
correlations above 0.9999998. This checks decoder agreement; it does not measure
the physical mixer or DAC.

Commands:

```sh
make test
make check
make
make preview
```

## DSi validation still required

1. Launch the generated ROM from the internal SD card in DSi mode with DSP and NWRAM access. Verify the folder list and that selecting a song progresses past the firmware handshake.
2. Play known mono and stereo AAC-LC tracks at 44.1 and 48 kHz; verify pitch, tempo, channel separation, full software volume and audible output on speakers/headphones.
3. Pause/resume repeatedly, including during initial buffering and near EOF. Try short songs, previous/next, sequential wrap, a full shuffle cycle and repeat one.
4. Browse large/deep folders during playback, search Unicode/accented names, edit the queue and seek in both directions. Close/reopen the lid and unplug/replug earphones while closed, including during buffering and a manual pause. Exercise unsupported files. Confirm that failures display a message and the app remains responsive.
5. Measure underruns and timing over long playback on the intended SD card. Host results do not support any performance or sound-quality claim.

## Debugger counters

The ARM9 ELF exports `sounda_diagnostics`, a 40-element array of unsigned
32-bit words. Its physical-RAM snapshot is flushed at most once per 16 ms while
input/wake state is being sampled, so an emulator can read it without ARM9 cache
snooping. Framebuffer generation and transfers remain independently measured.

| Index | Meaning |
| --- | --- |
| 0 / 1 | Magic `0x534e4441` / layout version `3` |
| 2 / 3 | DSP boot / shutdown count |
| 4 / 5 | Decoded AAC frame count / PCM underrun count |
| 6 / 7 | Queued source PCM frames / minimum queue depth since track reset |
| 8 / 9 | Maximum DSP completion polling interval / maximum player service gap, ms |
| 10 / 11 | Changed redraw count / maximum redraw and presentation time, ms |
| 12 / 13 | Last completed directory entry count / scan time, ms |
| 14 | Reserved |
| 15 / 16 | DSP status / SCFG_EXT snapshots |
| 17 | Application milliseconds at last service |
| 18 / 19 | PCM callback count / maximum simultaneous ledger segments |
| 20 / 21 / 22 | Pane (0 library, 1 queue, 2 search) / queue length / zero-based current folder entry (`0xffffffff` for an external song) |
| 23 / 24 / 25 | Elapsed seconds / duration seconds / seek fraction (0..65535) |
| 26 / 27 | Lid state (0 open, 1 closed with headphones, 2 suspended) / headphone GPIO |
| 28 / 29 | Search pending / visible library entries |
| 30 / 31 | Committed seek count / paused flag |
| 32 / 33 / 34 / 35 | Expanded top list / selected visible entry / first visible entry / visible row capacity |
| 36 | Selected upcoming queue entry |
| 37 / 38 / 39 | FNV-1a hash of current full path / player loaded / hash of first upcoming full path |

Counters aid diagnosis; zero PCM underruns alone does not establish DAC fidelity,
correct pitch, or the absence of a hardware/interrupt scheduling fault.

The firmware command layout follows the [melonDS developer's reverse-engineering notes](https://melonds.kuribo64.net/board/thread.php?pid=7797). The COFF memory placement agrees with the [reference loader](https://github.com/Gericom/DSiDsp/blob/master/arm9/source/libdsp/DspProcess.cpp), and Maxmod's manual callback and sample-clock behavior were checked against the installed SDK and [upstream implementation](https://github.com/blocksds/maxmod/blob/master/source/ds/common/stream.c). The SDK's [DSP power routines](https://github.com/blocksds/libnds/blob/master/source/arm9/teak/dsp.twl.c) also establish why the previous second-song capability check was wrong.
