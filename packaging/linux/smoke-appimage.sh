#!/usr/bin/env bash
set -euo pipefail
image=$(realpath "$1")
version=$(cat VERSION)
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT
cd "$workspace"
mkdir 'SpoolCut ü relocated'
cp "$image" 'SpoolCut ü relocated/SpoolCut.AppImage'
image="$workspace/SpoolCut ü relocated/SpoolCut.AppImage"
"$image" --appimage-extract >/dev/null
bundle="$workspace/SpoolCut ü payload"
mv "$workspace/squashfs-root" "$bundle"
export XDG_DATA_HOME="$workspace/desktop-data"
export XDG_CURRENT_DESKTOP=KDE
test -f "$bundle/plugins/platformthemes/libqxdgdesktopportal.so"
unset QT_QPA_PLATFORMTHEME
env PATH=/usr/bin:/bin "$bundle/AppRun" --cli --version | grep -Fx "SpoolCut $version"
for video in avc hevc; do
    encoding=(-c:v libx264)
    if [[ $video == hevc ]]; then
        encoding=(-c:v libx265 -tag:v hvc1 -pix_fmt yuv420p10le -x265-params pools=1:frame-threads=1:log-level=error:open-gop=0)
    fi
    env PATH=/usr/bin:/bin LD_LIBRARY_PATH="$bundle/lib" "$bundle/bin/ffmpeg" -nostdin -v error \
        -f lavfi -i testsrc2=size=320x180:rate=25:duration=2 \
        -f lavfi -i sine=sample_rate=48000:duration=2 "${encoding[@]}" -c:a aac -video_track_timescale 12800 "$video ü video.mp4"
    source="$workspace/$video ü video.mp4"
    project="$workspace/$video ü project.spoolcut.json"
    exported="$workspace/$video ü export.mkv"
    printf '{"sources":{"A":"%s"},"clips":[{"source":"A","in_pts":0,"out_pts":25600}]}\n' "${source##*/}" > "$project"
    env PATH=/usr/bin:/bin "$bundle/AppRun" --cli inspect "$source"
    env PATH=/usr/bin:/bin "$bundle/AppRun" --cli plan "$project"
    env PATH=/usr/bin:/bin "$bundle/AppRun" --cli export "$project" "$exported"
    env PATH=/usr/bin:/bin LD_LIBRARY_PATH="$bundle/lib" "$bundle/bin/ffmpeg" \
        -nostdin -v error -xerror -i "$exported" -f null -
    for mode in software auto; do
        env QT_DEBUG_PLUGINS=1 PATH=/usr/bin:/bin SPOOLCUT_QT_PREVIEW="$mode" SPOOLCUT_QT_TEST_AUDIO_NULL=1 \
            APPIMAGE_EXTRACT_AND_RUN=1 "$image" --smoke "$workspace/$video ü video.mp4" > "$workspace/preview.log" 2>&1 || {
                cat "$workspace/preview.log"
                exit 1
            }
        grep -F 'platformthemes/libqxdgdesktopportal.so" loaded library' "$workspace/preview.log"
        grep -E '^(Software|OpenGL)' "$workspace/preview.log"
    done
done
launcher="$XDG_DATA_HOME/applications/io.github.bugrakoc.SpoolCut.desktop"
desktop-file-validate "$launcher"
grep -Fx "$(printf 'Exec="%s" %%F' "$image")" "$launcher"
test -s "$XDG_DATA_HOME/icons/hicolor/256x256/apps/io.github.bugrakoc.SpoolCut.png"
