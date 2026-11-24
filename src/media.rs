use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, Metadata},
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{TimeBase, process};

const STREAM_FIELDS: &str = "stream=index,codec_type,codec_name,codec_tag_string,profile,level,width,height,pix_fmt,field_order,color_space,color_range,color_transfer,color_primaries,chroma_location,sample_aspect_ratio,sample_fmt,sample_rate,channels,channel_layout,extradata,time_base:stream_disposition=attached_pic:stream_side_data=side_data_type";
const PACKET_FIELDS: &str =
    "packet=stream_index,pts,dts,duration,pos,size,flags:packet_side_data=side_data_type";

#[derive(Debug, Serialize)]
pub struct MediaInfo {
    pub video_codec: String,
    pub audio_codec: String,
    pub video_time_base: TimeBase,
    pub safe_in_pts: Vec<i64>,
    pub video_end_pts: i64,
}

pub(crate) struct Media {
    pub path: PathBuf,
    metadata: Metadata,
    pub video: Stream,
    pub audio: Stream,
    pub video_base: TimeBase,
    pub audio_base: TimeBase,
    pub audio_frame_ticks: i64,
    pub video_packets: Vec<Packet>,
    pub audio_packets: Vec<Packet>,
    pub cuts: BTreeMap<i64, usize>,
    pub video_end_pts: i64,
}

#[derive(Deserialize)]
pub(crate) struct Stream {
    pub index: usize,
    pub codec_type: String,
    pub time_base: String,
    #[serde(default)]
    disposition: Disposition,
    #[serde(default)]
    side_data_list: Vec<Value>,
    #[serde(flatten)]
    pub parameters: BTreeMap<String, Value>,
}

#[derive(Default, Deserialize)]
struct Disposition {
    attached_pic: u8,
}

impl Stream {
    fn parameter(&self, name: &str) -> Result<&str> {
        self.parameters
            .get(name)
            .and_then(Value::as_str)
            .with_context(|| format!("stream {} has no {name}", self.index))
    }
}

#[derive(Deserialize)]
pub(crate) struct Packet {
    pub stream_index: usize,
    pub pts: Option<i64>,
    pub dts: Option<i64>,
    pub duration: Option<i64>,
    pos: Option<String>,
    pub size: Option<String>,
    flags: String,
    #[serde(default)]
    side_data_list: Vec<Value>,
}

impl Packet {
    pub(crate) fn is_key(&self) -> bool {
        self.flags.contains('K')
    }

    pub(crate) fn usable(&self) -> bool {
        !self.flags.contains(['D', 'C']) && self.side_data_list.is_empty()
    }
}

#[derive(Deserialize)]
struct Streams {
    streams: Vec<Stream>,
}

#[derive(Deserialize)]
struct Packets {
    #[serde(default)]
    packets: Vec<Packet>,
}

#[derive(Clone, Copy)]
enum VideoCodec {
    H264,
    Hevc,
}

impl Media {
    pub(crate) fn open(path: &Path, cancel: &AtomicBool) -> Result<Self> {
        let path = path.canonicalize()?;
        let metadata = fs::metadata(&path)?;
        ensure!(metadata.is_file(), "source must be a regular file");
        let Streams { streams } = probe(
            &path,
            "mov",
            &["-show_data", "-show_streams"],
            STREAM_FIELDS,
            cancel,
        )?;
        let mut audio = None;
        let mut video = None;
        for stream in streams {
            let selected = match stream.codec_type.as_str() {
                "video" if stream.disposition.attached_pic == 0 => &mut video,
                "audio" => &mut audio,
                _ => continue,
            };
            ensure!(
                selected.is_none(),
                "source has multiple {} streams",
                stream.codec_type
            );
            *selected = Some(stream);
        }
        let video = video.context("missing video stream")?;
        let audio = audio.context("missing audio stream")?;
        ensure!(
            video.side_data_list.is_empty() && audio.side_data_list.is_empty(),
            "stream side data such as a rotation matrix is not supported"
        );
        let codec = match (
            video.parameter("codec_name")?,
            video.parameter("codec_tag_string")?,
        ) {
            ("h264", "avc1") => VideoCodec::H264,
            ("hevc", "hvc1") => VideoCodec::Hevc,
            _ => bail!("expected H.264 (avc1) or HEVC (hvc1) video"),
        };
        ensure!(
            audio.parameter("codec_name")? == "aac" && audio.parameter("profile")? == "LC",
            "expected AAC-LC audio"
        );
        let video_base = TimeBase::parse(&video.time_base)?;
        let audio_base = TimeBase::parse(&audio.time_base)?;
        let sample_rate: i32 = audio.parameter("sample_rate")?.parse()?;
        let frame_numerator = 1024 * i64::from(audio_base.denominator);
        let frame_denominator = i64::from(sample_rate) * i64::from(audio_base.numerator);
        ensure!(
            frame_denominator > 0 && frame_numerator % frame_denominator == 0,
            "audio time base does not represent a whole AAC frame"
        );
        let audio_frame_ticks = frame_numerator / frame_denominator;

        let mut video_packets = Vec::new();
        let mut audio_packets = Vec::new();
        for packet in read_packets(&path, "mov", cancel)? {
            if packet.stream_index == video.index {
                video_packets.push(packet);
            } else if packet.stream_index == audio.index {
                audio_packets.push(packet);
            }
        }
        validate_timestamps(&video_packets, false)?;
        validate_timestamps(&audio_packets, true)?;

        let mut file = File::open(&path)?;
        let config = decode_extradata(video.parameter("extradata")?)?;
        let length_offset = match codec {
            VideoCodec::H264 => 4,
            VideoCodec::Hevc => 21,
        };
        ensure!(
            config.first() == Some(&1),
            "invalid video decoder configuration"
        );
        let length_bytes = usize::from(
            (config
                .get(length_offset)
                .context("missing NAL length size")?
                & 3)
                + 1,
        );
        let (cuts, video_end_pts) =
            index_cuts(&video_packets, &mut file, codec, length_bytes, cancel)?;
        let media = Self {
            path,
            metadata,
            video,
            audio,
            video_base,
            audio_base,
            audio_frame_ticks,
            video_packets,
            audio_packets,
            cuts,
            video_end_pts,
        };
        media.check_source()?;
        Ok(media)
    }

    pub(crate) fn check_source(&self) -> Result<()> {
        let current = fs::metadata(&self.path)?;
        ensure!(
            self.metadata.len() == current.len()
                && self.metadata.modified()? == current.modified()?,
            "source changed during operation: {}",
            self.path.display()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                self.metadata.ino() == current.ino() && self.metadata.dev() == current.dev(),
                "source was replaced during operation: {}",
                self.path.display()
            );
        }
        Ok(())
    }
}

pub fn inspect(path: &Path, cancel: &AtomicBool) -> Result<MediaInfo> {
    let media =
        Media::open(path, cancel).with_context(|| format!("inspecting {}", path.display()))?;
    Ok(MediaInfo {
        video_codec: media.video.parameter("codec_name")?.into(),
        audio_codec: media.audio.parameter("codec_name")?.into(),
        video_time_base: media.video_base,
        safe_in_pts: media.cuts.into_keys().collect(),
        video_end_pts: media.video_end_pts,
    })
}

fn validate_timestamps(packets: &[Packet], audio: bool) -> Result<()> {
    ensure!(!packets.is_empty(), "stream has no packets");
    let mut previous_dts = None;
    let mut presentation_times = HashSet::new();
    for packet in packets {
        let pts = packet.pts.context("source packet has no PTS")?;
        let dts = packet.dts.context("source packet has no DTS")?;
        ensure!(
            previous_dts.is_none_or(|previous| dts > previous),
            "source DTS is not strictly increasing"
        );
        ensure!(dts <= pts, "source DTS is later than PTS");
        if audio {
            ensure!(dts == pts, "audio PTS differs from DTS");
        } else {
            ensure!(presentation_times.insert(pts), "video packets share a PTS");
        }
        previous_dts = Some(dts);
    }
    Ok(())
}

fn index_cuts(
    video: &[Packet],
    file: &mut File,
    codec: VideoCodec,
    length_bytes: usize,
    cancel: &AtomicBool,
) -> Result<(BTreeMap<i64, usize>, i64)> {
    let mut suffix_min = vec![i64::MAX; video.len() + 1];
    for (index, packet) in video.iter().enumerate().rev() {
        suffix_min[index] = suffix_min[index + 1].min(packet.pts.unwrap());
    }
    let mut prefix_max = None;
    let mut cuts = BTreeMap::new();
    let mut last_frame = &video[0];
    for (index, packet) in video.iter().enumerate() {
        let pts = packet.pts.unwrap();
        // A decode-order boundary must also partition presentation order.
        if packet.is_key()
            && packet.usable()
            && prefix_max.is_none_or(|max| max < pts)
            && suffix_min[index] == pts
        {
            process::check(cancel)?;
            if is_idr(file, packet, codec, length_bytes)? {
                cuts.insert(pts, index);
            }
        }
        if prefix_max.is_none_or(|max| pts > max) {
            prefix_max = Some(pts);
            last_frame = packet;
        }
    }
    ensure!(!cuts.is_empty(), "source has no closed IDR cut points");
    let duration = last_frame
        .duration
        .filter(|d| *d > 0)
        .context("final video frame has no duration")?;
    let end = last_frame
        .pts
        .unwrap()
        .checked_add(duration)
        .context("video end overflows")?;
    Ok((cuts, end))
}

fn decode_extradata(hex_dump: &str) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    for line in hex_dump.lines().filter(|line| !line.is_empty()) {
        let (_, columns) = line.split_once(": ").context("invalid ffprobe hex dump")?;
        let (hex, _) = columns
            .split_once("  ")
            .context("invalid ffprobe hex columns")?;
        for group in hex.split_whitespace() {
            for digit_pair in group.as_bytes().chunks(2) {
                let digits = std::str::from_utf8(digit_pair)?;
                ensure!(digits.len() == 2, "invalid hex byte");
                data.push(u8::from_str_radix(digits, 16)?);
            }
        }
    }
    Ok(data)
}

fn is_idr(
    file: &mut File,
    packet: &Packet,
    codec: VideoCodec,
    length_bytes: usize,
) -> Result<bool> {
    let mut position: u64 = packet
        .pos
        .as_deref()
        .context("key packet has no file position")?
        .parse()?;
    let mut remaining: u64 = packet
        .size
        .as_deref()
        .context("key packet has no size")?
        .parse()?;
    while remaining > 0 {
        let mut length = [0; 4];
        let prefix = length_bytes as u64;
        let header = match codec {
            VideoCodec::H264 => 1,
            VideoCodec::Hevc => 2,
        };
        ensure!(remaining >= prefix + header, "truncated video NAL header");
        file.seek(SeekFrom::Start(position))?;
        file.read_exact(&mut length[4 - length_bytes..])?;
        let length = u64::from(u32::from_be_bytes(length));
        ensure!(
            length >= header && length <= remaining - prefix,
            "invalid video NAL length"
        );
        let mut first = [0];
        file.read_exact(&mut first)?;
        match codec {
            VideoCodec::H264 => match first[0] & 0x1f {
                5 => return Ok(true),
                1..=4 | 19..=21 => return Ok(false),
                _ => {}
            },
            VideoCodec::Hevc => match (first[0] >> 1) & 0x3f {
                19 | 20 => return Ok(true),
                0..=31 => return Ok(false),
                _ => {}
            },
        }
        remaining -= prefix + length;
        position = position
            .checked_add(prefix + length)
            .context("video NAL position overflows")?;
    }
    Ok(false)
}

pub(crate) fn read_packets(path: &Path, format: &str, cancel: &AtomicBool) -> Result<Vec<Packet>> {
    let Packets { packets } = probe(path, format, &["-show_packets"], PACKET_FIELDS, cancel)?;
    Ok(packets)
}

pub(crate) fn probe<T: DeserializeOwned>(
    path: &Path,
    format: &str,
    options: &[&str],
    fields: &str,
    cancel: &AtomicBool,
) -> Result<T> {
    let mut output = tempfile::tempfile()?;
    let mut command = Command::new("ffprobe");
    command
        .args(["-v", "error", "-protocol_whitelist", "file", "-f", format])
        .args(options)
        .args(["-show_entries", fields, "-of", "json", "-i"])
        .arg(path);
    process::run(&mut command, Some(&output), cancel)?;
    output.rewind()?;
    serde_json::from_reader(BufReader::new(output)).context("reading ffprobe JSON")
}
