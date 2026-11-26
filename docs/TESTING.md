# Test SpoolCut in five minutes

Download the AppImage for Fedora x86-64 or the portable ZIP for Windows
11 x86-64 from [Releases](https://github.com/bugrakoc/SpoolCut/releases).
Use a **0.4.x** build for this campaign. The [README](../README.md#download-and-run)
includes launch instructions and renderer settings.

Start with two short, ordinary MP4 or MOV clips from the same device and
recording mode. Use H.264 (`avc1`) or HEVC (`hvc1`) with one AAC-LC audio
track. Compatible settings include matching resolution, video and audio
parameters, and decoder configuration. Pick clips with a few scene changes
and some audible sound. A few seconds per clip is enough if there are
multiple safe keyframes. The [supported footage section](../README.md#supported-footage)
covers the restrictions in more detail.

| # | Try | What to look for |
| --- | --- | --- |
| 1 | Launch SpoolCut. | Main window, toolbar and native file picker open normally. |
| 2 | Import both clips and double-click each in the bin to append it. | Both appear in the timeline. |
| 3 | Press **Space** twice. | Picture and audio play, then pause. |
| 4 | Drag in the timeline ruler. | Playhead and picture follow the scrub. Clicking jumps to a safe keyframe. |
| 5 | Drag a clip edge inward. Move the playhead inside a clip and press **S**. | Trim and split snap to safe cut points. |
| 6 | Drag a clip body past another clip. | Order changes and the sequence stays gapless. |
| 7 | Play from just before a join. | Playback continues into the next clip. Note any pause or audio glitch. |
| 8 | Press **Ctrl+Z**, then **Ctrl+Shift+Z**. | The last edit is undone and restored. |
| 9 | Save a project, close it and reopen the JSON. | Sources, edits and order come back. |
| 10 | Choose **File → Export Matroska** and save to a new MKV. | Export and verification finish. |
| 11 | Open the MKV in your usual player. | Check the cut order, playback across joins, duration and A/V sync. |

## Optional extended tests

- Rename a source after saving, reopen the project, then select the missing
  source and relink it. Also try moving a source before playback reaches
  its join. Check that the missing state appears and playback recovers.
- Import and export with spaces and Unicode in paths. Try a long recording
  or higher resolutions to see how analysis, scrubbing and playback behave.
- Try supported H.264 and HEVC samples in MP4 and MOV independently. Try
  variable-frame-rate footage as an exploratory case. Record the file
  characteristics and any message SpoolCut shows.
- Compare default and forced preview modes using the [README instructions](../README.md#download-and-run).
  Note the actual renderer in **Help → System Information** after playing
  a clip. Try X11 and Wayland, or different desktop environments, where available.
- Try high-DPI scaling or dragging the window between monitors with
  different scaling. Check the file picker, timeline, dialogs and preview.
- Compare planned and actual export duration, A/V sync, and playback at
  the start, at joins and near the final frame. Matroska rounds timestamps to
  milliseconds and exports keep complete AAC frames inside the selected video
  range. For a destination test, report the drive type or filesystem if known.

## Submit a compatibility report

Successful, partial and unsuccessful tests are welcome. Open
[GitHub Discussions](https://github.com/bugrakoc/SpoolCut/discussions), find the
pinned **SpoolCut 0.4.x — Public Testing & Hardware Compatibility** thread and
reply using as much of the template as you know. A single passed or failed
check is useful. Use [Issues](https://github.com/bugrakoc/SpoolCut/issues/new/choose)
for a reproducible bug and link it from your compatibility report if you
have one. When Discussions is unavailable, you can still use this checklist
and save your compatibility report for the testing thread.

After playing a clip, choose **Help → System Information → Copy Information**.
Paste it in the template if you want to share it, and remove anything private
from any extra materials you add.

### Copyable report

```markdown
SpoolCut version:
Installation (AppImage / portable ZIP / source / other):
Footage (container, video/audio codecs, resolution, frame rate, recording mode):

Results (Pass / Fail / Not tested):
1. Launch:
2. Import and append:
3. Play and pause:
4. Scrub:
5. Trim and split:
6. Rearrange:
7. Play across a join:
8. Undo and redo:
9. Save and reopen:
10. Lossless export:
11. Verify exported playback:

Performance (import, scrubbing, joins, export, or anything noticeable):
Notes and optional checks:
Bug links (if any):

System Information (optional):
```
