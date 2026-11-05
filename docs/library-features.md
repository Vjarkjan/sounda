# Unicode, queue, seek, clock and lid validation

Historical Unicode/lid update, checked 2026-10-15. The current [compact UI/upcoming-queue update](compact-ui-queue.md) supersedes the full-folder queue, four-row layout and test script scenario described here.

Checked 2026-10-15. The ROM SHA-256 is `94eb2dd3799933bdf0b5ed98d6134c539553340de1d3d163345223286df1bcf9`. The supplied `aac.a` remains unchanged (SHA-256 `6770fe97550eb70f27a0f11612cfba31bb800ae9b4d5b87745c3c673751951b8`). No replacement AAC decoder or runtime Rust dependencies were added.

## Behavior

[README controls](../README.md#controls) describe FIND, the touch keyboard, queue editing, previous/next history and the seek slider. Filename rendering uses prebuilt 8x16/16x16 glyphs; [coverage and font license](third-party-fonts.md) describe the BMP repertoire and square fallback for rare supplementary ideographs. Search caches normalized names, considers 16 per step, folds Latin accents/case/width and kana forms, and ranks prefix/substring/subsequence matches ahead of bounded spelling corrections. SD paths keep the original spelling. Searching is limited to the current folder.

Queues use full SD paths and preserve user order. Appending, reordering and removing a different entry do not reset the DSP. Removing the playing entry loads its successor while preserving pause. Clearing the last entry stops playback. Shuffle retains the current visit and ten prior visits; previous/next walks that same history, and edits remap track indices. Removing a song removes its history visits. Queue/history are session state and are not saved to SD.

A seek directly locates the AAC frame through the MP4 chunk/run tables, resets the output ring/decoder, decodes and discards two preceding access units, then enqueues the target. Dragging previews the time while playback continues; release commits once. A paused seek remains paused. Precision is one 1024-sample AAC frame and seeking includes a short rebuffering gap. Encoder delay, edit lists and gapless playback remain outside the supported feature set.

The HH:MM:SS clock reads the console local RTC once per second while open. A tick redraws only its widget and uploads 20 top-screen scanlines. Main/sub RGB555 2D backgrounds and the independent PCM refill IRQ remain in use. Four list rows leave room for touch seeking and queue controls.

## Lid and earphones

The ARM7 reads `REG_GPIO_DATA & GPIO_HEADPHONE_CONNECT` and reports initial state and changes on `FIFO_USER_01`, which is separate from Maxmod/system/storage channels. [BlocksDS GPIO definitions](https://github.com/blocksds/libnds/blob/master/include/nds/arm7/gpio.h) and [GBATEK GPIO documentation](https://problemkaputt.de/gbatek.htm#dsigpioregisters) define this DSi ARM7 input; bit 5 is high when connected. The custom ARM7 preserves standard sound, storage, touch, power-button and RTC services.

| Lid/jack | Application work |
| --- | --- |
| Open | Normal playback, UI, clock and browsing |
| Closed, earphones attached | Playback and track changes continue; UI/clock/scanning/search stop; backlights off |
| Closed, no earphones | Playback pauses; PCM refill timer and DSP clock stop; all UI, clock, SD scan and search work stop |
| Reopen or attach earphones while closed | Restore the previous playback state and buffered tail; manually paused music stays paused |

ARM9 halts between wake/input interrupts during suspension. The small ARM7 input/jack monitor, SDK time/power services and wake-state checks remain active. This does **not** invoke `systemSleep()` (which only wakes on opening the lid), so plugging in earphones can wake playback while closed. It is not a measurement or claim of deep-sleep battery consumption. The decoder's timeout deadlines exclude the interval with its clock gated, and its AAC state and unplayed PCM remain intact.

## Automated evidence

Run after building the ROM and [pinned headless emulator](emulator-validation.md):

```sh
make test
make check
make
python3 scripts/verify_library.py
python3 scripts/verify_library.py --analyze-only
```

The script generates 200 valid 40-second AAC-LC files, including Latin/Polish, Cyrillic, Japanese and Chinese UTF-8 names in a Unicode folder. It uses a private SD image and a private copy of the existing local NAND. It executes 1,860 frames (about 31 seconds) in official melonDS 1.1 with interpreted ARM9/ARM7 and Teakra DSP LLE. The core is unmodified; the test frontend injects lid state and the headphone GPIO as sensor inputs. Host AAC decoding is disabled and its call count remains zero.

All 40 checks pass in `output/emulator/library-lle/analysis.json`:

- 2,598 AAC access units decoded, 28 DSP boots, zero observed PCM underruns or emulator exceptions.
- 200 Unicode directory entries, with a 32 ms emulated native enumeration time. `beyonce` typed on the touchscreen finds the original `Beyoncé` filename. Opening the Cyrillic track succeeds.
- Append, reorder and remove preserve the DSP boot count and currently playing song; the shuffled queue returns through ten actual prior visits, then Next follows the retained forward history.
- Two slider gestures commit exactly twice. The displayed forward position is about 24 seconds and the backward paused position about 6 seconds. The generated chirp increases 20 Hz per source second; captured output peaks at 711.92 Hz after seeking forward and 350.98 Hz after seeking backward, confirming the actual decoded position changes.
- Closed-lid headphones have captured-audio RMS 1961.489; unplugging gives exact digital silence and freezes both decoder and redraw counters. Replugging while closed resumes audio (RMS 1962.488) without a DSP restart. A second unplug/open cycle and a manually paused close/open cycle also pass. Early-track suspension resumes without a timeout.
- Clock pixels change while the logo pixels remain identical.

Artifacts include exact events/command/ROM hash, per-frame `diagnostics.csv` and PCM/DSP `trace.csv`, `audio.s16le`, isolated RAM/NWRAM dumps and screenshots (for example `frame-00380.png`, `frame-00650.png`, `frame-01260.png`). The runtime queue is only three songs here so repeated shuffle visits and edits are easy to inspect; a separate Rust test exercises a 30-song bag and all ten previous visits across reshuffles. Host renderer tests compare retained/partially uploaded images against full renders for all panes, Unicode text, empty results and slider/clock changes.

The rate/control regression in `output/emulator/library-rates-lle` also passes 900 frames: 48 kHz stereo, 22.05 kHz mono, 8 kHz mono, initial buffering pause, normal pause/resume, sequential EOF wrap and repeat-one. That run predates the small fix which excludes queued-but-unplayed entries from history; it is a separate ROM hash. Existing sanitizer-backed C tests cover exact retained PCM tails and maximum software volume. FFmpeg comparisons check six real M4A layouts and six direct seek positions per file.

## Physical DSi checks remaining

Emulator sensor injection does not establish physical jack polarity/wiring behavior, SD latency, audio quality, or battery savings. On the intended DSi, test a 200+ song Unicode folder, a queue spanning folders, all ten previous shuffle visits, seeking near the start/end and while paused, and closing/unplugging/replugging/opening during buffering and normal playback. Confirm that manual pause survives opening the lid and that the console RTC agrees with the top clock. The revised ROM has not yet been tested on the affected physical DSi.
