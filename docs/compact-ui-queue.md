# Compact browsing and explicit upcoming queue

The current release uses smaller controls, extends the list across both screens while browsing, and keeps automatic folder playback separate from explicitly queued songs. The final ROM SHA-256 is `a9f2c7b13c2bda1932e34fea248ee88b3414f26945a08836e4c81baeea64dbb3`. The supplied `aac.a` is unchanged: `6770fe97550eb70f27a0f11612cfba31bb800ae9b4d5b87745c3c673751951b8`.

## Screens and controls

Playback buttons are 16 pixels high instead of 28, header/mode/queue buttons are 14 pixels high, and Unicode filename glyphs keep their 16-pixel height. The lower screen now displays six songs. Browsing, opening a folder, changing queue selection, dragging the list or editing search adds eight rows on the top screen. This produces 14 visible entries in library/queue view and 13 with the compact search keyboard.

The two screens form a continuous list, read from top to bottom. The D-pad selects entries on either screen; A plays/opens, Y adds a library selection or removes an upcoming queue entry. Only the lower screen accepts touch. Its right-hand scrollbar accepts both track taps and thumb dragging, covers the whole filtered/current list, and clamps at the first/last page. Grabbing the thumb retains the finger's offset instead of jumping its center to the touch point. Scrolling changes selection without opening a file or seeking the current song.

After 10,000 milliseconds without interaction the top screen returns to the retained player artwork, title, position and clock. The lower viewport shrinks around the same selected entry. Typing/navigation expands it again; a held stylus prevents timeout. Clock ticks, audio decoding and automatic song changes do not reset the inactivity timer. The clock stays in the top-right corner in both views.

Rendering continues to use retained RGB555 main/sub 2D backgrounds and dirty scanline uploads. The renderer retains six lower and eight upper row records, updates changed rows and yields to the decoder between components. Switching top-screen modes requires one full top redraw; ordinary moves and clock changes remain partial. Idle now-playing selection changes still upload only 36 lower scanlines. One elapsed/seek update covers 20 top and 14 bottom scanlines (17,408 bytes), and an RTC tick covers 20 top scanlines (10,240 bytes).

## Queue behavior

`src/playback.rs` separates three kinds of state:

- The automatic folder cursor and cached file paths, used to choose the next file on demand. Only one M4A media file is open/decoded at a time.
- The explicit FIFO of upcoming songs, containing only Y/ADD selections, including paths in other folders.
- The current song and last ten actual playback visits, stored independently of upcoming entries. Previous walks this history and Next retraces retained forward visits before generating new playback.

Starting a song in a folder with 200 files leaves the upcoming queue empty. Adding C and E while automatic A plays gives C, E, then the automatic folder's B. The queue consumes an entry when it starts. Automatic folder navigation never appends all its songs to the explicit queue. The same explicit FIFO priority applies in shuffle mode; LOOP 1 repeats the current song on natural completion, while manual Next can advance through the queue.

TOP moves the selected upcoming entry to position one (plays next); 2ND moves it behind the first upcoming entry; BOTTOM moves it to the end. Left/right still moves it one place. DEL/Y removes an upcoming song, and either ADD button returns from QUEUE to the library. A, or tapping an already selected queue entry, plays that entry immediately and removes it from the pending list. Editing/removing pending songs preserves the current stream, DSP state and playback history. If no automatic folder was established, playback stops after the explicit queue drains. Session limits remain 2048 upcoming songs and ten prior playback visits.

The player seek slider, Unicode/accent folding, maximum software volume and headphone-aware lid suspension are retained. See [README controls](../README.md#controls).

## Reproduce and inspect

```sh
make test
make check
make
python3 scripts/emulator_check.py --setup  # build the pinned runner if needed
python3 scripts/verify_library.py --output output/emulator/compact-release-lle
python3 scripts/verify_library.py --output output/emulator/compact-release-lle --analyze-only
```

The generated 200-file SD folder includes Polish, Cyrillic, Japanese, Chinese and accented Latin names. The runner uses interpreted ARM9/ARM7 and the supplied original firmware in Teakra DSP LLE, with host AAC decoding disabled. It uses an isolated generated SD image and a copy of the existing local NAND; no real SD card is modified.

The scenario covers the initial genuinely empty queue; adding four entries; TOP/2ND/BOTTOM and removal without restarting audio; draggable scrollbar endpoints; 14/13-row viewports; typed `beyonce` matching `Beyoncé`; natural EOF choosing explicit songs before automatic folder playback; 10-second top-screen restoration; renewed browsing/search; clock updates; and closed-lid headphone unplug/replug plus manually paused wake behavior. Per-frame telemetry checks all observed PCM underrun counters, including before song resets. Captured PCM checks continuity while dragging and across the idle layout change, plus audible/silent intervals for the lid policy.

All **49 checks passed** on the recorded release ROM: **1376 AAC frames decoded**, **6 DSP boots**, and **zero observed PCM underruns**. Captured audio has zero digitally silent milliseconds while dragging the list and across the ten-second layout change; the unplugged-lid and manual-pause intervals are silent. The emulated native directory enumeration measured 52 ms for 200 entries. These timings describe the emulator, not the physical SD card.

Artifacts are in `output/emulator/compact-release-lle/`: `analysis.json`, `rom.sha256`, exact events/command, `diagnostics.csv`, `trace.csv`, `audio.s16le`, RAM/NWRAM dumps and screenshots. Useful screenshots are `frame-00193.png` (empty queue while playing), `frame-00310.png` (queue move), `frame-00430.png` (last list page), `frame-00550.png` (search), and `frame-01320.png` versus `frame-01370.png` (before/after inactivity).

The separate rate regression in `output/emulator/compact-rates-lle` passed 900 frames with 48 kHz stereo, 22.05 kHz mono, 8 kHz mono, pause during startup buffering, ordinary pause/resume, sequential EOF and repeat-one, with zero PCM underruns. That run uses hash `a3116909d4d0b6e9e7dc328c97a6b0dde2dcee4d779684ed23206cceb0ff8fc1`, before only the empty-queue hints and header ADD navigation were corrected; its PCM/controller code is the same.

The 33 Rust tests include the queue priority/drain rules, jump targets, mixed automatic/queued shuffle history, timeout including timer wrap, variable viewport bounds, shared scrollbar geometry, and complete-versus-retained framebuffer equivalence during top-screen/list transitions. Existing C sanitizer tests and real FFmpeg AAC/seek byte comparisons also pass. Host preview screenshots use the actual renderer and are separate from emulator screenshots.

## Remaining device validation

The revised ROM still needs testing on the intended physical DSi and SD card. Check stylus target size, scrollbar dragging in large filtered folders, repeated idle transitions, queue priority and all jump buttons, and headphone/lid transitions while audio continues. Emulator results do not measure physical SD latency, analog sound quality or battery savings.
