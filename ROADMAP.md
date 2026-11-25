# SpoolCut roadmap

SpoolCut is a free, open-source desktop editor for cutting and joining video without re-encoding. Import source files into a media bin, arrange their ranges on a timeline, and export one playable file with the original video and audio streams.

## Release priorities

- Target Fedora and Wayland first. Keep Windows and macOS as later ports.
- Use Rust, GTK 4 through `gtk4-rs`, libmpv for preview, and FFmpeg and ffprobe for media inspection and export.
- Keep timeline edits, project persistence, and export planning independent of GTK and libmpv. Keep the codebase small enough for one maintainer.

## Milestone 1 — Prove stream-copy export

- [x] Model a project as a set of source files and an ordered sequence of clips. Each clip references its source and its own in and out points, so one source can appear several times.
- [x] Probe streams and index safe cut points. Use source timestamps and time bases so the timeline handles footage whose timestamps do not align with a fixed frame grid.
- [x] Start with matching camera clips and MKV output. Verify H.264 and HEVC workflows with representative footage and document the combinations the prototype supports.
- [x] Validate the requested boundaries and stream compatibility before export. Copy the selected video and audio streams, report any blocking mismatch, and make cancellation or failure leave no partial final file.
- [x] Test a generated `A → B → A` sequence with at least one trimmed range. Verify clip order, frames around each join, output duration, A/V timing, and that both streams were copied. Compare packet payloads where framing is preserved.

The [command-line prototype](README.md) verifies H.264 and HEVC with generated fixtures. Testing on physical cameras and Fedora hardware is part of Milestone 2.

## Milestone 2 — Ship a usable Fedora prototype

- [x] Add a media bin with multi-file import and one video lane. Place clips on a gapless sequence, then trim, split, reorder, or remove them.
- [x] Show the effective video cut points and snap clip boundaries to safe positions. Use the same video cut decisions for preview and export.
- [ ] Validate rapid seeks and source switching on Fedora Wayland. The editor embeds libmpv for scrubbing and playback across source changes.
- [x] Save and reopen projects with their source references and edits. Handle missing source files and support undo and redo for timeline edits.
- [x] Add export progress, cancellation, and actionable compatibility errors. Provide a runnable Fedora build or documented build steps.
- [ ] Run automated core and export tests in CI, plus a desktop smoke test on a runner with a display. Validate preview responsiveness and the exported joins on real Fedora hardware with representative camera footage.

The prototype is ready when a user can assemble, save, reopen, preview, and export the same multi-source sequence on Fedora. The exported joins must match the effective cuts shown in the editor, with audio in sync and both original streams copied.

## After the Fedora prototype

- Expand the supported source combinations using media fixtures and regression tests.
