# Maintainer regression checklist for 0.4.0-rc.1

Run this on the **Fedora KDE Wayland** desktop and the **Windows 11**
VM with the packaged AppImage and extracted portable ZIP. Use the same
candidate revision on both. A physical Windows 11 test is also welcome.
For each platform, record the package, renderer, version and result. See
the [five-minute checklist](TESTING.md) for step-by-step instructions.

## Package and diagnostics

- [ ] Check the package SHA-256 against the accompanying checksum. Move
  the AppImage or extract the ZIP to a fresh path with spaces. Launch without
  the development toolchain on PATH. Check the launcher or taskbar icon,
  window controls, native file picker and drag and drop.
- [ ] Open **Help → System Information** before loading media. Confirm the
  app version and build revision match the candidate. Qt, libmpv, FFmpeg and
  FFprobe should have versions. Missing hardware, display or renderer details
  should read **Unknown or unavailable**. Check the privacy notice.
- [ ] Play a clip and reopen the dialog. On Fedora, confirm OpenGL is active
  on the NVIDIA setup and the OpenGL renderer and version agree with the
  context. On Windows, confirm the default is software and the OpenGL
  fields are unavailable. Check that a hardware GPU can still be listed
  with software preview. The VM can identify itself as virtualized.
- [ ] Compare the visible report with clipboard text after selecting **Copy
  Information**. Confirm clipboard contents only change when copied.
  Inspect for personal filenames, paths, user and host names, hardware
  identifiers, addresses and raw tool output. Close the dialog immediately
  after opening it once, reopen it and ensure it still collects normally.
- [ ] Follow the Help links. Check the testing guide on the candidate
  revision and confirm the issue form appears after merging. Verify the
  reporting instructions lead to the pinned discussion after enabling it. Confirm About shows the same version.
- [ ] On Fedora, repeat with `SPOOLCUT_QT_PREVIEW=software` and check for a
  stable picture with no white fade. On Windows, try OpenGL only where the
  graphics driver can provide it or record the opt-in test as skipped.
  Check the dialog at typical display scaling and at 150% or 200% as
  available. Test a mixed-DPI move if you have two suitable displays.

## Editing, preview and export

- [ ] Do all eleven [basic checks](TESTING.md#test-spoolcut-in-five-minutes)
  with two compatible clips. Include the known HEVC recording on Windows
  and, on Fedora, the project that previously failed with 46.795 s vs
  46.785 s. Verify picture, sound and tail duration in VLC or another player.
- [ ] Play through several joins in the default renderer. Note stutters
  or glitches and retest the known joins in Fedora software preview.
- [ ] Reopen a pre-0.4.0 project and confirm its clip order and cuts. Save
  and reopen paths with spaces and Unicode. Compare expected and exported
  content across joins.
- [ ] Rename a source while the project is closed, then reopen. Confirm
  the missing styling in the bin and timeline and the relink message in
  preview. Relink and confirm the existing window resumes playback.
  Repeat with a source that disappears before playback reaches its join.
- [ ] Cancel an export after it starts and confirm that no final MKV or
  staging files remain. Attempt export to a pre-existing destination and
  confirm the existing file is preserved. On the Windows VM, repeat export
  to the previously tested VirtIO share if available.
