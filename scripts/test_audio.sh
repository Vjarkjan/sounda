#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
mkdir -p output
"${HOST_CC:-cc}" -std=c11 -O1 -g -Wall -Wextra -Wconversion -Werror \
    -fsanitize=address,undefined tests/audio_transport.c -o output/audio-transport-tests
# The transport uses static buffers. LeakSanitizer cannot run under the
# workspace's ptrace wrapper; address and undefined-behavior checks stay on.
ASAN_OPTIONS=detect_leaks=0 ./output/audio-transport-tests
