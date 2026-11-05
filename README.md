# Sounda

A basic Nintendo DSi SD music player written in Rust. It loads the supplied DSi Sound `aac.a` firmware onto the XpertTeak DSP and sends raw AAC frames to that firmware. The DSP decodes the audio; ARM7/Maxmod streams the resulting 16-bit stereo PCM.

The interface has two original bitmap screens, a compact Unicode folder browser, fuzzy search, an explicit upcoming-song queue, a touch seek slider, a real-time clock, play/pause, stop, previous/next, sequential playback, shuffle, and repeat one song. Software stream and master volume are both **127/127**, with the ARM/DSP output ratio set to full ARM output. The console's physical volume controls remain available.

## Build and install

Requirements: Rust stable (host checks), Rust nightly with `rust-src` (ROM), Wonderful's ARM GCC, and BlocksDS including Maxmod. No Cargo packages or Teak compiler are required. The supplied `aac.a` must remain in this project root; the build embeds it unchanged in the ROM. Neither `00000002.app` nor `texture.szs` is needed.

```sh
make                  # output/sounda.nds and dist/sounda/sounda.nds
make test             # host tests plus FFmpeg demux validation
make check            # formatting, host and ARM clippy
make preview          # renders the same Rust GUI to output/preview.ppm
```

Copy **`output/sounda.nds`** anywhere on the SD card. Add music folders containing `.m4a` files anywhere on that card. Launch Sounda in **DSi mode** using a loader that grants SD, DSP, SCFG and NWRAM access. A DS-mode flashcart launch cannot provide the DSi DSP. The browser starts at `sd:/`. Selecting a song starts automatic playback from that folder, opening one media file at a time. The upcoming queue stays empty until you explicitly add songs. You can browse other folders while music plays.

The locally built ROM contains your supplied firmware. Generated ROMs and the input dumps are excluded from source control; rebuilding from a source checkout requires supplying `aac.a` yourself.

## Controls

| Control | Action |
|---|---|
| Touch a folder/song; A | Open folder / play selected song |
| D-pad up/down | Select entry |
| D-pad left/right | Page through the visible list; move the selected upcoming entry up/down in QUEUE |
| B; upper-left arrow | Close search/queue; clear a library filter; then parent folder |
| SD / QUEUE / FIND tabs | Folder browser / queue manager / filename search |
| Upper-right arrow | Page up |
| Drag the right-hand list scrollbar | Scroll directly through the folder, results or queue |
| QUEUE: TOP / 2ND / BOTTOM | Move the selected upcoming song to first, second or last |
| START; play/pause button | Play / pause / resume |
| L / R; previous/next buttons | Previous / next song |
| Y; search ADD button | Add selected song to the queue; Y removes the selected entry in QUEUE |
| STOP touch button | Stop playback |
| Touch/drag the bar above playback buttons | Preview position; release to seek once |
| X; ORDER / SHUFFLE / LOOP 1 buttons | Change playback mode |
| SELECT | Toggle queue manager |
| SELECT + START | Return to loader |
| Close lid with earphones | Continue playback with both backlights off |
| Close lid without earphones | Suspend playback, DSP clock, UI, directory scanning and search |
| Open lid / attach earphones while closed | Resume if playback was automatically paused |

Explicitly queued songs play in their chosen order before automatic folder playback continues. ORDER advances alphabetically through the automatic folder and wraps at its end. LOOP 1 repeats on natural completion; previous/next still changes tracks. Shuffle's previous button follows the last **10 played visits**, even across shuffle cycles; Next retraces that history after going back. Queue edits preserve the current song and playback history. Entries leave the upcoming queue when they start playing; removing an upcoming entry does not erase previously played visits. Playback errors stop and display an explanation; they do not silently skip songs.

## Search, queue and seeking

FIND searches the current folder incrementally without rereading the SD. Latin accents and case are folded (`e` matches `é` and `ë`, `zolc` matches `Żółć`); ordered partial letters and small spelling mistakes are accepted and ranked. Cyrillic is case-insensitive, and hiragana/katakana and full/half-width forms are equivalent. The original names are used to open files.

The touch keyboard has ABC, CYR and KANA layouts. `>` cycles pages. FILES offers the non-ASCII letters and ideographs present in this folder, allowing Chinese/Japanese searches without an IME. DEL removes one character. B leaves search with its results in the browser; B again clears the filter. Queries are limited to 48 characters.

The queue contains **only explicitly added, upcoming songs**. Y or ADD appends songs, including from other folders, without interrupting playback. They play first-in, first-out on natural completion or Next, including in shuffle mode. An automatic folder selection never fills the queue with the other 200 songs. When the explicit queue runs out, automatic playback continues after the last automatic folder song. For example: while automatic song A plays, add C and E; playback becomes C, E, then the automatic folder's B. LOOP 1 keeps repeating the current song on natural completion; Next still consumes an upcoming song.

QUEUE has TOP, 2ND and BOTTOM shortcuts, DEL removes the selected entry, and left/right moves it one place. The currently playing song is separate from this list, so these edits never stop it. Touch an entry to select it and again to play it immediately; A also plays it, removing it from the upcoming list. ADD returns to the library. The queue holds up to 2048 upcoming songs in RAM for this session. If playback started solely from the queue with no automatic folder, it stops when the queue is exhausted.

Playback buttons are 16 pixels high, with six 18-pixel Unicode song rows on the touch screen. Browsing, folder changes, queue navigation or search temporarily adds eight list rows on the top screen: **14 visible entries**, or **13 while the search keyboard is open**. The screens form one list from top to bottom; D-pad and A/Y work with the selected entry on either screen. Only the lower screen accepts touch. After **10 seconds without interaction**, the top returns to artwork, current song, progress and clock; selection remains in view on the lower screen. Dragging a scrollbar or holding the stylus keeps browsing active. Clock ticks and audio activity do not reset that timer.

The slider commits one seek on release. It locates the AAC frame directly, restarts the decoder and primes the two preceding frames before playing the target; it does not decode the whole preceding song. Seeking has 1024-sample precision (about 23 ms at 44.1 kHz) and a short rebuffering gap. A paused seek remains paused. The top-right HH:MM:SS clock uses the console's configured local RTC time and refreshes once per second.

Closing the lid disables both backlights. Earphones keep playback and automatic song changes running while drawing, directory scans and search are suspended. Without earphones, PCM output and the DSP clock stop as well; opening the lid or plugging earphones back in restores the unplayed audio tail. A manually paused song remains paused. Only the small ARM7 lid/jack monitor, system interrupts and wake checks remain active; this is application suspension, rather than the SDK's lid-only deep sleep, so inserting earphones can resume playback without opening the lid. See [validation and hardware limits](docs/library-features.md).

## Supported music

Unencrypted, nonfragmented **AAC-LC** in `.m4a`, mono or stereo, at 8, 11.025, 12, 16, 22.05, 24, 32, 44.1 or 48 kHz. The extension alone does not establish the codec: ALAC, HE-AAC, DRM, multichannel audio and raw ADTS files are rejected. The firmware's documented input limit is **1700 bytes per compressed frame**.

The reader follows `stsz`, `stsc`, `stco`/`co64`, and the AAC decoder configuration. It supports `moov` before or after `mdat` and interleaved chunks. It buffers PCM rather than entire songs. Limits: 262144 AAC frames per file (about 93 minutes at 48 kHz), files below 2 GiB, 2048 visible folders/music entries per directory, and 1023-byte paths. A visible notice marks a truncated directory listing. Filenames and folder names are preserved in UTF-8. A prebuilt 31,538-glyph font displays Latin and Polish, Cyrillic, kana, and common simplified/traditional Chinese and Japanese ideographs, including CJK Extension A. Rare supplementary-plane ideographs use a square fallback. See [font coverage and license](docs/third-party-fonts.md).

AAC encoder delay/padding and MP4 edit lists are not removed, so loops and track changes are not gapless. Output uses the DS sound channel rate divider and DSi's approximately 47.61 kHz DAC mode; it should not be described as bit-perfect 48 kHz output. Unsupported files can be converted on a computer, for example:

```sh
ffmpeg -i input.flac -c:a aac -profile:a aac_low -b:a 192k -ar 44100 -ac 2 song.m4a
```

## Implementation and verification

`src/firmware.rs` validates the linked Teak COFF sections before assigning NWRAM B/C. The C adapter copies the selected code/data, starts the DSP, and exposes nonblocking CMD1/REP polling. `src/player.rs` implements the startup handshake, decode commands and timeouts. Firmware state is reset on each new song so decoder overlap/history cannot bleed between files.

`src/mp4.rs` is a bounded Rust M4A demuxer. `src/read_ahead.rs` groups compressed music reads into 64 KiB, sector-aligned reads after metadata parsing. `native/nds.c` holds aligned DSP DMA buffers and a 65536-frame PCM ring. A dedicated timer interrupt refills Maxmod's 8192-frame output buffer independently of UI and filesystem work. It copies only ready PCM: no SD access, allocation, DSP waits or Rust runs inside that callback. Maxmod's manual sample clock is retained for accurate pause/resume accounting because this SDK does not implement position queries in its automatic mode.

The retained interface follows Devy's dirty-region approach on the DS main/sub 2D bitmap backgrounds. It redraws and uploads changed rows, and services the decoder between changed widgets. Directory enumeration uses the file type already returned by FatFs, without a second `stat` lookup per song; scanning has a 32-entry limit and a 2 ms soft budget per batch. See [UI and MoonShell source findings](docs/ui-browser-performance.md).

The second-song failure was caused by treating `SCFG_EXT_DSP` as a lasting permission. BlocksDS clears that bit when powering off the DSP, so Sounda incorrectly rejected its own shutdown state. Preparation now checks DSi/NWRAM access and mapping results, then verifies DSP access after powering it on.

The firmware loader also preserves sections marked for **both** program and data memory (`@c0@d0`). Missing their program copies damaged runtime initialization when the firmware restored its data constants. The supplied firmware file itself is unchanged.

Host validation, compiled ROM validation and hardware playback are separate. See [docs/verification.md](docs/verification.md) for the recorded results and remaining DSi checks. The preview uses the real Rust renderer; it is not an emulator screenshot.

Full DSi DSP emulation, captured audio, repeated song changes and a 200-file
browser scenario are now covered by a reproducible
[melonDS/GDB validation harness](docs/emulator-validation.md). The revised ROM
still needs testing on the affected physical DSi and SD card.

[DSi Sound reverse-engineering findings](docs/research/dsp-investigation.md)
record the REA/Ghidra evidence, original six-frame decoder ring, DSP command
format, installed tools, and remaining output-quality questions.

Protocol references: [melonDS developer's DSP binary notes](https://melonds.kuribo64.net/board/thread.php?pid=7797), [DSi AAC protocol investigation](https://melonds.kuribo64.net/comments.php?id=231), [Gericom's COFF loader](https://github.com/Gericom/DSiDsp/blob/master/arm9/source/libdsp/DspProcess.cpp), and [BlocksDS DSP loader](https://github.com/blocksds/libnds/blob/master/source/arm9/teak/process.twl.c). Sounda uses an independent small adapter against the installed SDK; these links document the hardware protocol and file layout.
