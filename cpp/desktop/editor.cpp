// SPDX-License-Identifier: GPL-3.0-only
#include "editor.h"

#include <QFileInfo>

#include <algorithm>
#include <cmath>
#include <limits>

using namespace Qt::StringLiterals;

namespace spoolcut {
namespace {

QString sourceId(Project &project, const QString &path)
{
    for (const auto &[id, file] : project.sources) {
        if (file == path)
            return id;
    }
    size_t number = 1;
    auto id = u"source-1"_s;
    while (project.sources.contains(id))
        id = u"source-%1"_s.arg(++number);
    project.sources.emplace(id, path);
    return id;
}

bool validRange(const IndexedSource &source, const Clip &clip)
{
    try {
        (void)source.planClip(clip);
        return true;
    } catch (const Error &) {
        return false;
    }
}

void sortByDistance(std::vector<qint64> &points, TimeBase base, double target)
{
    std::stable_sort(points.begin(), points.end(), [&](qint64 a, qint64 b) {
        return std::abs(base.seconds(a) - target) < std::abs(base.seconds(b) - target);
    });
}

} // namespace

void Editor::open(const QString &path)
{
    auto project = Project::load(path);
    const auto root = projectDirectory(path);
    for (auto &[id, source] : project.sources)
        source = resolveProjectSource(root, source);
    *this = Editor{};
    project_ = std::move(project);
    saved_ = project_;
    path_ = path;
}

void Editor::save(const QString &path)
{
    auto portable = project_;
    const auto root = projectDirectory(path);
    for (auto &[id, source] : portable.sources)
        source = projectSourcePath(root, source);
    portable.save(path);
    saved_ = project_;
    path_ = path;
}

void Editor::setIndex(const QString &path, SourceResult result)
{
    media_.insert_or_assign(path, std::move(result));
}

const SourceResult *Editor::indexResult(const QString &path) const
{
    const auto item = media_.find(path);
    return item == media_.end() ? nullptr : &item->second;
}

const IndexedSource &Editor::index(const QString &id) const
{
    const auto path = project_.sources.find(id);
    if (path == project_.sources.end())
        throw Error(u"Unknown source %1"_s.arg(id));
    if (const auto *result = indexResult(path->second)) {
        if (const auto *source = std::get_if<IndexedSource>(result))
            return *source;
        throw Error(std::get<QString>(*result));
    }
    throw Error(u"%1 is being inspected"_s.arg(QFileInfo(path->second).fileName()));
}

void Editor::change(Project next)
{
    if (next == project_)
        return;
    undo_.push_back(std::move(project_));
    project_ = std::move(next);
    redo_.clear();
}

void Editor::import(const QStringList &paths)
{
    auto next = project_;
    for (const auto &path : paths)
        sourceId(next, path);
    change(std::move(next));
}

void Editor::relink(const QString &id, const QString &path, IndexedSource source)
{
    for (const auto &clip : project_.clips) {
        if (clip.source == id && !validRange(source, clip))
            throw Error(u"The replacement cannot play the existing cuts."_s);
    }
    setIndex(path, std::move(source));
    auto next = project_;
    next.sources.at(id) = path;
    change(std::move(next));
}

std::vector<Segment> Editor::segments() const
{
    std::vector<Segment> result;
    result.reserve(project_.clips.size());
    qint64 start = 0;
    for (const auto &clip : project_.clips) {
        const auto &source = index(clip.source);
        const auto planned = source.planClip(clip);
        if (planned.durationUs > std::numeric_limits<qint64>::max() - start)
            throw Error(u"Timeline is too long"_s);
        const auto end = start + planned.durationUs;
        const auto &info = source.info();
        const auto base = info.videoBase;
        Segment segment{project_.sources.at(clip.source), start, end,
                        base.seconds(clip.inPts), base.seconds(clip.outPts), {}};
        const auto first = std::lower_bound(info.safeInPts.begin(), info.safeInPts.end(), clip.inPts);
        const auto last = std::lower_bound(first, info.safeInPts.end(), clip.outPts);
        segment.safePoints.reserve(static_cast<size_t>(last - first));
        for (auto point = first; point != last; ++point) {
            // The display offset is rounded independently of the stored integer PTS.
            // Bound before conversion and leave the exclusive end out of the ruler.
            const auto offset = std::round((static_cast<long double>(*point) - clip.inPts)
                                           * base.numerator * 1'000'000 / base.denominator);
            if (offset >= 0 && offset < static_cast<long double>(planned.durationUs))
                segment.safePoints.push_back(start + static_cast<qint64>(offset));
        }
        result.push_back(std::move(segment));
        start = end;
    }
    return result;
}

size_t Editor::insertSource(const QString &id, size_t slot)
{
    const auto &source = index(id);
    const auto &info = source.info();
    if (info.safeInPts.empty())
        throw Error(u"No safe cut was found"_s);
    auto ends = info.safeInPts;
    ends.push_back(info.videoEndPts);
    for (auto point = ends.rbegin(); point != ends.rend(); ++point) {
        const Clip clip{id, info.safeInPts.front(), *point};
        if (validRange(source, clip)) {
            auto next = project_;
            slot = std::min(slot, next.clips.size());
            next.clips.insert(next.clips.begin() + static_cast<qsizetype>(slot), clip);
            change(std::move(next));
            return slot;
        }
    }
    throw Error(u"No complete stream-copy range was found from the first safe cut"_s);
}

void Editor::remove(size_t row)
{
    auto next = project_;
    next.clips.erase(next.clips.begin() + static_cast<qsizetype>(row));
    change(std::move(next));
}

size_t Editor::move(size_t row, size_t slot)
{
    auto next = project_;
    slot = std::min(slot, next.clips.size());
    const auto from = next.clips.begin() + static_cast<qsizetype>(row);
    const auto to = next.clips.begin() + static_cast<qsizetype>(slot);
    if (slot > row)
        std::rotate(from, from + 1, to);
    else
        std::rotate(to, from, from + 1);
    change(std::move(next));
    return slot > row ? slot - 1 : slot;
}

void Editor::trim(size_t row, bool in, double sourceSeconds)
{
    auto clip = project_.clips.at(row);
    const auto &source = index(clip.source);
    const auto &info = source.info();
    auto points = info.safeInPts;
    if (!in)
        points.push_back(info.videoEndPts);
    std::erase_if(points, [&](qint64 pts) { return in ? pts >= clip.outPts : pts <= clip.inPts; });
    sortByDistance(points, info.videoBase, sourceSeconds);
    for (const auto point : points) {
        (in ? clip.inPts : clip.outPts) = point;
        if (validRange(source, clip)) {
            auto next = project_;
            next.clips[row] = clip;
            change(std::move(next));
            return;
        }
    }
    throw Error(u"No safe trim point was found"_s);
}

void Editor::split(size_t row, double sourceSeconds)
{
    auto left = project_.clips.at(row);
    auto right = left;
    const auto &source = index(left.source);
    const auto &info = source.info();
    auto points = info.safeInPts;
    std::erase_if(points, [&](qint64 pts) { return pts <= left.inPts || pts >= right.outPts; });
    sortByDistance(points, info.videoBase, sourceSeconds);
    for (const auto point : points) {
        left.outPts = right.inPts = point;
        if (validRange(source, left) && validRange(source, right)) {
            auto next = project_;
            next.clips[row] = left;
            next.clips.insert(next.clips.begin() + static_cast<qsizetype>(row + 1), right);
            change(std::move(next));
            return;
        }
    }
    throw Error(u"This clip has no safe split point inside its range"_s);
}

bool Editor::copy(size_t row)
{
    const auto &clip = project_.clips.at(row);
    const auto path = project_.sources.find(clip.source);
    if (path == project_.sources.end())
        return false;
    clipboard_ = std::pair{clip, path->second};
    return true;
}

void Editor::paste(size_t slot)
{
    if (!clipboard_)
        return;
    auto [clip, path] = *clipboard_;
    auto next = project_;
    const auto source = next.sources.find(clip.source);
    if (source == next.sources.end() || source->second != path)
        clip.source = sourceId(next, path);
    slot = std::min(slot, next.clips.size());
    next.clips.insert(next.clips.begin() + static_cast<qsizetype>(slot), clip);
    change(std::move(next));
}

void Editor::undo()
{
    if (undo_.empty())
        return;
    redo_.push_back(std::move(project_));
    project_ = std::move(undo_.back());
    undo_.pop_back();
}

void Editor::redo()
{
    if (redo_.empty())
        return;
    undo_.push_back(std::move(project_));
    project_ = std::move(redo_.back());
    redo_.pop_back();
}

} // namespace spoolcut
