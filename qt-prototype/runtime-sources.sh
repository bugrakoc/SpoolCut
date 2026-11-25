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
