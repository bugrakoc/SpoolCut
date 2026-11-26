#!/usr/bin/env bash
# Static AppImage runtime dependencies from its pinned upstream build recipe.
set -euo pipefail
cd "$1"
fetch() {
    curl -fsSL --retry 3 --max-time 90 "$1" -o "$2"
}
fetch https://github.com/libfuse/libfuse/releases/download/fuse-3.15.0/fuse-3.15.0.tar.xz fuse-3.15.0.tar.xz
fetch https://github.com/vasi/squashfuse/archive/0.5.2.tar.gz squashfuse-0.5.2.tar.gz
sha256sum -c <<'HASHES'
70589cfd5e1cff7ccd6ac91c86c01be340b227285c5e200baa284e401eea2ca0  fuse-3.15.0.tar.xz
db0238c5981dabbd80ee09ae15387f390091668ca060a7bc38047912491443d3  squashfuse-0.5.2.tar.gz
HASHES

notices=$2
mkdir -p "$notices"
cat > Alpine-sources.tsv <<'SOURCES'
Package	Alpine version	Recipe revision
musl	1.2.5-r11	8999cf56154b9bd09c995f293d8dfd3aa8001e3a
zlib	1.3.2-r1	c973d294927355eac1d2a6e97d86e41fbab8ffda
zstd	1.5.6-r2	5c2ddf18f193dafdd25ae7391d96c7e30766eecd
mimalloc2	2.1.7-r0	5a1a5dc3216a69e3cd66a57ee07cb5ef01b8be39
SOURCES
# The static versions are recorded in the pinned runtime's x86-64 build:
# https://github.com/AppImage/type2-runtime/actions/runs/39320578745/job/118293253809
collect_alpine_source() (
    package=${1##*/}
    recipe="https://raw.githubusercontent.com/alpinelinux/aports/$2/$1"
    mkdir "$package"
    cd "$package"
    fetch "$recipe/APKBUILD" APKBUILD
    grep -Fx "pkgver=${3%-r*}" APKBUILD
    grep -Fx "pkgrel=${3##*-r}" APKBUILD
    awk '/^sha512sums="/ {sums=1; next} sums && /^"/ {exit} sums {print}' APKBUILD > SHA512SUMS
    while read -r digest filename; do
        case "$filename" in
            *.tar.gz) fetch "$4" "$filename" ;;
            *) fetch "$recipe/$filename" "$filename" ;;
        esac
    done < SHA512SUMS
    sha512sum -c SHA512SUMS
)
collect_alpine_source main/musl 8999cf56154b9bd09c995f293d8dfd3aa8001e3a 1.2.5-r11 \
    https://distfiles.alpinelinux.org/distfiles/v3.21/musl-1.2.5.tar.gz
collect_alpine_source main/zlib c973d294927355eac1d2a6e97d86e41fbab8ffda 1.3.2-r1 \
    https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz
collect_alpine_source main/zstd 5c2ddf18f193dafdd25ae7391d96c7e30766eecd 1.5.6-r2 \
    https://codeload.github.com/facebook/zstd/tar.gz/refs/tags/v1.5.6
collect_alpine_source community/mimalloc2 5a1a5dc3216a69e3cd66a57ee07cb5ef01b8be39 2.1.7-r0 \
    https://codeload.github.com/microsoft/mimalloc/tar.gz/refs/tags/v2.1.7

tar -xOf fuse-3.15.0.tar.xz fuse-3.15.0/LICENSE > "$notices/libfuse-LICENSE"
tar -xOf fuse-3.15.0.tar.xz fuse-3.15.0/LGPL2.txt > "$notices/libfuse-LGPL2.txt"
tar -xOf squashfuse-0.5.2.tar.gz squashfuse-0.5.2/LICENSE > "$notices/squashfuse-LICENSE"
tar -xOf musl/musl-1.2.5.tar.gz musl-1.2.5/COPYRIGHT > "$notices/musl-COPYRIGHT"
tar -xOf mimalloc2/mimalloc-2.1.7.tar.gz mimalloc-2.1.7/LICENSE > "$notices/mimalloc-LICENSE"
tar -xOf zlib/zlib-1.3.2.tar.gz zlib-1.3.2/LICENSE > "$notices/zlib-LICENSE"
tar -xOf zstd/zstd-1.5.6.tar.gz zstd-1.5.6/LICENSE > "$notices/zstd-LICENSE"
cp Alpine-sources.tsv "$notices/"
cp -r "$notices" .
