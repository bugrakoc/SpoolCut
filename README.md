# SpoolCut

Cut and join video without re-encoding. The first prototype is a Linux command-line tool that exports an ordered sequence to a single MKV file.

## Build and export

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
