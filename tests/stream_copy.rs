use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;
use spoolcut::{Clip, Project};

const SPOOLCUT: &str = env!("CARGO_BIN_EXE_spoolcut");

#[derive(Clone, Copy)]
enum FixtureVideo {
    H264,
    Hevc { open_gop: bool },
    VariableRate,
}

fn run(command: &mut Command) -> Output {
    let result = command
        .output()
        .unwrap_or_else(|error| panic!("{command:?}: {error}"));
    assert!(
        result.status.success(),
        "{command:?}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result
}

fn fixture(path: &Path, codec: FixtureVideo, variant: u32, time_scale: u32) {
    let (rate, encoder, tag) = match codec {
        FixtureVideo::H264 | FixtureVideo::VariableRate => ("25", "libx264", "avc1"),
        FixtureVideo::Hevc { .. } => ("30000/1001", "libx265", "hvc1"),
    };
    let mut command = Command::new("ffmpeg");
    command
        .args(["-nostdin", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("testsrc2=size=96x64:rate={rate}:duration=3"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "sine=frequency={}:sample_rate=48000:duration=3",
            440 + 440 * variant
        ))
        .args(["-c:v", encoder, "-tag:v", tag, "-threads:v", "1"]);
    let mut video_filter = format!("trim=end_frame=64,hue=h={}", variant * 90);
    match codec {
        FixtureVideo::H264 => {
            command.args([
                "-x264-params",
                "keyint=16:min-keyint=16:scenecut=0:open-gop=0:bframes=2",
            ]);
        }
        FixtureVideo::Hevc { open_gop } => {
            command.arg("-x265-params").arg(format!(
                "keyint=16:min-keyint=16:scenecut=0:open-gop={}:bframes=2:pools=none:frame-threads=1:log-level=error",
                u8::from(open_gop)
            ));
        }
        FixtureVideo::VariableRate => {
            command.args([
                "-x264-params",
                "keyint=16:min-keyint=16:scenecut=0:open-gop=0:bframes=0",
            ]);
            video_filter.push_str(",setpts=N+floor((N+8)/16)");
        }
    }
    command.args(["-fps_mode", "vfr", "-vf"]).arg(video_filter);
    command
        .args(["-c:a", "aac", "-ac", "2", "-video_track_timescale"])
        .arg(time_scale.to_string())
        .args(["-movflags", "+faststart"])
        .arg(path);
    run(&mut command);
}

#[derive(Deserialize)]
struct Inspection {
    safe_in_pts: Vec<i64>,
    video_end_pts: i64,
}

fn inspect(path: &Path) -> Inspection {
    let result = run(Command::new(SPOOLCUT).arg("inspect").arg(path));
    serde_json::from_slice(&result.stdout).unwrap()
}

#[derive(Debug, Deserialize)]
struct Packet {
    pts: i64,
    pts_time: String,
    flags: String,
    data_hash: String,
}

fn packets(path: &Path, stream: &str) -> Vec<Packet> {
    #[derive(Deserialize)]
    struct Result {
        packets: Vec<Packet>,
    }
    let result = run(Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            stream,
            "-show_packets",
            "-show_data_hash",
            "sha256",
            "-show_entries",
            "packet=pts,pts_time,flags,data_hash",
            "-of",
            "json",
            "-i",
        ])
        .arg(path));
    serde_json::from_slice::<Result>(&result.stdout)
        .unwrap()
        .packets
}

fn frames(path: &Path) -> Vec<String> {
    let decoded = run(Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-xerror", "-threads", "1", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-fps_mode",
            "passthrough",
            "-threads",
            "1",
            "-f",
            "framemd5",
            "-",
        ]));
    String::from_utf8(decoded.stdout)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.rsplit(',').next().unwrap().trim().to_owned())
        .collect()
}

#[derive(Deserialize)]
struct Plan {
    clips: Vec<PlannedClip>,
    duration_us: i64,
}

#[derive(Deserialize)]
struct PlannedClip {
    timeline_start_us: i64,
    duration_us: i64,
    video_packets: std::ops::Range<usize>,
    audio_packets: std::ops::Range<usize>,
}

fn sequence(
    codec: FixtureVideo,
    scales: [u32; 2],
    duration_us: i64,
    audio_matches_video_at_joins: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let a = directory.path().join("A's camera file.mp4");
    let b = directory.path().join("-B camera file.mp4");
    fixture(&a, codec, 0, scales[0]);
    fixture(&b, codec, 1, scales[1]);
    let a_info = inspect(&a);
    let b_info = inspect(&b);
    assert_eq!(a_info.safe_in_pts.len(), 4);
    assert_eq!(b_info.safe_in_pts.len(), 4);
    // FFmpeg's MP4 muxer marks the final packet of this VFR fixture as
    // discarded via its edit list. Exercise two IDR endpoints in this case.
    let (last_gop, last_out) = match codec {
        FixtureVideo::VariableRate => (2, a_info.safe_in_pts[3]),
        _ => (3, a_info.video_end_pts),
    };
    let project = Project {
        sources: [
            ("a".into(), PathBuf::from(a.file_name().unwrap())),
            ("b".into(), PathBuf::from(b.file_name().unwrap())),
        ]
        .into(),
        clips: vec![
            Clip {
                source: "a".into(),
                in_pts: a_info.safe_in_pts[0],
                out_pts: a_info.safe_in_pts[1],
            },
            Clip {
                source: "b".into(),
                in_pts: b_info.safe_in_pts[1],
                out_pts: b_info.safe_in_pts[2],
            },
            Clip {
                source: "a".into(),
                in_pts: a_info.safe_in_pts[last_gop],
                out_pts: last_out,
            },
        ],
    };
    let project_path = directory.path().join("timeline.json");
    fs::write(&project_path, serde_json::to_vec(&project).unwrap()).unwrap();
    let planned = run(Command::new(SPOOLCUT)
        .current_dir("/")
        .arg("plan")
        .arg(&project_path));
    let plan: Plan = serde_json::from_slice(&planned.stdout).unwrap();
    assert!((plan.duration_us - duration_us).abs() <= 1);
    let output = directory.path().join("output.mkv");
    run(Command::new(SPOOLCUT)
        .current_dir("/")
        .arg("export")
        .arg(&project_path)
        .arg(&output));

    let a_frames = frames(&a);
    let b_frames = frames(&b);
    let expected_frames = [
        &a_frames[..16],
        &b_frames[16..32],
        &a_frames[last_gop * 16..(last_gop + 1) * 16],
    ]
    .concat();
    assert_eq!(frames(&output), expected_frames);

    let output_video = packets(&output, "v:0");
    let output_audio = packets(&output, "a:0");
    let a_video = packets(&a, "v:0");
    let b_video = packets(&b, "v:0");
    let a_audio = packets(&a, "a:0");
    let b_audio = packets(&b, "a:0");
    let mut output_video_offset = 0;
    let mut output_audio_offset = 0;

    for (index, clip) in plan.clips.iter().enumerate() {
        let (video, audio) = if index == 1 {
            (&b_video, &b_audio)
        } else {
            (&a_video, &a_audio)
        };
        let expected_video = &video[clip.video_packets.clone()];
        let expected_audio = &audio[clip.audio_packets.clone()];
        let actual_video =
            &output_video[output_video_offset..output_video_offset + expected_video.len()];
        let actual_audio =
            &output_audio[output_audio_offset..output_audio_offset + expected_audio.len()];
        let source_start = seconds(&expected_video[0]);
        let start = clip.timeline_start_us as f64 / 1_000_000.0;
        let duration = clip.duration_us as f64 / 1_000_000.0;

        assert!(actual_video[0].flags.contains('K'));
        assert!((seconds(&actual_video[0]) - start).abs() < 0.001);
        if audio_matches_video_at_joins {
            assert!((seconds(&actual_audio[0]) - seconds(&actual_video[0])).abs() < 0.001);
        }
        assert!(seconds(&actual_audio[0]) >= start - 0.001);
        assert!(seconds(&actual_audio[0]) - start < 1024.0 / 48000.0 + 0.001);
        assert!(
            seconds(actual_audio.last().unwrap()) + 1024.0 / 48000.0 <= start + duration + 0.001
        );
        for (expected, actual) in expected_video
            .iter()
            .zip(actual_video)
            .chain(expected_audio.iter().zip(actual_audio))
        {
            assert_eq!(expected.data_hash, actual.data_hash);
            assert!(
                (seconds(actual) - (seconds(expected) - source_start + start)).abs() <= 0.001_001
            );
        }
        output_video_offset += expected_video.len();
        output_audio_offset += expected_audio.len();
    }
    assert_eq!(output_video.len(), output_video_offset);
    assert_eq!(output_audio.len(), output_audio_offset);

    let result = run(Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
            "-i",
        ])
        .arg(output));
    let actual: f64 = String::from_utf8(result.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!((actual - duration_us as f64 / 1_000_000.0).abs() < 0.002);
}

fn seconds(packet: &Packet) -> f64 {
    packet.pts_time.parse().unwrap()
}

#[test]
fn copies_h264_and_aac_across_aligned_joins() {
    sequence(FixtureVideo::H264, [12800, 12800], 1_920_000, true);
}

#[test]
fn copies_hevc_and_aac_across_fractional_joins_with_different_time_bases() {
    sequence(
        FixtureVideo::Hevc { open_gop: false },
        [30000, 90000],
        1_601_600,
        false,
    );
}

#[test]
fn copies_h264_with_variable_frame_intervals() {
    sequence(FixtureVideo::VariableRate, [12800, 12800], 2_040_000, false);
}

#[test]
fn rejects_non_idr_cuts_and_incompatible_video_without_publishing() {
    let directory = tempfile::tempdir().unwrap();
    let a = directory.path().join("open.mp4");
    let b = directory.path().join("h264.mp4");
    fixture(&a, FixtureVideo::Hevc { open_gop: true }, 0, 30000);
    fixture(&b, FixtureVideo::H264, 1, 12800);
    let a_info = inspect(&a);
    let b_info = inspect(&b);
    let flagged = packets(&a, "v:0")
        .into_iter()
        .find(|packet| packet.flags.contains('K') && !a_info.safe_in_pts.contains(&packet.pts))
        .expect("open GOP fixture must have a non-IDR packet marked as a keyframe");
    let mut project = Project {
        sources: [("a".into(), a), ("b".into(), b)].into(),
        clips: vec![
            Clip {
                source: "a".into(),
                in_pts: flagged.pts,
                out_pts: a_info.video_end_pts,
            },
            Clip {
                source: "b".into(),
                in_pts: 0,
                out_pts: b_info.safe_in_pts[1],
            },
        ],
    };
    let output = directory.path().join("output.mkv");
    let cancel = AtomicBool::new(false);
    let error = spoolcut::export(&project, directory.path(), &output, &cancel).unwrap_err();
    assert!(format!("{error:#}").contains("not a safe IDR boundary"));
    assert!(!output.exists());
    project.clips[0].in_pts = a_info.safe_in_pts[0];
    let error = spoolcut::export(&project, directory.path(), &output, &cancel).unwrap_err();
    assert!(
        format!("{error:#}").contains("video streams differ"),
        "{error:#}"
    );
    assert!(!output.exists());
}

#[test]
fn cancels_export_without_leaving_temporary_or_final_files() {
    let directory = tempfile::tempdir().unwrap();
    let a = directory.path().join("a.mp4");
    fixture(&a, FixtureVideo::H264, 0, 12800);
    let info = inspect(&a);
    let project = Project {
        sources: [("a".into(), a)].into(),
        clips: (0..25)
            .map(|_| Clip {
                source: "a".into(),
                in_pts: 0,
                out_pts: info.safe_in_pts[1],
            })
            .collect(),
    };
    let output = directory.path().join("output.mkv");
    let cancel = AtomicBool::new(false);
    thread::scope(|scope| {
        let exporting =
            scope.spawn(|| spoolcut::export(&project, directory.path(), &output, &cancel));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let workspace_created = fs::read_dir(directory.path()).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".spoolcut-")
            });
            if workspace_created || exporting.is_finished() || Instant::now() >= deadline {
                cancel.store(true, Ordering::Relaxed);
                assert!(workspace_created, "export did not reach staging");
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(format!("{:#}", exporting.join().unwrap().unwrap_err()).contains("cancelled"));
    });
    assert!(!output.exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn keeps_an_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("output.mkv");
    fs::write(&output, b"existing").unwrap();
    let project = Project {
        sources: Default::default(),
        clips: Vec::new(),
    };
    let error =
        spoolcut::export(&project, directory.path(), &output, &AtomicBool::new(false)).unwrap_err();
    assert!(format!("{error:#}").contains("output already exists"));
    assert_eq!(fs::read(&output).unwrap(), b"existing");
}

#[test]
fn unicode_paths_and_a_destination_created_during_verification() {
    let scratch = tempfile::tempdir().unwrap();
    let mut destination_dir = scratch.path().join("export # çığ ' space");
    // Keep each component below NAME_MAX while exercising a Windows path over MAX_PATH.
    for _ in 0..5 {
        destination_dir =
            destination_dir.join("long directory component with spaces and unicode ş");
    }
    fs::create_dir_all(&destination_dir).unwrap();

    let source = scratch.path().join("kamera görüntüsü 01.mp4");
    fixture(&source, FixtureVideo::H264, 0, 12800);
    let info = inspect(&source);
    let project = Project {
        sources: [("a".into(), PathBuf::from(source.file_name().unwrap()))].into(),
        clips: vec![Clip {
            source: "a".into(),
            in_pts: info.safe_in_pts[0],
            out_pts: info.safe_in_pts[1],
        }],
    };
    let project_path = scratch.path().join("kurgu ğ.json");
    project.save(&project_path).unwrap();
    assert_eq!(Project::load(&project_path).unwrap(), project);
    let output = destination_dir.join("birleşik video.mkv");
    let cancel = AtomicBool::new(false);
    let error =
        spoolcut::export_with_progress(&project, scratch.path(), &output, &cancel, |stage| {
            if matches!(stage, spoolcut::ExportProgress::Verifying) {
                fs::write(&output, b"existing").unwrap();
            }
        })
        .unwrap_err();
    assert!(format!("{error:#}").contains("publishing"), "{error:#}");
    assert_eq!(fs::read(&output).unwrap(), b"existing");
    assert_eq!(fs::read_dir(&destination_dir).unwrap().count(), 1);

    fs::remove_file(&output).unwrap();
    spoolcut::export(&project, scratch.path(), &output, &cancel).unwrap();
    assert!(output.is_file());
    assert_eq!(fs::read_dir(destination_dir).unwrap().count(), 1);

    #[cfg(windows)]
    if let Some(path) = std::env::var_os("SPOOLCUT_TEST_OTHER_VOLUME") {
        let other = tempfile::tempdir_in(path).unwrap();
        let source_drive = scratch.path().components().next();
        assert_ne!(source_drive, other.path().components().next());
        let across_drives = other.path().join("ikinci sürücü.mkv");
        spoolcut::export(&project, scratch.path(), &across_drives, &cancel).unwrap();
        assert!(across_drives.is_file());
        assert_eq!(fs::read_dir(other.path()).unwrap().count(), 1);
    }
}

#[cfg(windows)]
#[test]
fn a_locked_project_cannot_be_replaced_and_keeps_its_previous_contents() {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};

    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("locked.json");
    let original = Project::default();
    original.save(&path).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let updated = Project {
        sources: [("a".into(), PathBuf::from("a.mp4"))].into(),
        clips: Vec::new(),
    };
    assert!(updated.save(&path).is_err());
    drop(lock);
    assert_eq!(Project::load(&path).unwrap(), original);
    assert_eq!(fs::read_dir(scratch.path()).unwrap().count(), 1);
    updated.save(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap(), updated);
}
