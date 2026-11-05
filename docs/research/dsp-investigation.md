# DSi Sound firmware and playback investigation

This investigation used the supplied Nintendo DSi Sound executable, its unchanged
`aac.a`, REA 6.5.0 with Ghidra 12.1.4, Teakra, and a full DSi emulator. The findings
below distinguish static observations, emulator behavior, and hardware validation.
No Nintendo ARM program code or decompiler output was added to the player.

## Artifact identity and scope

| Input | Bytes | SHA-256 |
|---|---:|---|
| `00000002.app` (`DSSOUND`, `HNKE`) | 4,526,080 | `9cafdb10f984aed6330ba99e54bb37828d9a92de4d292a6507f45c798416f5c5` |
| `aac.a` | 694,904 | `6770fe97550eb70f27a0f11612cfba31bb800ae9b4d5b87745c3c673751951b8` |

ndspy 4.2.0 read the executable's NitroFS and confirmed that its `aac.a` is
byte-for-byte identical to the supplied root file. The COFF contains 276 section
records, including duplicate block headers, and **107 actual memory placements**.
The ARM9 starts at file offset `0x4000`, load address `0x02004000`, entry
`0x02004800`; its first `0x800` bytes are the secure area.

The stored ARM9i bytes are encrypted. The stock application was therefore loaded
through melonDS's DSi direct boot and its decrypted main RAM and ITCM were captured.
That run reached an ARM9 data abort in its cartridge/DSiWare environment; it supplies
code bytes for static analysis, not evidence that stock Sound playback worked.
REA analyzed the extracted ARM9, the decrypted RAM range, and the ITCM image.
The raw ARM import uses Ghidra's generic little-endian ARM language; inferred
function names/types and decompiler pseudocode are not original source.

Local machine-readable identities, all COFF placements, and tool versions are in
[`artifact-manifest.json`](../../output/research/artifact-manifest.json) and
[`tool-installations.json`](../../output/research/tool-installations.json).
Nintendo-derived images and full decompilation evidence remain under `output/`.

## Two concrete loader/reset defects

**The COFF loader omitted the program-memory copy of two dual-placement sections.**
`.data@c0@d0` has flags `0x18`, program word address `0x2CD3`, data word address
`0x9014`, and size `0x1DE`. `const_data@c0@d0` similarly maps to program `0x2CBF`
and data `0x9000`, size `0x28`. Matching only the end of the section name selected
`@d0` and omitted `@c0`. Both markers must be recognized independently.

The firmware's symbol-bearing Teak disassembly shows `_initDataSections` at
DSP word `0x0E7E` copying those program constants into data memory. Without the
program copy, this destroys the heap configuration. The baseline emulator then
reached `_exit` and its `TRAP` at word `0x0FF3`, with the saved return address
`0x0EBF` identifying the file-I/O initialization allocation failure. The correct
loader places both copies and boots the firmware. Stock ARM9 loader function
`0x020F0C20` also independently tests all four `@c0/@c1/@d0/@d1` markers.

**The restart check confused DSP power state with permission.** The installed
BlocksDS `dspPowerOff()` clears `SCFG_EXT_DSP` (bit 18) and the extended interrupt
bit. Requiring bit 18 already set before the next `dspPowerOn()` rejects a DSP
that this application just powered off. NWRAM availability and successful mapping
remain the relevant permission checks; DSP power must be enabled during boot.
This defect directly explains a second song failing with a misleading launch-mode
error. `nwramIsAvailable()` itself checks `SCFG_EXT_MBK_RAM`, not the current B/C
address ranges.

## Observed stock decode and buffering protocol

Stock function `0x02010138` sends these eleven 16-bit words through **CMD1**:

| Word | Value |
|---:|---|
| 0 | `1` (decode one frame) |
| 1 | Compressed frame length in bytes |
| 2–3 | Sample rate, high word then low word |
| 4 | Channel configuration (`1` or `2`) |
| 5–6 | ARM9 compressed-buffer address, high then low |
| 7–8 | ARM9 left PCM address, high then low |
| 9–10 | ARM9 right PCM address, high then low |

Each command produces 1024 signed 16-bit samples per channel, with separate
`0x800`-byte output planes. The firmware reports on REPLY0: `0` success,
`1` invalid parameters, `2` decode failure. Boot synchronizes on `1` from all
three reply registers; the common pipe monitor then reports its address on
REPLY2 and signals semaphore 15. AAC decoding uses CMD1 directly; it does not
require sending the compressed data through a pipe.

The Teak `_main` at word `0x0195` shows that command `0` clears the cached previous
frequency. The next frame then runs `CEVA_HE_AAC_DECODER_INIT` even if its rate is
unchanged. Command zero is therefore a useful stream-reset operation. It is not
a promise that an in-flight command can safely be overwritten. Stock waiter
`0x02010218` cooperatively yields and uses a two-second completion deadline; its
timeout path sends command zero and disables its completion interrupt.

Stock ITCM function `0x01FF86B8` rotates **six slots of 1024 samples per channel**.
It cleans the compressed input cache range, invalidates the output cache ranges,
submits a frame, waits for completion, and reads the next compressed frame.
Stock start function `0x020102E8` clears `0x1000` input bytes and two `0x3000` PCM
planes, and primes six decoder slots before output. ITCM `0x01FF8804` interleaves
the planar ring into an output buffer and wakes its producer after consumption.
This confirms that bounded buffering and producer/consumer service matter; a UI
redraw does not intrinsically disconnect the DSP.

## Output quality boundary

The observed stock path decodes to ARM9 PCM buffers and uses a buffered output
callback. Its player volume code `0x02091454` iterates sound channel IDs; SDK
`0x020A3304` enqueues command `0x14`, and flush `0x020A3838` submits the command
queue on PXI tag 7. This supports an ARM7/Nitro sound transport path. The common
`DSPAudio_*`/BTDMP routines also exist in `aac.a`; their presence alone does not
establish that the music player routes decoded AAC directly through those ports.

Sounda uses the supplied decoder and buffered PCM output, but the complete stock
resampling, mixer, codec configuration, and physical analog response have not been
reconstructed or measured. The work establishes stable decode/control behavior,
not a measured increase in fidelity or bit-identical stock output. Physical DSi
listening remains a separate check from emulator PCM continuity.

## REA evidence ledger

Full inline records and their profile/target digests are retained under
[`output/research/rea`](../../output/research/rea/). This table lists the decisive
records; [`evidence-index.json`](../../output/research/rea/evidence-index.json)
indexes all successful queries. Addresses are observed; descriptive names in
this report are analyst labels.

| Observation | REA Evidence ID |
|---|---|
| Stock `/aac.a` path and AAC class/error strings | `ev_099858c1ca1b515e4060ed2684203a2945e3965fbe580ad69da2623b6318a805` |
| Stock loader/boot and three reply synchronization | `ev_6bd3aa227de1f5d1c09da45f26763b103bff7a6d07a4b051f06b75a6485b8859` |
| Independent code/data section-name marker tests | `ev_b1984b84810e85cf12f1c7ffab00cdac2cdd2f9d3e31075f1b8ea98a8c08c1dd` |
| Eleven-word AAC frame submission | `ev_9544abafa115bfcb489733875ce12ad8268e92193a293c3f029b6f45bc1117f4` |
| Yielding wait, deadline, command-zero recovery | `ev_59556434f3a7ca38e55f506fd8471cf5192ec4cc38fb008383566d6ccbc1b1b4` |
| Six-frame priming and plane sizes | `ev_680f05280875f4c5168677ddb4d14fa5bf2e7053ab01768c0f9013157f87a69a` |
| ITCM ring rotation and cache handoff | `ev_7d4258b7bdf74b03cf133590566339e0a8946fdb958770e5bc1e7aa29e4c5dcf` |
| Planar-to-interleaved PCM consumer | `ev_13edbc75e71343b3d5905bd2700bdc68200034c900a2f1c79d5da436d0ac89e0` |
| Sound channel volume command | `ev_60a01d26e4778df683a565bf55f6834b77011868a5be24c3f82b82e9eeb96f27` |
| Sound command queue flush/PXI tag 7 | `ev_eccc78fe246c7ae8691e3b2883d9c2a4a9c5649af4a1cc2b153a9218cc786e65` |

REA exported the ARM9, ITCM, and SDK evidence bundles and closed each native
session. The investigation used a separate real MCP session because the already
running chat server could not acquire newly installed engine environment values.

## Installed tools and reproducibility

Installed under `~/.local/share/sounda-tools`: official Ghidra **12.1.4**, Temurin
JDK **25.0.4.1+1**, ndspy **4.2.0**, Capstone **5.0.9**, and Teakra commit
`3d697a18df504f4677b65129d9ab14c7c597e3eb`. Teakra libraries are in
`~/.local/lib`; `teakra-coff_reader`, `teakra-dsp1_reader`, `teakra-makedsp1`, and
`teakra-test_verifier` are on PATH. BlocksDS `ndstool` was already installed.
Official release archive hashes and the small DSi DMA patch provenance are in
the tool manifest. The upstream Teakra suite passed **572 assertions in 7 cases**.

The Codex REA entry now has `GHIDRA_INSTALL_DIR`, `JAVA_HOME`, a 300-second engine
startup budget, and a 360-second tool budget. Its MCP server startup remains
30 seconds. Other parsed configuration was verified unchanged. The exact backup
is `~/.codex/config.toml.sounda-ghidra-20261015.backup`; identities and scoped
doctor success are in `output/research/rea/provider-config-verification.json`
and `configured-doctor.json`. Reload Codex to use this provider in the live chat.

```sh
python3 scripts/research/extract_sound.py
cp aac.a output/research/aac-local.a
~/.local/bin/teakra-coff_reader output/research/aac-local.a > output/research/aac-symbols.txt
```

The COFF reader writes its disassembly beside its input. First copy the supplied
firmware to that analysis-only path; do not point it at a release directory.
`scripts/research/make_analysis_elf.py` wraps an explicitly mapped ARM range for
Ghidra without modifying its instruction bytes. `rea_session.mjs` accepts one
JSON MCP request per line and saves the complete returned Evidence. For example:

```sh
export REA_PACKAGE_ROOT="$HOME/.npm/_npx/ad9a3fdbcad992c6/node_modules/rea-agents"
export GHIDRA_INSTALL_DIR="$HOME/.local/share/sounda-tools/ghidra_12.1.4_PUBLIC"
export JAVA_HOME="$HOME/.local/share/sounda-tools/jdk-25.0.4.1+1"
export REA_GHIDRA_STARTUP_TIMEOUT_MS=300000
node scripts/research/rea_session.mjs
```

Use `tool-schemas.json` for the canonical tool inputs. Example request:
`{"tool":"procedure_pseudo_code","arguments":{"procedure":"0x02010138"},"save":"aac-submit"}`
after opening the matching runtime image. A native image's base address and
entry must match its recorded mapping.

The standalone upstream Teakra probe exposed its missing DSi DMA-enable-edge
behavior. The documented local patch makes input DMA run, but that upstream
revision still encounters unimplemented opcode `0xD780` at DSP word `0x5095`.
Those initial runs are diagnostic failures, not successful decode tests.

The **working standalone DSP emulator** instead links melonDS 1.1's more complete
vendored Teakra, with a byte-addressed DSP-memory callback and a logging shim. It
does not link a host AAC decoder. It is installed as `~/.local/bin/sounda-dsp-probe`:

```sh
sounda-dsp-probe aac.a output/research/stereo-44100.aac
```

The input must be ADTS AAC; the probe strips the ADTS transport header before
sending each raw frame to the unchanged firmware. It writes interleaved signed
16-bit little-endian PCM beside the input, with suffix `.dsp.s16le`. Multiple
ADTS paths exercise command-zero resets between streams without reloading the
firmware. For an M4A source, create an analysis copy with
`ffmpeg -i song.m4a -c:a copy -f adts song.aac`. Rebuild using
`sh scripts/research/build_dsp_probe.sh` after the emulator libraries are built.

This probe decoded **251 frames across five streams with zero errors**: 44.1 kHz
stereo twice, 8 kHz mono, 22.05 kHz mono, and 48 kHz stereo. Boot synchronized in
3157 emulated DSP cycles. An independent FFmpeg decode of the same four ADTS
fixtures produced equal PCM lengths and direct waveform correlations above
0.99999987, with RMS differences of 0.70–1.47 signed-16 sample units. This is
decoder agreement on synthetic fixtures, not a measurement of the DSi DAC or
physical sound quality. Reproduce the comparison with
`scripts/research/compare_dsp_pcm.py`; full measurements and byte identities are
in `output/research/teakra-pcm-comparison.json`, and the five-stream trace is
`output/research/teakra-dsi-multitrack.log`.

The separate full DSi emulator validation executes Sounda's actual ARM9/ARM7
program, SD access, output queue, and original firmware; see its verification
report for playback, UI navigation, track changes, and pause/resume results.

## Primary references

- [Stock protocol investigation by the melonDS developer](https://melonds.kuribo64.net/board/thread.php?pid=7825)
- [GBATEK DSP registers and memory](https://problemkaputt.de/gbatek-dsi-xpertteak-dsp.htm)
- [Teakra APBP communication semantics](https://github.com/wwylele/teakra/blob/master/src/apbp.md)
- [Teakra external-memory/AHBM interface](https://github.com/wwylele/teakra/blob/master/src/ahbm.md)
- [Reference linked-COFF loader](https://github.com/Gericom/DSiDsp/blob/master/arm9/source/libdsp/DspProcess.cpp)
- [BlocksDS DSP power/reset implementation](https://github.com/blocksds/libnds/blob/master/source/arm9/teak/dsp.twl.c)
- [melonDS DSi DSP integration](https://github.com/melonDS-emu/melonDS/blob/1.1/src/DSi_DSP.cpp)
- [Official Ghidra release](https://github.com/NationalSecurityAgency/ghidra/releases/tag/Ghidra_12.1.4_build)
