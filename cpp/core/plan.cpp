// SPDX-License-Identifier: GPL-3.0-only
#include "detail.h"

#include <QDir>

#include <algorithm>

using namespace Qt::StringLiterals;

namespace spoolcut::detail {
namespace {

void checkCompatibility(const Stream &reference, const Stream &other)
{
    // Different source time bases do not change the decoder or presentation parameters.
    auto fields = reference.parameters.keys();
    fields += other.parameters.keys();
    for (const auto &field : fields) {
        if (reference.parameters.value(field) != other.parameters.value(field)) {
            const auto name = field == u"extradata"_s ? u"decoder configuration"_s : field;
            throw Error(u"%1 streams differ in %2"_s.arg(other.type, name));
        }
    }
}

PacketRange planAudio(const Media &media, const Clip &clip)
{
    const auto &packets = media.audioPackets;
    const auto compareAudio = [&](Wide a, qint64 v) { return compare(media.audioBase, a, media.videoBase, v); };
    const Wide frame = media.audioFrameTicks;
    const auto start = std::partition_point(packets.begin(), packets.end(), [&](const Packet &packet) {
        return compareAudio(*packet.pts, clip.inPts) < 0;
    });
    const auto end = std::partition_point(packets.begin(), packets.end(), [&](const Packet &packet) {
        return compareAudio(Wide{*packet.pts} + frame, clip.outPts) <= 0;
    });
    require(end > start, u"clip contains no complete AAC frame"_s);
    Wide nextPts = *start->pts;
    require(compareAudio(nextPts - frame, clip.inPts) <= 0,
            u"audio starts more than one AAC frame after in_pts"_s);
    for (auto packet = start; packet != end; ++packet) {
        require(packet->usable && packet->duration == media.audioFrameTicks && *packet->pts == nextPts,
                u"selected audio contains a gap, a partial AAC frame, or packet side data"_s);
        nextPts += frame;
    }
    require(compareAudio(nextPts + frame, clip.outPts) >= 0,
            u"audio ends more than one AAC frame before out_pts"_s);
    return {start - packets.begin(), end - packets.begin()};
}

} // namespace

PlannedClip planClip(const Media &media, const Clip &clip, qint64 timelineStart)
{
    require(clip.inPts < clip.outPts, u"in_pts must be less than out_pts"_s);
    const auto begin = media.cuts.find(clip.inPts);
    require(begin != media.cuts.end(),
            u"in_pts %1 is not a safe IDR boundary. Run inspect for available points"_s.arg(clip.inPts));
    const auto start = begin->second;
    qsizetype end = static_cast<qsizetype>(media.videoPackets.size());
    if (clip.outPts != media.videoEndPts) {
        const auto boundary = media.cuts.find(clip.outPts);
        require(boundary != media.cuts.end(),
                u"out_pts %1 is not a safe IDR boundary or video end. Run inspect for available points"_s.arg(clip.outPts));
        end = boundary->second;
    }

    require(start < end, u"empty video range"_s);
    const auto first = media.videoPackets.begin() + start;
    const auto last = media.videoPackets.begin() + end;
    require(std::all_of(first, last, [](const Packet &packet) { return packet.usable; }),
            u"selected video contains discard flags, corrupt packets, or packet side data"_s);
    const auto finalFrame = std::max_element(first, last, [](const Packet &a, const Packet &b) {
        return a.pts < b.pts;
    });
    require(Wide{*finalFrame->pts} + finalFrame->duration.value_or(0) == clip.outPts,
            u"the final frame does not end at out_pts"_s);
    const auto audio = planAudio(media, clip);
    const auto duration = micros(media.videoBase, Wide{clip.outPts} - clip.inPts);
    require(duration > 0, u"clip is shorter than one microsecond"_s);
    return {clip, timelineStart, duration, {start, end}, audio};
}

Prepared prepare(const Project &project, const QString &root, const Cancel &cancel)
{
    require(!project.clips.empty(), u"project has no clips"_s);
    Prepared result{{{}, 0}, {}};
    result.plan.clips.reserve(project.clips.size());
    for (const auto &clip : project.clips) {
        checkCancelled(cancel);
        const auto planned = context(u"clip %1 (%2)"_s.arg(result.plan.clips.size() + 1).arg(clip.source), [&] {
            auto source = result.sources.find(clip.source);
            if (source == result.sources.end()) {
                const auto location = project.sources.find(clip.source);
                require(location != project.sources.end(), u"unknown source"_s);
                const auto path = QDir(root).filePath(location->second);
                auto media = context(u"inspecting %1"_s.arg(path), [&] { return Media::open(path, cancel); });
                if (!result.plan.clips.empty()) {
                    const auto &reference = result.sources.at(result.plan.clips.front().clip.source);
                    checkCompatibility(reference.video, media.video);
                    checkCompatibility(reference.audio, media.audio);
                }
                source = result.sources.emplace(clip.source, std::move(media)).first;
            }
            return planClip(source->second, clip, result.plan.durationUs);
        });
        result.plan.durationUs = checkedTicks(Wide{result.plan.durationUs} + planned.durationUs,
                                              u"timeline duration overflows"_s);
        result.plan.clips.push_back(planned);
    }
    return result;
}

} // namespace spoolcut::detail

namespace spoolcut {

ExportPlan plan(const Project &project, const QString &root, const Cancel &cancel)
{
    return detail::prepare(project, root, cancel).plan;
}

} // namespace spoolcut
