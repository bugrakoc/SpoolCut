// SPDX-License-Identifier: GPL-3.0-only
#include "detail.h"

#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QTemporaryDir>

#include <algorithm>
#include <span>

using namespace Qt::StringLiterals;

namespace spoolcut {
namespace {

using namespace detail;

constexpr TimeBase mkvTimeBase{1, 1000};
constexpr auto stagingVideoTimescale = 1'000'000;

const QStringList ffmpegOptions{u"-nostdin"_s, u"-hide_banner"_s, u"-loglevel"_s, u"warning"_s,
                               u"-xerror"_s, u"-n"_s};

void copyOutput(QStringList &args, const QString &format, const QString &path)
{
    args << u"-c"_s << u"copy"_s << u"-map_metadata"_s << u"-1"_s << u"-map_chapters"_s << u"-1"_s
         << u"-avoid_negative_ts"_s << u"disabled"_s << u"-f"_s << format << mediaPath(path);
}

QString selectPackets(const PacketRange &range)
{
    return u"noise=drop=lt(n\\,%1)+gte(n\\,%2)"_s.arg(range.start).arg(range.end);
}

void writeClip(const Media &media, const PlannedClip &clip, const QString &output, const Cancel &cancel)
{
    const auto offset = micros(media.videoBase, -Wide{clip.clip.inPts});
    auto args = ffmpegOptions;
    // Select by demuxed packet ordinal. Timestamp seeking can choose other packets.
    args << u"-copyts"_s << u"-itsoffset"_s << ffmpegTime(offset)
         << u"-protocol_whitelist"_s << u"file"_s << u"-f"_s << u"mov"_s << u"-i"_s << mediaPath(media.path)
         << u"-map"_s << u"0:%1"_s.arg(media.video.index) << u"-map"_s << u"0:%1"_s.arg(media.audio.index)
         << u"-bsf:v"_s << selectPackets(clip.videoPackets) << u"-bsf:a"_s << selectPackets(clip.audioPackets);

    // Fragmentation preserves AAC timing without source priming or inferred
    // padding. A shared video timescale allows concat across source time bases.
    args << u"-movflags"_s << u"+frag_keyframe+delay_moov+default_base_moof"_s << u"-use_editlist"_s << u"1"_s
         << u"-movie_timescale"_s << u"1000000"_s << u"-video_track_timescale"_s << QString::number(stagingVideoTimescale);
    copyOutput(args, u"mp4"_s, output);
    runTool(u"ffmpeg"_s, args, cancel);
}

struct HevcConfiguration {
    int nalLengthSize;
    std::vector<std::pair<int, QByteArray>> nals;
    bool operator==(const HevcConfiguration &) const = default;
};

HevcConfiguration hevcConfiguration(const QByteArray &data)
{
    require(data.size() >= 23, u"incomplete HEVC header"_s);
    require(data[0] == 1, u"invalid HEVC configuration version"_s);
    const auto byte = [&](qsizetype pos) { return static_cast<unsigned char>(data[pos]); };
    const auto word = [&](qsizetype pos) { return (byte(pos) << 8) | byte(pos + 1); };
    HevcConfiguration result{(byte(21) & 3) + 1, {}};
    qsizetype pos = 23;
    for (int array = 0; array < byte(22); ++array) {
        require(data.size() - pos >= 3, u"incomplete HEVC array"_s);
        const int type = byte(pos) & 0x3f;
        const int count = word(pos + 1);
        pos += 3;
        for (int nal = 0; nal < count; ++nal) {
            require(data.size() - pos >= 2, u"missing HEVC NAL length"_s);
            const auto length = word(pos);
            pos += 2;
            require(data.size() - pos >= length, u"incomplete HEVC NAL"_s);
            result.nals.emplace_back(type, data.mid(pos, length));
            pos += length;
        }
    }
    require(pos == data.size(), u"unexpected data after HEVC arrays"_s);
    // Matroska can reorder hvcC arrays or normalize their completeness flags.
    std::sort(result.nals.begin(), result.nals.end());
    return result;
}

void verifyPackets(const std::vector<Packet> &source, PacketRange range, TimeBase base,
                   qint64 sourceStart, qint64 timelineStart, const std::vector<const Packet *> &output,
                   size_t &offset, const QString &label)
{
    const auto selected = std::span(source).subspan(static_cast<size_t>(range.start),
                                                     static_cast<size_t>(range.end - range.start));
    for (const auto &packet : selected) {
        require(offset < output.size(), u"missing %1 packet"_s.arg(label));
        const auto &actual = *output[offset++];
        require(actual.pts.has_value(), u"output packet has no PTS"_s);
        const auto expected = Wide{micros(base, *packet.pts)} - sourceStart + timelineStart;
        const auto actualUs = micros(mkvTimeBase, *actual.pts);
        const auto delta = expected - actualUs;
        // The final muxer rounds to millisecond ticks.
        if (delta < -1001 || delta > 1001)
            throw Error(u"%1 timestamp changed (expected %2 µs, got %3 µs)"_s.arg(label)
                .arg(checkedTicks(expected, u"expected output timestamp overflows"_s)).arg(actualUs));
        require(actual.usable && actual.size == packet.size && actual.key == packet.key,
                u"%1 packet framing changed"_s.arg(label));
    }
}

QString verifyStreams(const QString &path, const Prepared &prepared, const Cancel &cancel)
{
    const auto info = probe(path, u"matroska"_s, {u"-show_format"_s, u"-show_streams"_s, u"-show_data"_s},
                            u"format=duration:stream=codec_type,codec_name,extradata:stream_disposition=default"_s, cancel);
    const auto streams = info.value(u"streams"_s).toArray();
    require(streams.size() == 2, u"expected one video and one audio stream"_s);
    const auto &reference = prepared.sources.at(prepared.plan.clips.front().clip.source);
    const std::array<const Stream *, 2> expected{&reference.video, &reference.audio};
    for (size_t i = 0; i < expected.size(); ++i) {
        const auto actual = streams[static_cast<qsizetype>(i)].toObject();
        const auto &source = *expected[i];
        const auto codecName = actual.value(u"codec_name"_s).toString();
        require(actual.value(u"codec_type"_s).toString() == source.type && codecName == source.parameter(u"codec_name"_s),
                u"output %1 codec changed"_s.arg(source.type));
        require(actual.value(u"disposition"_s).toObject().value(u"default"_s).toInt() == 1,
                u"output %1 track is not default"_s.arg(source.type));
        const auto outputConfig = actual.value(u"extradata"_s);
        require(outputConfig.isString(), u"output %1 decoder configuration is missing"_s.arg(source.type));
        const auto sourceConfig = source.parameter(u"extradata"_s);
        const bool matches = codecName == u"hevc"_s
            ? hevcConfiguration(decodeExtradata(outputConfig.toString())) == hevcConfiguration(decodeExtradata(sourceConfig))
            : outputConfig.toString() == sourceConfig;
        require(matches, u"output %1 decoder configuration changed"_s.arg(source.type));
    }

    const auto duration = info.value(u"format"_s).toObject().value(u"duration"_s).toString();
    const auto parts = duration.split(QLatin1Char('.'));
    require(parts.size() == 2 && parts[1].size() == 6, u"invalid output duration precision"_s);
    bool secondsOk = false;
    bool fractionOk = false;
    const auto seconds = parts[0].toLongLong(&secondsOk);
    const auto fraction = parts[1].toUInt(&fractionOk);
    require(secondsOk && fractionOk && seconds >= 0, u"invalid output duration"_s);
    const auto actual = Wide{seconds} * 1'000'000 + fraction;
    const auto difference = actual - prepared.plan.durationUs;
    if (difference < -2000 || difference > 2000)
        return u"output duration is %1 s, expected %2 µs"_s.arg(duration).arg(prepared.plan.durationUs);
    return {};
}

QString verify(const QString &path, const Prepared &prepared, const Cancel &cancel)
{
    const auto packets = readPackets(path, u"matroska"_s, cancel);
    std::array<std::vector<const Packet *>, 2> streams;
    for (const auto &packet : packets) {
        require(packet.streamIndex < 2, u"unexpected output stream"_s);
        streams[static_cast<size_t>(packet.streamIndex)].push_back(&packet);
    }
    size_t videoOffset = 0;
    size_t audioOffset = 0;
    const auto &clips = prepared.plan.clips;
    for (size_t i = 0; i < clips.size(); ++i) {
        checkCancelled(cancel);
        const auto &clip = clips[i];
        const auto &source = prepared.sources.at(clip.clip.source);
        const auto start = micros(source.videoBase, clip.clip.inPts);
        context(u"clip %1"_s.arg(i + 1), [&] {
            verifyPackets(source.videoPackets, clip.videoPackets, source.videoBase, start,
                          clip.timelineStartUs, streams[0], videoOffset, u"video"_s);
            verifyPackets(source.audioPackets, clip.audioPackets, source.audioBase, start,
                          clip.timelineStartUs, streams[1], audioOffset, u"audio"_s);
        });
    }
    require(videoOffset == streams[0].size() && audioOffset == streams[1].size(),
            u"unexpected extra output packets"_s);
    return verifyStreams(path, prepared, cancel);
}

QString finalVideoDuration(const Prepared &prepared)
{
    const auto &clips = prepared.plan.clips;
    const auto &lastClip = clips.back();
    const auto &packets = prepared.sources.at(lastClip.clip.source).videoPackets;
    const auto begin = packets.begin() + lastClip.videoPackets.start;
    const auto end = packets.begin() + lastClip.videoPackets.end;
    const auto lastFrame = std::max_element(begin, end, [](const Packet &left, const Packet &right) {
        return left.pts < right.pts;
    });
    Wide lastOrdinal = lastFrame - begin;
    for (auto clip = clips.begin(); clip != std::prev(clips.end()); ++clip)
        lastOrdinal += clip->videoPackets.end - clip->videoPackets.start;

    // Fragmented MOV probing can infer a nominal duration for the tail.
    // The planner knows the last presented frame and the exclusive end.
    // Keep the staged microsecond PTS/DTS and payloads. Correct the final
    // frame's duration and cap any inferred video duration at the same end.
    return u"setts=pts=PTS:dts=DTS:duration='if(eq(N,%1),max(0,%2-PTS),min(DURATION,max(0,%2-PTS)))'"_s
        .arg(checkedTicks(lastOrdinal, u"video packet ordinal overflows"_s))
        .arg(prepared.plan.durationUs);
}

void appendClip(QFile &manifest, const QString &name, const QString &path, qint64 durationUs)
{
#ifdef Q_OS_WIN
    Q_UNUSED(name);
    // Do not give CreateProcess the potentially long staging directory as cwd.
    auto escaped = mediaPath(path);
    escaped.replace(QLatin1Char('\''), u"'\\''"_s);
    const auto file = u"file 'file:%1'\n"_s.arg(escaped);
#else
    Q_UNUSED(path);
    const auto file = u"file %1\n"_s.arg(name);
#endif
    const auto line = (file + u"duration %1\n"_s.arg(ffmpegTime(durationUs))).toUtf8();
    require(manifest.write(line) == line.size(), u"writing concat manifest"_s);
}

} // namespace

void exportProject(const Project &project, const QString &root, const QString &output,
                   const Cancel &cancel, const std::function<void(ExportProgress)> &progress)
{
    const auto notify = [&](ExportProgress update) {
        if (progress)
            progress(update);
    };
    const QFileInfo outputInfo(output);
    require(!output.contains(QChar::Null) && outputInfo.suffix().compare(u"mkv"_s, Qt::CaseInsensitive) == 0,
            u"output must have a .mkv extension"_s);
    require(!outputInfo.exists() && !outputInfo.isSymbolicLink(), u"output already exists: %1"_s.arg(output));
    notify({ExportStage::Planning});
    const auto prepared = prepare(project, root, cancel);
    const auto parent = canonicalPath(projectDirectory(output));
    const auto destination = QDir(parent).filePath(outputInfo.fileName());
    const QTemporaryDir workspace(QDir(parent).filePath(u".spoolcut-XXXXXX"_s));
    require(workspace.isValid(), u"creating temporary files in %1: %2"_s.arg(parent, workspace.errorString()));
    const auto directory = canonicalPath(workspace.path());

    const auto manifestName = u"timeline.ffconcat"_s;
    const auto manifestPath = QDir(directory).filePath(manifestName);
    QFile manifest(manifestPath);
    const bool opened = manifest.open(QIODevice::WriteOnly | QIODevice::NewOnly);
    require(opened, manifest.errorString());
    constexpr QByteArrayView header("ffconcat version 1.0\n");
    require(manifest.write(header.data(), header.size()) == header.size(), u"writing concat manifest"_s);
    for (size_t i = 0; i < prepared.plan.clips.size(); ++i) {
        notify({ExportStage::Copying, i, prepared.plan.clips.size()});
        checkCancelled(cancel);
        const auto &clip = prepared.plan.clips[i];
        const auto &source = prepared.sources.at(clip.clip.source);
        const auto name = u"%1.mp4"_s.arg(i);
        const auto path = QDir(directory).filePath(name);
        source.checkSource();
        context(u"copying clip %1 (%2)"_s.arg(i + 1).arg(clip.clip.source), [&] {
            writeClip(source, clip, path, cancel);
        });
        appendClip(manifest, name, path, clip.durationUs);
    }
    const bool flushed = manifest.flush();
    require(flushed, manifest.errorString());
    manifest.close();

    const auto staged = QDir(directory).filePath(u"output.mkv"_s);
    auto args = ffmpegOptions;
    args << u"-protocol_whitelist"_s << u"file"_s << u"-f"_s << u"concat"_s << u"-safe"_s;
#ifdef Q_OS_WIN
    const QString workingDirectory;
    args << u"0"_s << u"-auto_convert"_s << u"0"_s << u"-i"_s << mediaPath(manifestPath);
#else
    const auto &workingDirectory = directory;
    args << u"1"_s << u"-auto_convert"_s << u"0"_s << u"-i"_s << manifestName;
#endif
    args << u"-map"_s << u"0:v:0"_s << u"-map"_s << u"0:a:0"_s
         << u"-disposition:v:0"_s << u"default"_s << u"-disposition:a:0"_s << u"default"_s;
    const auto joinAndVerify = [&](QStringList options) {
        copyOutput(options, u"matroska"_s, staged);
        notify({ExportStage::Joining});
        context(u"joining copied clips"_s, [&] { runTool(u"ffmpeg"_s, options, cancel, false, workingDirectory); });
        notify({ExportStage::Verifying});
        return context(u"verifying joined output"_s, [&] { return verify(staged, prepared, cancel); });
    };

    if (const auto durationError = joinAndVerify(args); !durationError.isEmpty()) {
        checkCancelled(cancel);
        QFile previous(staged);
        require(previous.remove(), u"replacing the staged output: %1"_s.arg(previous.errorString()));
        // Packet and stream verification succeeded. Reuse the copied clips
        // and correct the duration at muxing, then verify the new file in full.
        args << u"-bsf:v"_s << finalVideoDuration(prepared);
        const auto correctedDurationError = joinAndVerify(std::move(args));
        context(u"verifying joined output"_s, [&] {
            require(correctedDurationError.isEmpty(), correctedDurationError);
        });
    }
    for (const auto &[id, source] : prepared.sources)
        source.checkSource();
    checkCancelled(cancel);
    context(u"publishing %1"_s.arg(output), [&] { publish(staged, destination); });
}

} // namespace spoolcut
