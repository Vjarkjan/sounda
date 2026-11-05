# Directory and UI performance evidence

The reports from the physical DSi are the starting observations: audio breaks
up more while navigating the file list, and a folder of about 200 songs takes
too long to open. These observations do not establish that drawing resets the
DSP. The original UI path did, however, rerasterize and upload both complete
screens on every navigation event and every 200 ms.

## Source references inspected

The local Devy checkout is `../devy`:

- `src/video/frame.rs:138-150` records changed spans per scanline and skips
  pixels that already have the requested value.
- `src/video/mod.rs:114-152` uploads only those spans, then clears the damage.
- `native/nds.c:18-36` uses the main and sub **2D engines**, with a main RGB555
  BG3 and a sub indexed BG3.
- `src/background/mod.rs` also offers hardware tile backgrounds and sprites
  through the Devy runtime. Those facilities have their own video ownership
  and are not drop-in additions to Sounda's existing background initialization.

Sounda applies Devy's retained-background and partial-transfer approach in
`src/ui.rs`. It keeps RGB555 on both screens and records changed scanlines.
Retaining the widget state also avoids rasterizing unchanged artwork, labels,
and buttons. The existing main/sub BG3 bitmap backgrounds already use the DS
2D engines; no 3D renderer is involved. Adding Devy's entire Bevy runtime would
not by itself remove the repeated drawing, and would introduce another owner
of VRAM and the frame loop. The change therefore adopts the relevant rendering
design without adding that runtime dependency.

The MoonShell source archive inspected is:

`~/Downloads/moonshello(1)/200901161857_moonshell-2.00beta5-src.zip`

SHA-256:
`e556fabf4d33fecf3fd8e07c1cc97fff1b853d08b8788765313c50bb0cae2f46`

Paths below are relative to its
`200901161857_moonshell200beta5_sources/arm9/source/` directory:

- `procbody/proc_FileList/proc_FileList_NDSFiles.h:85-232` enumerates with
  `FAT2_FindFirstFile`/`FAT2_FindNextFile`, receives the entry's file/directory
  type and FAT attributes directly, and caches its name. Sounda's old native
  iterator instead constructed a complete path and called `stat` for every
  entry, repeating work the FAT directory iterator already knows.
- `procbody/proc_FileList/proc_FileList.cpp:980-1019` refreshes clock information
  about once per 60 VSyncs and uses `ScreenRedrawFlag` and
  `ForceUpdateSubScreenFlag` to avoid unnecessary redraws.
- The same file at `1023-1038` loads additional NDS icons only until the VBlank
  deadline. Sounda uses fixed folder/music icons, so it needs no per-song icon
  reads or metadata parsing while listing a directory.
- `proc_FileList_NDSFiles.h:235-269` uses a quadratic comparison sort. This part
  is not a useful optimization to copy: Sounda retains Rust's in-place
  `O(n log n)` sort, with folders first and case-insensitive filename ordering.

These are source observations, not a benchmark comparison with MoonShell or
Devy, and no MoonShell code has been copied into Sounda.

## UI behavior and verification

`ui::Renderer` owns only the previous widget state and six lower-screen and eight upper-screen filename
copies. The caller retains the two RGB555 framebuffers. `Damage::top` and
`Damage::bottom` return contiguous dirty scanline runs for partial presentation.
A caller-supplied service callback runs after each changed component or file
row; unchanged redraw requests perform no rasterization or presentation.

The host tests simulate a display that copies **only** the reported runs, then
compare its complete output with the original full renderer. Cases include
scrolling through 200 songs, moving back, empty folders, replaced filenames,
scan-state changes, play/pause, playback modes, long/short titles, timer updates,
and appearing/disappearing errors. Thus they check both pixel output and dirty
coverage, including erasure of old text and scrollbars.

The covered update sizes below apply when the top screen shows now-playing. The expanded browser retains its own top rows and summary. Switching the top screen between listing and player requires a full top upload once; ordinary navigation and clock changes remain partial. See [compact layout evidence](compact-ui-queue.md).

The covered update sizes are:

| Update | Top rows | Bottom rows | RGB555 upload bytes |
| --- | ---: | ---: | ---: |
| Original full update | 192 | 192 | 196,608 |
| Move selection by one visible row | 0 | 36 | 18,432 |
| Advance song time / slider in now-playing view | 20 | 14 | 17,408 |
| Advance real-time clock by one second | 20 | 0 | 10,240 |
| Repeated update with unchanged state | 0 | 0 | 0 |

These are exact dirty-region sizes asserted by host tests, not measured device
frame rates or a guarantee of uninterrupted audio. The audio transport must
independently keep output running during SD access, decoding, and rendering.

Validation for this part: `cargo +stable test --offline --lib` and
`cargo +stable clippy --offline --lib -- -D warnings`.
