# GitHub community setup

## Repository settings

1. Open **Settings → General → Features** and enable **Discussions**.
2. In **Discussions**, click the pencil beside **Categories**. Keep or
   create **Announcements** (Announcement), **General** (Open-ended
   discussion), and **Ideas** (Open-ended discussion). Add **Testing &
   Compatibility** as an open-ended category for test results and
   hardware questions.
3. Choose **New discussion → Testing & Compatibility**. Use the title
   **SpoolCut 0.4.x — Public Testing & Hardware Compatibility** and paste
   the [discussion body](PUBLIC_TESTING_DISCUSSION.md).
4. Open the posted discussion and use **Pin discussion** in its sidebar.
   Verify it appears in the repository-wide Discussions list.
5. Confirm that **Issues → New issue** shows Bug report, Feature request,
   and the Compatibility report link after the PR is merged. Successful
   compatibility reports belong in Discussions.

See GitHub's [Discussions quickstart](https://docs.github.com/en/discussions/quickstart)
the [category guide](https://docs.github.com/en/discussions/managing-discussions-for-your-community/managing-categories-for-discussions)
and the [pinning instructions](https://docs.github.com/en/discussions/managing-discussions-for-your-community/managing-discussions#pinning-a-discussion).

## Repository description and topics

The existing description is “Cut and join video without re-encoding.”
For more context in GitHub search results, use the gear beside **About** to
set the description to **Lossless, keyframe-aligned video trimming and
joining for Linux and Windows.** In the same panel add `video-editing`,
`lossless`, `ffmpeg`, `qt6`, `libmpv`, `linux`, and `windows`.

## Candidate publication

The candidate packages and corresponding source builds are CI artifacts.
For an anonymous download link after candidate acceptance, create a GitHub
pre-release and check **Set as a pre-release**. Attach both tested binary
packages, their corresponding source archives and all four SHA-256 files.
Verify the tagged commit matches the packages' embedded source revision.
For final `0.4.0`, change [`VERSION`](../VERSION) to `0.4.0` and rebuild the
stable packages before publishing. See [release notes](RELEASE_NOTES_0.4.0.md)
and [packaging](PACKAGING.md).
