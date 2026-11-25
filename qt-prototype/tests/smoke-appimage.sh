#!/usr/bin/env bash
set -euo pipefail
image=$(realpath "$1")
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT
cd "$workspace"
mkdir 'SpoolCut ü relocated'
cp "$image" 'SpoolCut ü relocated/SpoolCut Qt.AppImage'
image="$workspace/SpoolCut ü relocated/SpoolCut Qt.AppImage"
"$image" --appimage-extract >/dev/null
bundle="$workspace/squashfs-root"
for video in avc hevc; do
    encoding=(-c:v libx264)
    if [[ $video == hevc ]]; then
        encoding=(-c:v libx265 -tag:v hvc1 -pix_fmt yuv420p10le -x265-params pools=1:frame-threads=1:log-level=error)
    fi
    env LD_LIBRARY_PATH="$bundle/lib" "$bundle/bin/ffmpeg" -nostdin -v error \
        -f lavfi -i testsrc2=size=320x180:rate=25:duration=2 \
        -f lavfi -i sine=sample_rate=48000:duration=2 "${encoding[@]}" -c:a aac "$video ü video.mp4"
    for mode in software auto; do
        env PATH=/usr/bin:/bin SPOOLCUT_QT_PREVIEW="$mode" SPOOLCUT_QT_TEST_AUDIO_NULL=1 \
            APPIMAGE_EXTRACT_AND_RUN=1 "$image" --smoke "$workspace/$video ü video.mp4"
    done
done
