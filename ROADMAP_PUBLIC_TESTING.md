# SpoolCut 0.4.0 public testing

The initial candidate is `0.4.0-rc.1`. Work starts from
[`ec0780c`](https://github.com/bugrakoc/SpoolCut/commit/ec0780c61fc26c44060c25a9184f360726ae1f6c)
on `main`, after PR #4 and the 0.3.0 release.

The C++20 core, Qt Widgets editor, renderer defaults, stream-copy export,
and Linux AppImage and Windows portable ZIP pipelines already exist. The
[0.3.0 audit](docs/CPP_QT_STATUS.md#030-production-audit) records prior CI
and maintainer evidence. The Help menu currently has only About. GitHub
Discussions is disabled at the start of this work.

| Milestone | Acceptance criteria | Status |
| --- | --- | --- |
| Assessment | Verify the baseline, prior test evidence, release assets and CI, and start the public-testing branch. | Complete |
| Diagnostics | Help opens a responsive native dialog. The visible report matches the clipboard, fields use scoped local sources, and unavailable data has an explicit label. | Implemented. Local tests passed. |
| Community | Provide bug and feature forms, a short compatibility checklist and report template, and a sourced matrix. Enable Discussions and pin the testing thread where permissions allow. | Templates and instructions ready. Owner setup is pending. |
| Presentation | Clarify supported workflows, downloads, limitations and privacy. Prepare outreach drafts and capture genuine screenshots and a demo, or provide capture instructions for unavailable assets. | README, outreach drafts, screenshots and recording ready. |
| Candidate | Build and smoke-test both packages. Check app, CLI and package versions and corresponding sources. Open the PR, record CI and give the maintainer the candidate downloads and regression checklist. | Local Linux checks passed. The [candidate PR](https://github.com/bugrakoc/SpoolCut/pull/5) records hosted results and downloads. |
| Acceptance | Maintainer runs the packaged Fedora and Windows checks and approves merge and the final release. | Pending |

Editing behavior, project compatibility, lossless export, software preview
on Windows and OpenGL with fallback on Linux are regression requirements.
Public outreach and the final `0.4.0` release require maintainer approval.

The demo exposed a preview stall after quick timeline edits. A desktop
regression test reproduced it before the fix and verifies that playback
resumes after editing. The fix lets the replacement preview load without an
asynchronous stop clearing it.
