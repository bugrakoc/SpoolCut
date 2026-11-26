# SpoolCut 0.4.0-rc.1

This candidate prepares SpoolCut for public testing on Linux and Windows.

- **Help → System Information** shows the active renderer, operating
  system, hardware where available and the versions of Qt, libmpv,
  FFmpeg and FFprobe. Its Markdown-compatible report can be copied
  manually. SpoolCut has no automatic telemetry.
- The Help menu links to documentation, bug reporting and compatibility
  reporting. The [testing guide](TESTING.md) includes a five-minute test
  and a template for voluntary reports.
- Fixed a preview stall when a trim, split or rearrangement replaced a
  clip while libmpv was still opening the previous preview.
- Candidate packages target Fedora 44 x86-64 and Windows 11 x86-64.
  OpenGL remains the Linux default with software fallback. Windows
  defaults to software preview and offers OpenGL as an opt-in.

SpoolCut copies compatible compressed H.264 or HEVC and AAC-LC streams
from MP4 or MOV to Matroska. Cuts are aligned to safe keyframes. Extra
tracks are omitted, and other formats and mixed recording modes may be
rejected. Physical Windows OpenGL, mixed-DPI displays and additional
Linux distributions need more testing. See [compatibility](COMPATIBILITY.md).
