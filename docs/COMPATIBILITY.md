# Compatibility matrix

These entries describe observed results and their test scope. Use
the [five-minute guide](TESTING.md) to report a new configuration.

## 0.4.x

### Maintainer-tested

No maintainer report for 0.4.x is recorded yet. The automated checks below
use generated footage and are listed separately from hardware reports.

### Automated verification

| SpoolCut | OS and session | GPU and renderer | Package | Test scope and result |
| --- | --- | --- | --- | --- |
| 0.4.0-rc.1 | Pending verification | | | |

### Community-reported

No community reports have been recorded. Mark any future entry **Partial**
if it covers only some workflows. Mark a verified incompatibility **Known
problem**, cite the bug, and say which workflow and version it affects.

## Historical evidence

| SpoolCut | Tester and scope | OS and session | GPU and preview | Package | Observed result |
| --- | --- | --- | --- | --- | --- |
| Qt 0.2.2 | Maintainer, partial | Fedora KDE Plasma / Wayland. Fedora version unspecified in the report. | NVIDIA GeForce RTX 4070 Ti. OpenGL and forced software. | Unspecified | Editing, scrubbing, joins and the original duration-regression export passed. Missing-media detection worked. The preview message and relink behavior prompted follow-up fixes. [Report](CPP_QT_STATUS.md#maintainer-acceptance-of-qt-022) |
| Qt 0.2.2 | Maintainer, partial | Windows 11 / QEMU | VM display adapter model unspecified. Software. | Unspecified | Main 10 HEVC preview and export, join playback, relink, and saving/reopening passed. Export to the tested VirtIO shared folder passed. [Report](CPP_QT_STATUS.md#maintainer-acceptance-of-qt-022) |
| 0.3.0 | Maintainer acceptance and automated checks | Fedora 44 / Weston Wayland and Windows hosted runner | Hosted Mesa software OpenGL on Fedora. Windows software default. | Fedora AppImage and Windows portable ZIP | Both relocated packages passed the production smoke checks. The maintainer approved the candidate. [Release evidence](CPP_QT_STATUS.md#030-release) |

## Priority configurations still needed for 0.4.x

- AMD Radeon, Intel integrated graphics, older NVIDIA GPUs and hybrid-GPU laptops.
- Physical Windows 11 systems, including software preview and optional OpenGL.
- Ubuntu, Linux Mint, Debian and Arch-based Linux on physical systems, plus
  GNOME Wayland and X11 on a range of GPUs.
- Lower-end hardware, high DPI and mixed-DPI monitors.
- Real recordings across supported codecs and containers, long recordings,
  high resolutions, variable frame rates and different recording devices.
- Other destination filesystems, including exFAT and removable media.

macOS is outside this campaign.

## Recording a result

Include the exact SpoolCut version, OS and session, GPU, actual preview
renderer, package method, what was tested and a link to the original report
or CI run. Use **Unknown** for missing details. Retain older results when a
newer build changes the outcome.
