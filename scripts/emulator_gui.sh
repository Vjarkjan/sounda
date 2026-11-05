#!/usr/bin/env bash
# Isolated GUI for the locally built, GDB-enabled official melonDS 1.1.
set -euo pipefail
sounda_root="$(cd -- "$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")/.." && pwd)"
sounda_gui="$HOME/.local/share/sounda-emulator/gui"
sounda_bios="${SOUNDA_BIOS_DIR:-$HOME/Downloads/DS firmware + DSi NAND and firmware}"
sounda_binary="$HOME/.local/libexec/sounda-melonds/melonDS"
mkdir -p "$sounda_gui/config/melonDS" "$sounda_gui/data"
if [[ ! -f "$sounda_gui/nand-copy.bin" ]]; then
    cp --reflink=auto -- "$sounda_bios/dsinand.bin" "$sounda_gui/nand-copy.bin"
fi
python - "$sounda_gui" "$sounda_bios" "$sounda_root" <<'PY'
from pathlib import Path
import json
import sys
gui, bios, root = map(Path, sys.argv[1:])
config = gui / 'config/melonDS/melonDS.toml'
if not config.exists():
    def quoted(p): return json.dumps(str(p))
    config.write_text(f'''[Emu]
ConsoleType = 1
DirectBoot = true
ExternalBIOSEnable = true
[DS]
BIOS9Path = {quoted(bios / 'biosnds9.bin')}
BIOS7Path = {quoted(bios / 'biosnds7.bin')}
FirmwarePath = {quoted(bios / 'dsfirmware.bin')}
[DSi]
BIOS9Path = {quoted(bios / 'biosdsi9.bin')}
BIOS7Path = {quoted(bios / 'biosdsi7.bin')}
FirmwarePath = {quoted(bios / 'dsifirmware.bin')}
NANDPath = {quoted(gui / 'nand-copy.bin')}
[DSi.DSP]
HLE = false
[DSi.SD]
Enable = true
ReadOnly = false
FolderSync = true
FolderPath = {quoted(root / 'output/emulator/sd')}
ImagePath = {quoted(gui / 'sd.bin')}
ImageSize = 1
[Instance0.Gdb]
Enabled = true
[Instance0.Gdb.ARM9]
Port = 4333
BreakOnStartup = false
[Instance0.Gdb.ARM7]
Port = 4334
BreakOnStartup = false
[Screen]
UseGL = false
[3D]
Renderer = 0
[JIT]
Enable = false
''')
PY
export XDG_CONFIG_HOME="$sounda_gui/config"
export XDG_DATA_HOME="$sounda_gui/data"
if [[ $# == 0 ]]; then set -- "$sounda_root/output/sounda.nds"; fi
exec "$sounda_binary" "$@"
