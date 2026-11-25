use std::{
    collections::BTreeMap, fs::File, io::Write, ops::Range, path::Path, process::Command,
    sync::atomic::AtomicBool,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    Clip, MediaInfo, Project, filesystem,
    media::{Media, Packet, Stream, media_info, probe, read_packets},
    process,
    time::{TimeBase, ffmpeg_time},
};

const MKV_TIME_BASE: TimeBase = TimeBase {
    numerator: 1,
    denominator: 1000,
};

#[derive(Debug, Serialize)]
pub struct ExportPlan {
    pub clips: Vec<PlannedClip>,
    pub duration_us: i64,
}

#[derive(Debug, Serialize)]
pub struct PlannedClip {
    pub source: String,
    pub in_pts: i64,
    pub out_pts: i64,
    pub timeline_start_us: i64,
    pub duration_us: i64,
    pub video_packets: Range<usize>,
    pub audio_packets: Range<usize>,
}

pub struct IndexedSource {
    media: Media,
    frame_pts: Vec<i64>,
}

impl IndexedSource {
    pub fn open(path: &Path, cancel: &AtomicBool) -> Result<Self> {
        let media =
            Media::open(path, cancel).with_context(|| format!("inspecting {}", path.display()))?;
        let mut frame_pts: Vec<_> = media
            .video_packets
            .iter()
            .filter_map(|packet| packet.pts)
            .collect();
        frame_pts.sort_unstable();
        Ok(Self { media, frame_pts })
    }

    pub fn info(&self) -> Result<MediaInfo> {
        media_info(&self.media)
    }

    pub fn plan_clip(&self, clip: &Clip) -> Result<PlannedClip> {
        plan_clip(&self.media, clip, 0)
    }

    pub fn check_source(&self) -> Result<()> {
        self.media.check_source()
    }

    pub fn frame_at(&self, clip: &Clip, source_seconds: f64) -> f64 {
        let base = self.media.video_base;
        let index = self
            .frame_pts
            .partition_point(|&pts| pts < clip.out_pts && base.seconds(pts) <= source_seconds);
        let pts = index
            .checked_sub(1)
            .map(|index| self.frame_pts[index])
            .unwrap_or(clip.in_pts);
        base.seconds(pts.max(clip.in_pts))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ExportProgress {
    Planning,
    Copying { completed: usize, total: usize },
    Joining,
    Verifying,
}

pub fn plan(project: &Project, root: &Path, cancel: &AtomicBool) -> Result<ExportPlan> {
    let (plan, _) = prepare(project, root, cancel)?;
    Ok(plan)
}

pub fn export(project: &Project, root: &Path, output: &Path, cancel: &AtomicBool) -> Result<()> {
    export_with_progress(project, root, output, cancel, |_| {})
}

pub fn export_with_progress(
    project: &Project,
    root: &Path,
    output: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(ExportProgress),
) -> Result<()> {
    ensure!(
        output
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("mkv")),
        "output must have a .mkv extension"
    );
    ensure!(
        !output.exists(),
        "output already exists: {}",
        output.display()
    );
    progress(ExportProgress::Planning);
    let (plan, sources) = prepare(project, root, cancel)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    let destination = parent.join(output.file_name().context("missing output filename")?);
    let workspace = tempfile::Builder::new()
        .prefix(".spoolcut-")
        .tempdir_in(&parent)
        .with_context(|| format!("creating temporary files in {}", parent.display()))?;
    // Absolute paths remain valid while the concat subprocess uses the temporary directory.
    let directory = workspace.path().canonicalize()?;
    let manifest_path = directory.join("timeline.ffconcat");
    let mut manifest = File::create(&manifest_path)?;
    writeln!(manifest, "ffconcat version 1.0")?;
    for (index, clip) in plan.clips.iter().enumerate() {
        progress(ExportProgress::Copying {
            completed: index,
            total: plan.clips.len(),
        });
        let name = format!("{index}.mkv");
        let media = &sources[&clip.source];
        media.check_source()?;
        write_clip(media, clip, &directory.join(&name), cancel)
            .with_context(|| format!("copying clip {} ({})", index + 1, clip.source))?;
        #[cfg(not(windows))]
        writeln!(manifest, "file {name}")?;
        #[cfg(windows)]
        {
            // A verbatim Windows path can contain '?' and the working directory
            // passed to CreateProcess has length limits. Use a complete file URL
            // so the concat demuxer does not try to resolve a relative URL.
            let segment = directory.join(&name);
            let path = segment
                .to_str()
                .context("FFmpeg requires a Unicode path on Windows")?;
            let quoted = path.replace('\'', "'\\''");
            writeln!(manifest, "file 'file:{quoted}'")?;
        }
        writeln!(manifest, "duration {}", ffmpeg_time(clip.duration_us))?;
    }
    drop(manifest);

    progress(ExportProgress::Joining);
    let staged_output = directory.join("output.mkv");
    let mut command = ffmpeg()?;
    #[cfg(windows)]
    let input = manifest_path.as_path();
    #[cfg(not(windows))]
    let input = {
        command.current_dir(&directory);
        Path::new("timeline.ffconcat")
    };
    command
        .args([
            "-protocol_whitelist",
            "file",
            "-f",
            "concat",
            "-safe",
            if cfg!(windows) { "0" } else { "1" },
            "-auto_convert",
            "0",
            "-i",
        ])
        .arg(input)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a:0",
            "-bsf:a",
            &aac_duration(&sources[&plan.clips[0].source]),
        ]);
    mkv_output(&mut command, &staged_output);
    process::run(&mut command, None, cancel).context("joining copied clips")?;
    progress(ExportProgress::Verifying);
    verify(&staged_output, &plan, &sources, cancel).context("verifying joined output")?;
    for source in sources.values() {
        source.check_source()?;
    }
    process::check(cancel)?;
    // Publish on the same filesystem, atomically and without overwriting an existing path.
    filesystem::publish(&staged_output, &destination)
        .with_context(|| format!("publishing {}", output.display()))?;
    Ok(())
}

fn prepare(
    project: &Project,
    root: &Path,
    cancel: &AtomicBool,
) -> Result<(ExportPlan, BTreeMap<String, Media>)> {
    ensure!(!project.clips.is_empty(), "project has no clips");
    let mut clips = Vec::with_capacity(project.clips.len());
    let mut sources: BTreeMap<String, Media> = BTreeMap::new();
    let mut duration_us = 0i64;
    for (index, clip) in project.clips.iter().enumerate() {
        process::check(cancel)?;
        let result = (|| {
            if !sources.contains_key(&clip.source) {
                let path = project
                    .sources
                    .get(&clip.source)
                    .context("unknown source")?;
                let path = root.join(path);
                let media = Media::open(&path, cancel)
                    .with_context(|| format!("inspecting {}", path.display()))?;
                if let Some(first) = clips.first() {
                    let first: &PlannedClip = first;
                    let reference = &sources[&first.source];
                    check_compatibility(&reference.video, &media.video)?;
                    check_compatibility(&reference.audio, &media.audio)?;
                }
                sources.insert(clip.source.clone(), media);
            }
            plan_clip(&sources[&clip.source], clip, duration_us)
        })()
        .with_context(|| format!("clip {} ({})", index + 1, clip.source))?;
        duration_us = duration_us
            .checked_add(result.duration_us)
            .context("timeline duration overflows")?;
        clips.push(result);
    }
    Ok((ExportPlan { clips, duration_us }, sources))
}

fn check_compatibility(reference: &Stream, other: &Stream) -> Result<()> {
    // The source timestamps can have different time bases. Only decoder parameters and
    // presentation properties need to match for Matroska's single track configuration.
    for field in reference.parameters.keys().chain(other.parameters.keys()) {
        ensure!(
            reference.parameters.get(field) == other.parameters.get(field),
            "{} streams differ in {}",
            other.codec_type,
            if field == "extradata" {
                "decoder configuration"
            } else {
                field
            }
        );
    }
    Ok(())
}

fn plan_clip(media: &Media, clip: &Clip, timeline_start_us: i64) -> Result<PlannedClip> {
    ensure!(
        clip.in_pts < clip.out_pts,
        "in_pts must be less than out_pts"
    );
    let &start = media.cuts.get(&clip.in_pts).with_context(|| {
        format!(
            "in_pts {} is not a safe IDR boundary. Run inspect for available points",
            clip.in_pts
        )
    })?;
    let end = if clip.out_pts == media.video_end_pts {
        media.video_packets.len()
    } else {
        *media.cuts.get(&clip.out_pts)
            .with_context(|| format!("out_pts {} is not a safe IDR boundary or video end. Run inspect for available points", clip.out_pts))?
    };
    let selected_video = &media.video_packets[start..end];
    ensure!(
        selected_video.iter().all(Packet::usable),
        "selected video contains discard flags, corrupt packets, or packet side data"
    );
    let final_frame = selected_video
        .iter()
        .max_by_key(|packet| packet.pts)
        .context("empty video range")?;
    ensure!(
        final_frame
            .pts
            .unwrap()
            .checked_add(final_frame.duration.unwrap_or(0))
            == Some(clip.out_pts),
        "the final frame does not end at out_pts"
    );
    let audio_packets = plan_audio(media, clip)?;
    let duration_us = media
        .video_base
        .micros(i128::from(clip.out_pts) - i128::from(clip.in_pts))?;
    ensure!(duration_us > 0, "clip is shorter than one microsecond");
    Ok(PlannedClip {
        source: clip.source.clone(),
        in_pts: clip.in_pts,
        out_pts: clip.out_pts,
        timeline_start_us,
        duration_us,
        video_packets: start..end,
        audio_packets,
    })
}

fn plan_audio(media: &Media, clip: &Clip) -> Result<Range<usize>> {
    let packets = &media.audio_packets;
    let compare = |a, v| media.audio_base.compare(a, media.video_base, i128::from(v));
    let frame = i128::from(media.audio_frame_ticks);
    let start = packets
        .partition_point(|packet| compare(i128::from(packet.pts.unwrap()), clip.in_pts).is_lt());
    let end = packets.partition_point(|packet| {
        compare(i128::from(packet.pts.unwrap()) + frame, clip.out_pts).is_le()
    });
    ensure!(end > start, "clip contains no complete AAC frame");

    let mut next_pts = i128::from(packets[start].pts.unwrap());
    ensure!(
        compare(next_pts - frame, clip.in_pts).is_le(),
        "audio starts more than one AAC frame after in_pts"
    );
    for packet in &packets[start..end] {
        ensure!(
            packet.usable()
                && packet.duration == Some(media.audio_frame_ticks)
                && Some(next_pts) == packet.pts.map(i128::from),
            "selected audio contains a gap, a partial AAC frame, or packet side data"
        );
        next_pts += frame;
    }
    ensure!(
        compare(next_pts + frame, clip.out_pts).is_ge(),
        "audio ends more than one AAC frame before out_pts"
    );
    Ok(start..end)
}

fn ffmpeg() -> Result<Command> {
    let mut command = process::tool("ffmpeg")?;
    command.args([
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "warning",
        "-xerror",
        "-n",
    ]);
    Ok(command)
}

fn mkv_output(command: &mut Command, output: &Path) {
    command
        .args([
            "-c",
            "copy",
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
            "-avoid_negative_ts",
            "disabled",
            "-f",
            "matroska",
        ])
        .arg(output);
}

fn write_clip(media: &Media, clip: &PlannedClip, output: &Path, cancel: &AtomicBool) -> Result<()> {
    let offset = media.video_base.micros(-i128::from(clip.in_pts))?;
    let select =
        |range: &Range<usize>| format!("noise=drop=lt(n\\,{})+gte(n\\,{})", range.start, range.end);
    let mut command = ffmpeg()?;
    // The bitstream filter counts demuxed packets independently for each mapped stream.
    // Timestamp seeking can select a different packet at the same nominal cut time.
    command
        .args([
            "-copyts",
            "-itsoffset",
            &ffmpeg_time(offset),
            "-protocol_whitelist",
            "file",
            "-f",
            "mov",
            "-i",
        ])
        .arg(&media.path)
        .args([
            "-map",
            &format!("0:{}", media.video.index),
            "-map",
            &format!("0:{}", media.audio.index),
        ])
        .args([
            "-bsf:v",
            &select(&clip.video_packets),
            "-bsf:a",
            &format!("{},{}", select(&clip.audio_packets), aac_duration(media)),
        ]);
    mkv_output(&mut command, output);
    process::run(&mut command, None, cancel)
}

fn aac_duration(media: &Media) -> String {
    // Every selected AAC-LC packet is a complete frame. Set its duration in
    // the current time base so FFmpeg cannot infer a longer Matroska tail.
    let TimeBase {
        numerator,
        denominator,
    } = media.audio_base;
    format!(
        "setts=duration=round({}*{numerator}/{denominator}/TB)",
        media.audio_frame_ticks
    )
}

#[derive(Deserialize)]
struct OutputDuration {
    format: ContainerDuration,
}

#[derive(Deserialize)]
struct ContainerDuration {
    duration: String,
}

fn verify(
    path: &Path,
    plan: &ExportPlan,
    sources: &BTreeMap<String, Media>,
    cancel: &AtomicBool,
) -> Result<()> {
    let packets = read_packets(path, "matroska", cancel)?;
    let mut video = packets.iter().filter(|packet| packet.stream_index == 0);
    let mut audio = packets.iter().filter(|packet| packet.stream_index == 1);
    ensure!(
        packets.iter().all(|packet| packet.stream_index < 2),
        "unexpected output stream"
    );
    for (index, clip) in plan.clips.iter().enumerate() {
        process::check(cancel)?;
        let source = &sources[&clip.source];
        let start_us = source.video_base.micros(i128::from(clip.in_pts))?;
        for (label, expected, base, actual) in [
            (
                "video",
                &source.video_packets[clip.video_packets.clone()],
                source.video_base,
                &mut video as &mut dyn Iterator<Item = &Packet>,
            ),
            (
                "audio",
                &source.audio_packets[clip.audio_packets.clone()],
                source.audio_base,
                &mut audio as &mut dyn Iterator<Item = &Packet>,
            ),
        ] {
            for packet in expected {
                let output = actual
                    .next()
                    .with_context(|| format!("missing {label} packet in clip {}", index + 1))?;
                let expected_us = i128::from(base.micros(i128::from(packet.pts.unwrap()))?)
                    - i128::from(start_us)
                    + i128::from(clip.timeline_start_us);
                let actual_us = MKV_TIME_BASE
                    .micros(i128::from(output.pts.context("output packet has no PTS")?))?;
                // The intermediate and final Matroska muxes round to millisecond ticks.
                ensure!(
                    (expected_us - i128::from(actual_us)).abs() <= 1_001,
                    "{label} timestamp changed in clip {} (expected {} µs, got {} µs)",
                    index + 1,
                    expected_us,
                    actual_us
                );
                ensure!(
                    output.size == packet.size && output.is_key() == packet.is_key(),
                    "{label} packet framing changed in clip {}",
                    index + 1
                );
            }
        }
    }
    ensure!(
        video.next().is_none() && audio.next().is_none(),
        "unexpected extra output packets"
    );
    let OutputDuration { format } = probe(
        path,
        "matroska",
        &["-show_format"],
        "format=duration",
        cancel,
    )?;
    let (whole, fraction) = format
        .duration
        .split_once('.')
        .context("invalid output duration")?;
    ensure!(fraction.len() == 6, "invalid output duration precision");
    let actual_us =
        i128::from(whole.parse::<i64>()?) * 1_000_000 + i128::from(fraction.parse::<u32>()?);
    ensure!(
        (actual_us - i128::from(plan.duration_us)).abs() <= 2_000,
        "output duration is {} µs, expected {} µs",
        actual_us,
        plan.duration_us
    );
    Ok(())
}
