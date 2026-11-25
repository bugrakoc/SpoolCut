#!/usr/bin/env bash
# Static AppImage runtime dependencies from its pinned upstream build recipe.
set -euo pipefail
cd "$1"
curl -fL --retry 3 https://github.com/libfuse/libfuse/releases/download/fuse-3.15.0/fuse-3.15.0.tar.xz -o fuse-3.15.0.tar.xz
curl -fL --retry 3 https://github.com/vasi/squashfuse/archive/0.5.2.tar.gz -o squashfuse-0.5.2.tar.gz
sha256sum -c <<'HASHES'
70589cfd5e1cff7ccd6ac91c86c01be340b227285c5e200baa284e401eea2ca0  fuse-3.15.0.tar.xz
db0238c5981dabbd80ee09ae15387f390091668ca060a7bc38047912491443d3  squashfuse-0.5.2.tar.gz
HASHES

notices=$2
mkdir -p "$notices"
tar -xOf fuse-3.15.0.tar.xz fuse-3.15.0/LICENSE > "$notices/libfuse-LICENSE"
tar -xOf fuse-3.15.0.tar.xz fuse-3.15.0/LGPL2.txt > "$notices/libfuse-LGPL2.txt"
tar -xOf squashfuse-0.5.2.tar.gz squashfuse-0.5.2/LICENSE > "$notices/squashfuse-LICENSE"
curl -fsSL --retry 3 https://raw.githubusercontent.com/ifduyue/musl/v1.2.5/COPYRIGHT -o "$notices/musl-COPYRIGHT"
curl -fsSL --retry 3 https://raw.githubusercontent.com/microsoft/mimalloc/v2.1.7/LICENSE -o "$notices/mimalloc-LICENSE"
curl -fsSL --retry 3 https://raw.githubusercontent.com/madler/zlib/v1.3.2/LICENSE -o "$notices/zlib-LICENSE"
curl -fsSL --retry 3 https://raw.githubusercontent.com/facebook/zstd/v1.5.6/LICENSE -o "$notices/zstd-LICENSE"
cp -r "$notices" .
