# SpoolCut

Cut and join video without re-encoding. SpoolCut has a GTK desktop editor and a command-line tool on Linux.

## Desktop editor

Build with a current stable Rust toolchain, GTK 4.10 or newer, and libmpv from mpv 0.38 or newer. On recent Fedora releases, install the build dependencies and FFmpeg tools with:

```sh
sudo dnf install gcc cargo pkgconf-pkg-config gtk4-devel mpv-devel ffmpeg-free
cargo run --release --locked --features desktop --bin spoolcut-desktop
```

libmpv needs H.264 and HEVC decoding support for preview. The FFmpeg build used for the tests also needs the libx264 and libx265 encoders.

1. Import one or more MP4 or MOV files. Drag a source onto the timeline or select it and click **Add to timeline**.
2. Click the ruler to jump to the nearest safe keyframe or drag on it to scrub by frame. Scroll over the timeline to pan, use **−**, **+**, or Ctrl+scroll to zoom, and click **Fit** to show the whole sequence.
3. Drag clip bodies to reorder them and drag clip edges to trim. The timeline closes gaps after edits. Trimming and **Split at playhead** snap to safe video cut points.
4. Save the sequence as a JSON project, and use **Relink** if a source moves.
5. Choose **Export MKV**. The status area shows progress through validation, copying, joining, and verification.

The preview plays decoded source audio. Export keeps only complete AAC packets, so you may hear a short difference at joins with unaligned audio boundaries.

Use Space to play or pause, and **←** or **→** to seek to the keyframes marked on the clips. Select a timeline clip and use Ctrl+X or Ctrl+C to cut or copy it. Ctrl+V inserts the clip at the boundary nearest the playhead. Use Ctrl+Z to undo and Ctrl+Shift+Z to redo.

## Command-line export

Install a current stable Rust toolchain and put FFmpeg and ffprobe on your PATH. Then build and inspect each source.

```sh
cargo build --release --locked
target/release/spoolcut inspect camera-a.mp4
target/release/spoolcut inspect camera-b.mp4
```

Save a project as `timeline.json`. Relative source paths resolve from the project file's directory. Use the integer values from each file's `safe_in_pts` list for the clip boundaries. The out point is exclusive and can also be the source's `video_end_pts` if the range passes validation.

```json
{
  "sources": {
    "a": "camera-a.mp4",
    "b": "camera-b.mp4"
  },
  "clips": [
    { "source": "a", "in_pts": 0, "out_pts": 8192 },
    { "source": "b", "in_pts": 8192, "out_pts": 16384 },
    { "source": "a", "in_pts": 16384, "out_pts": 24576 }
  ]
}
```

The numbers above are examples. `inspect` reports video PTS in the source stream's own time base. `plan` checks the requested ranges and prints the effective timeline in microseconds before any media is written.

```sh
target/release/spoolcut plan timeline.json
target/release/spoolcut export timeline.json output.mkv
```

Press Ctrl+C to cancel. SpoolCut checks the staged MKV before publishing it and preserves any existing destination. The destination filesystem must support hard links for atomic publication. Allow temporary space for the copied clips and the joined file. Export reads each source again for each clip that uses it.

## Supported footage

- MP4 or MOV with one H.264 (`avc1`) or HEVC (`hvc1`) video stream and one AAC-LC audio stream. Source files in a sequence must share the same decoder configuration and stream properties. Other tracks such as subtitles and timecode are omitted.
- Video cuts are allowed at closed IDR points that partition the source's presentation timestamps cleanly. Each selected range must end at the end of its last displayed frame. Sources with stream side data such as display rotation or selected packets flagged for discard are rejected.
- SpoolCut copies complete AAC frames within the selected video range and preserves their timing relative to video. A cut between AAC frame boundaries can leave a gap of up to one AAC frame on each side of a join. Matroska packet timestamps are rounded to milliseconds.

## Tests

`cargo test --locked` generates fixtures using FFmpeg with the `libx264`, `libx265`, and `aac` encoders. It checks decoded frame order, video and audio packet hashes, output duration, A/V timing, unsafe cuts, incompatible streams, and cancellation. The fixtures cover B-frames, fractional and variable frame timing, and different source time bases.

With a graphical session, run the desktop smoke test with `cargo test --locked --features desktop --bin spoolcut-desktop -- --ignored`.
