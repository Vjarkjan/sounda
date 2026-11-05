#!/usr/bin/env bash
set -euo pipefail
sounda_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$sounda_root"
export BLOCKSDS="${BLOCKSDS:-/opt/wonderful/thirdparty/blocksds/core}"
export WONDERFUL_TOOLCHAIN="${WONDERFUL_TOOLCHAIN:-/opt/wonderful}"
export PATH="$WONDERFUL_TOOLCHAIN/toolchain/gcc-arm-none-eabi/bin:$PATH"
action="${1:-build}"
case "$action" in build|clippy) ;; *) echo 'Usage: bash scripts/build.sh [build|clippy]' >&2; exit 2;; esac
test -s aac.a || { echo 'Place your DSi Sound aac.a in the project root.' >&2; exit 1; }
command -v arm-none-eabi-gcc >/dev/null
extra=()
if [[ "$action" == clippy ]]; then extra=(-- -D warnings); fi
cargo "+${NDS_TOOLCHAIN:-nightly}" "$action" --locked --offline --release \
    --config .cargo/nds.toml --target "$sounda_root/armv5te-nintendo-ds.json" \
    --features nds --bin sounda "${extra[@]}"
if [[ "$action" == clippy ]]; then exit 0; fi
elf="${CARGO_TARGET_DIR:-target}/armv5te-nintendo-ds/release/sounda.elf"
mkdir -p output dist/sounda/docs
arm-none-eabi-gcc -std=gnu11 -O2 -Wall -Wextra -Werror \
    -mthumb -mcpu=arm7tdmi -ffunction-sections -fdata-sections \
    -specs="$BLOCKSDS/sys/crts/ds_arm7.specs" \
    -isystem "$BLOCKSDS/libs/libnds/include" -isystem "$BLOCKSDS/libs/maxmod/include" \
    native/arm7.c -o output/arm7.elf \
    -L"$BLOCKSDS/libs/libnds/lib" -L"$BLOCKSDS/libs/maxmod/lib" \
    -Wl,--gc-sections -Wl,--start-group -lmm7 -lnds7 -lc -lgcc -Wl,--end-group
python3 scripts/make_icon.py
"$BLOCKSDS/tools/ndstool/ndstool" -c output/sounda.nds \
    -9 "$elf" -7 output/arm7.elf \
    -b output/icon.bmp 'Sounda;SD music player;Rust + DSi DSP' \
    -g SNDA 00 SOUNDA -uc 3 -a 00000138
cp output/sounda.nds dist/sounda/sounda.nds
cp README.md dist/sounda/README.md
cp -R docs/. dist/sounda/docs/
python3 - <<'PY'
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile
with ZipFile('dist/sounda.zip', 'w', compression=ZIP_DEFLATED) as archive:
    for path in sorted(Path('dist/sounda').rglob('*')):
        if path.is_file():
            archive.write(path, path.relative_to('dist'))
PY
arm-none-eabi-size "$elf"
echo 'ROM: output/sounda.nds'
echo 'SD card package: dist/sounda/'
echo 'SD card archive: dist/sounda.zip'
