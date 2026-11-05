#!/bin/sh
# Reuse the DSi-capable Teakra libraries built by scripts/emulator.
set -eu
SOUNDA_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
SOUNDA_DSP_SOURCE=${SOUNDA_MELONDS_SOURCE:-"$HOME/.local/src/sounda-melonds-1.1"}
SOUNDA_DSP_BUILD=${SOUNDA_MELONDS_CORE_BUILD:-"$SOUNDA_ROOT/output/emulator/build/melonDS/src"}
mkdir -p "$SOUNDA_ROOT/output/research"
exec "${CXX:-c++}" -O2 -std=c++17 -DSOUNDA_VENDOR_TEAKRA \
    -I "$SOUNDA_DSP_SOURCE/src/teakra/include" \
    -I "$SOUNDA_DSP_BUILD" -I "$SOUNDA_DSP_SOURCE/src" \
    "$SOUNDA_ROOT/scripts/research/teakra_aac.cpp" \
    "$SOUNDA_DSP_BUILD/teakra/src/libteakra.a" "$SOUNDA_DSP_BUILD/libcore.a" \
    -pthread -lzstd -o "$SOUNDA_ROOT/output/research/teakra-aac-dsi"
