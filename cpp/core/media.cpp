// SPDX-License-Identifier: GPL-3.0-only
#include "detail.h"

#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>

#include <algorithm>
#include <set>

using namespace Qt::StringLiterals;

namespace spoolcut::detail {
namespace {

const auto streamFields = u"stream=index,codec_type,codec_name,codec_tag_string,profile,level,width,height,pix_fmt,field_order,color_space,color_range,color_transfer,color_primaries,chroma_location,sample_aspect_ratio,sample_fmt,sample_rate,channels,channel_layout,extradata,time_base,avg_frame_rate:stream_disposition=attached_pic:stream_side_data=side_data_type"_s;
const auto packetFields = u"packet=stream_index,pts,dts,duration,pos,size,flags:packet_side_data=side_data_type"_s;

enum class VideoCodec { H264, Hevc };

std::optional<qint64> integer(const QJsonValue &value)
{
    if (value.isUndefined() || value.isNull())
        return {};
    const auto number = value.toInteger(0);
    require(number == value.toInteger(1), u"invalid ffprobe integer"_s);
    return number;
}

QJsonArray array(const QJsonObject &object, const QString &key, bool optional = false)
{
    const auto value = object.value(key);
    require(value.isArray() || (optional && value.isUndefined()), u"invalid ffprobe %1"_s.arg(key));
    return value.toArray();
}

Stream parseStream(QJsonObject stream)
{
    const auto index = integer(stream.take(u"index"_s));
    const auto type = stream.take(u"codec_type"_s);
    const auto timeBase = stream.take(u"time_base"_s);
    const auto frameRate = stream.take(u"avg_frame_rate"_s);
    stream.remove(u"disposition"_s);
    stream.remove(u"side_data_list"_s);
    require(index && *index >= 0 && type.isString() && timeBase.isString(), u"invalid ffprobe stream"_s);
    return {*index, type.toString(), timeBase.toString(), frameRate.toString(), std::move(stream)};
}

void validateTimestamps(const std::vector<Packet> &packets, bool audio)
{
    require(!packets.empty(), u"stream has no packets"_s);
    std::optional<qint64> previousDts;
    std::set<qint64> presentationTimes;
    for (const auto &packet : packets) {
        require(packet.pts.has_value(), u"source packet has no PTS"_s);
        require(packet.dts.has_value(), u"source packet has no DTS"_s);
        require(!previousDts || *packet.dts > *previousDts, u"source DTS is not strictly increasing"_s);
        require(*packet.dts <= *packet.pts, u"source DTS is later than PTS"_s);
        if (audio)
            require(packet.pts == packet.dts, u"audio PTS differs from DTS"_s);
        else
            require(presentationTimes.insert(*packet.pts).second, u"video packets share a PTS"_s);
        previousDts = packet.dts;
    }
}

bool isIdr(QFile &file, const Packet &packet, VideoCodec codec, int lengthBytes)
{
    bool validPosition = false;
    bool validSize = false;
    auto position = packet.position.toLongLong(&validPosition);
    auto remaining = packet.size.toLongLong(&validSize);
    require(validPosition && position >= 0, u"key packet has no valid file position"_s);
    require(validSize && remaining >= 0, u"key packet has no valid size"_s);
    const auto header = codec == VideoCodec::H264 ? 1 : 2;
    while (remaining > 0) {
        require(remaining >= lengthBytes + header, u"truncated video NAL header"_s);
        require(file.seek(position), u"seeking to video NAL: %1"_s.arg(file.errorString()));
        const auto prefix = file.read(lengthBytes + 1);
        require(prefix.size() == lengthBytes + 1, u"truncated video NAL header"_s);
        qint64 length = 0;
        for (int i = 0; i < lengthBytes; ++i)
            length = (length << 8) | static_cast<unsigned char>(prefix[i]);
        require(length >= header && length <= remaining - lengthBytes, u"invalid video NAL length"_s);
        const auto byte = static_cast<unsigned char>(prefix[lengthBytes]);
        if (codec == VideoCodec::H264) {
            const auto type = byte & 0x1f;
            if (type == 5)
                return true;
            if ((type >= 1 && type <= 4) || (type >= 19 && type <= 21))
                return false;
        } else {
            const auto type = (byte >> 1) & 0x3f;
            if (type <= 31)
                return type == 19 || type == 20;
        }
        const auto consumed = lengthBytes + length;
        remaining -= consumed;
        position = checkedTicks(Wide{position} + consumed, u"video NAL position overflows"_s);
    }
    return false;
}

std::pair<std::map<qint64, qsizetype>, qint64> indexCuts(
    const std::vector<Packet> &packets, QFile &file, VideoCodec codec, int lengthBytes, const Cancel &cancel)
{
    std::vector<qint64> suffixMin(packets.size() + 1, std::numeric_limits<qint64>::max());
    for (auto index = packets.size(); index > 0; --index)
        suffixMin[index - 1] = std::min(suffixMin[index], *packets[index - 1].pts);

    std::map<qint64, qsizetype> cuts;
    const Packet *lastFrame = &packets.front();
    std::optional<qint64> prefixMax;
    for (size_t index = 0; index < packets.size(); ++index) {
        const auto &packet = packets[index];
        const auto pts = *packet.pts;
        // A closed IDR must partition both decode and presentation order.
        if (packet.key && packet.usable && (!prefixMax || *prefixMax < pts) && suffixMin[index] == pts) {
            checkCancelled(cancel);
            if (isIdr(file, packet, codec, lengthBytes))
                cuts.emplace(pts, static_cast<qsizetype>(index));
        }
        if (!prefixMax || pts > *prefixMax) {
            prefixMax = pts;
            lastFrame = &packet;
        }
    }
    require(!cuts.empty(), u"source has no closed IDR cut points"_s);
    require(lastFrame->duration && *lastFrame->duration > 0, u"final video frame has no duration"_s);
    const auto end = checkedTicks(Wide{*lastFrame->pts} + *lastFrame->duration, u"video end overflows"_s);
    return {std::move(cuts), end};
}

int hexDigit(char c)
{
    if (c >= '0' && c <= '9')
        return c - '0';
    if (c >= 'a' && c <= 'f')
        return c - 'a' + 10;
    if (c >= 'A' && c <= 'F')
        return c - 'A' + 10;
    return -1;
}

} // namespace

QString Stream::parameter(const QString &name) const
{
    const auto value = parameters.value(name);
    require(value.isString(), u"stream %1 has no %2"_s.arg(index).arg(name));
    return value.toString();
}

QJsonObject probe(const QString &path, const QString &format, const QStringList &options,
                  const QString &fields, const Cancel &cancel)
{
    QStringList args{u"-v"_s, u"error"_s, u"-protocol_whitelist"_s, u"file"_s, u"-f"_s, format};
    args += options;
    args << u"-show_entries"_s << fields << u"-of"_s << u"json"_s << u"-i"_s << mediaPath(path);
    const auto bytes = runTool(u"ffprobe"_s, args, cancel, true);
    QJsonParseError error{};
    const auto json = QJsonDocument::fromJson(bytes, &error);
    require(error.error == QJsonParseError::NoError && json.isObject(), u"reading ffprobe JSON: %1"_s.arg(error.errorString()));
    return json.object();
}

std::vector<Packet> readPackets(const QString &path, const QString &format, const Cancel &cancel)
{
    const auto probed = array(probe(path, format, {u"-show_packets"_s}, packetFields, cancel), u"packets"_s, true);
    std::vector<Packet> packets;
    packets.reserve(static_cast<size_t>(probed.size()));
    for (const auto &value : probed) {
        const auto packet = value.toObject();
        const auto index = integer(packet.value(u"stream_index"_s));
        const auto flags = packet.value(u"flags"_s);
        require(index && *index >= 0 && flags.isString(), u"invalid ffprobe packet"_s);
        const auto flagText = flags.toString();
        packets.push_back({*index,
                           integer(packet.value(u"pts"_s)), integer(packet.value(u"dts"_s)),
                           integer(packet.value(u"duration"_s)), packet.value(u"pos"_s).toString(),
                           packet.value(u"size"_s).toString(), flagText.contains(QLatin1Char('K')),
                           !flagText.contains(QLatin1Char('C')) && !flagText.contains(QLatin1Char('D')) &&
                               array(packet, u"side_data_list"_s, true).isEmpty()});
    }
    return packets;
}

QByteArray decodeExtradata(const QString &dump)
{
    QByteArray data;
    for (const auto &line : dump.toLatin1().split('\n')) {
        if (line.isEmpty())
            continue;
        const auto start = line.indexOf(": ");
        require(start >= 0, u"invalid ffprobe hex dump"_s);
        const auto columns = line.indexOf("  ", start + 2);
        require(columns >= 0, u"invalid ffprobe hex columns"_s);
        for (const auto &group : line.mid(start + 2, columns - start - 2).split(' ')) {
            require(group.size() % 2 == 0, u"invalid hex byte"_s);
            for (qsizetype i = 0; i < group.size(); i += 2) {
                const auto high = hexDigit(group[i]);
                const auto low = hexDigit(group[i + 1]);
                require(high >= 0 && low >= 0, u"invalid hex byte"_s);
                data.append(static_cast<char>((high << 4) | low));
            }
        }
    }
    return data;
}

Media Media::open(const QString &input, const Cancel &cancel)
{
    checkCancelled(cancel);
    const auto path = canonicalPath(input);
    const auto sourcePath = absolutePath(input);
    require(QFileInfo(path).isFile(), u"source must be a regular file"_s);
    QFile file(path);
    const bool opened = file.open(QIODevice::ReadOnly);
    require(opened, file.errorString());
    const auto identity = SourceIdentity::read(file);

    const auto streams = array(probe(path, u"mov"_s, {u"-show_data"_s, u"-show_streams"_s},
                                     streamFields, cancel), u"streams"_s);
    std::optional<Stream> video;
    std::optional<Stream> audio;
    for (const auto &value : streams) {
        const auto stream = value.toObject();
        const auto type = stream.value(u"codec_type"_s).toString();
        const auto pic = stream.value(u"disposition"_s).toObject().value(u"attached_pic"_s).toInt();
        if (type != u"audio"_s && (type != u"video"_s || pic != 0))
            continue;
        auto &selected = type == u"audio"_s ? audio : video;
        require(!selected, u"source has multiple %1 streams"_s.arg(type));
        require(array(stream, u"side_data_list"_s, true).isEmpty(),
                u"stream side data such as a rotation matrix is not supported"_s);
        selected = parseStream(stream);
    }
    require(video.has_value(), u"missing video stream"_s);
    require(audio.has_value(), u"missing audio stream"_s);

    const auto videoCodec = video->parameter(u"codec_name"_s);
    const auto tag = video->parameter(u"codec_tag_string"_s);
    const bool isAvc = videoCodec == u"h264"_s && tag == u"avc1"_s;
    require(isAvc || (videoCodec == u"hevc"_s && tag == u"hvc1"_s), u"expected H.264 (avc1) or HEVC (hvc1) video"_s);
    const auto codec = isAvc ? VideoCodec::H264 : VideoCodec::Hevc;
    require(audio->parameter(u"codec_name"_s) == u"aac"_s && audio->parameter(u"profile"_s) == u"LC"_s,
            u"expected AAC-LC audio"_s);
    const auto videoBase = parseTimeBase(video->timeBase);
    const auto audioBase = parseTimeBase(audio->timeBase);
    bool validRate = false;
    const auto rate = audio->parameter(u"sample_rate"_s).toInt(&validRate);
    const auto frameNumerator = qint64{1024} * audioBase.denominator;
    const auto frameDenominator = qint64{rate} * audioBase.numerator;
    require(validRate && frameDenominator > 0 && frameNumerator % frameDenominator == 0,
            u"audio time base does not represent a whole AAC frame"_s);

    std::vector<Packet> videoPackets;
    std::vector<Packet> audioPackets;
    for (auto &packet : readPackets(path, u"mov"_s, cancel)) {
        if (packet.streamIndex == video->index)
            videoPackets.push_back(std::move(packet));
        else if (packet.streamIndex == audio->index)
            audioPackets.push_back(std::move(packet));
    }
    validateTimestamps(videoPackets, false);
    validateTimestamps(audioPackets, true);

    const auto config = decodeExtradata(video->parameter(u"extradata"_s));
    const auto lengthOffset = isAvc ? 4 : 21;
    require(!config.isEmpty() && config.front() == 1, u"invalid video decoder configuration"_s);
    require(config.size() > lengthOffset, u"missing NAL length size"_s);
    const auto lengthBytes = (config[lengthOffset] & 3) + 1;
    auto [cuts, end] = indexCuts(videoPackets, file, codec, lengthBytes, cancel);
    Media media{path, sourcePath, identity, std::move(*video), std::move(*audio), videoBase, audioBase,
                frameNumerator / frameDenominator, std::move(videoPackets), std::move(audioPackets),
                std::move(cuts), end};
    media.checkSource();
    return media;
}

void Media::checkSource() const
{
    context(u"checking source %1"_s.arg(sourcePath), [&] {
        require(QFileInfo(sourcePath).isFile(), u"source is no longer a regular file"_s);
        QFile file(sourcePath);
        const bool opened = file.open(QIODevice::ReadOnly);
        require(opened, file.errorString());
        require(SourceIdentity::read(file) == identity, u"source changed or was replaced during operation"_s);
    });
}

MediaInfo Media::info() const
{
    std::vector<qint64> safe;
    for (const auto &[pts, index] : cuts)
        safe.push_back(pts);
    std::vector<qint64> keyframes;
    for (const auto &packet : videoPackets) {
        if (packet.key && packet.usable)
            keyframes.push_back(*packet.pts);
    }
    std::sort(keyframes.begin(), keyframes.end());
    std::optional<TimeBase> rate;
    if (!video.averageFrameRate.isEmpty()) {
        try {
            rate = parseTimeBase(video.averageFrameRate);
        } catch (const Error &) {
            // Frame rate is informational and ffprobe can report 0/0.
        }
    }
    return {video.parameter(u"codec_name"_s), audio.parameter(u"codec_name"_s), videoBase,
            std::move(safe), videoEndPts,
            {video.parameters.value(u"width"_s).toInt(), video.parameters.value(u"height"_s).toInt()},
            rate, std::move(keyframes)};
}

} // namespace spoolcut::detail

namespace spoolcut {

MediaInfo inspect(const QString &path, const Cancel &cancel)
{
    return detail::context(u"inspecting %1"_s.arg(path), [&] { return detail::Media::open(path, cancel).info(); });
}

struct IndexedSource::Data {
    detail::Media media;
    MediaInfo info;
    std::vector<qint64> framePts;
};

IndexedSource IndexedSource::open(const QString &path, const Cancel &cancel)
{
    auto media = detail::context(u"inspecting %1"_s.arg(path), [&] { return detail::Media::open(path, cancel); });
    std::vector<qint64> points;
    points.reserve(media.videoPackets.size());
    for (const auto &packet : media.videoPackets)
        points.push_back(*packet.pts);
    std::sort(points.begin(), points.end());
    auto info = media.info();
    return IndexedSource(std::make_shared<Data>(Data{std::move(media), std::move(info), std::move(points)}));
}

const MediaInfo &IndexedSource::info() const { return data_->info; }
PlannedClip IndexedSource::planClip(const Clip &clip) const { return detail::planClip(data_->media, clip, 0); }
void IndexedSource::checkSource() const { data_->media.checkSource(); }

double IndexedSource::frameAt(const Clip &clip, double sourceSeconds) const
{
    const auto base = data_->media.videoBase;
    const auto first = data_->framePts.begin();
    const auto index = std::partition_point(first, data_->framePts.end(), [&](qint64 pts) {
        return pts < clip.outPts && base.seconds(pts) <= sourceSeconds;
    });
    const auto pts = index == first ? clip.inPts : std::max(clip.inPts, *std::prev(index));
    return base.seconds(pts);
}

} // namespace spoolcut
