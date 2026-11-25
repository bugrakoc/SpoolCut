// SPDX-License-Identifier: GPL-3.0-only
#include "detail.h"

#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QScopeGuard>
#include <QTemporaryFile>

#include <charconv>

namespace spoolcut {
namespace {

// Qt JSON retains i64 values but also accepts integral floating-point tokens
// and collapses duplicate object keys. Read the strict project schema directly.
class ProjectReader {
public:
    explicit ProjectReader(QByteArray data) : data_(std::move(data)) {}

    Project read()
    {
        Project result;
        bool sources = false;
        bool clips = false;
        members([&](const QString &key) {
            if (key == QStringLiteral("sources") && !sources) {
                sources = true;
                members([&](const QString &id) { result.sources.insert_or_assign(id, string()); });
            } else if (key == QStringLiteral("clips") && !clips) {
                clips = true;
                list('[', ']', [&] { result.clips.push_back(clip()); });
            } else {
                throw Error(QStringLiteral("unknown or duplicate project field %1").arg(key));
            }
        });
        detail::require(sources && clips, QStringLiteral("project requires sources and clips"));
        whitespace();
        detail::require(pos_ == data_.size(), QStringLiteral("unexpected data after project"));
        return result;
    }

private:
    void whitespace()
    {
        while (pos_ < data_.size() && QByteArrayView(" \t\r\n").contains(data_[pos_]))
            ++pos_;
    }

    bool consume(char c)
    {
        whitespace();
        if (pos_ == data_.size() || data_[pos_] != c)
            return false;
        ++pos_;
        return true;
    }

    void expect(char c)
    {
        detail::require(consume(c), QStringLiteral("expected '%1' at byte %2").arg(QLatin1Char(c)).arg(pos_));
    }

    template<class Function>
    void list(char open, char close, Function parseItem)
    {
        expect(open);
        if (consume(close))
            return;
        do {
            parseItem();
        } while (consume(','));
        expect(close);
    }

    template<class Function>
    void members(Function parseMember)
    {
        list('{', '}', [&] {
            const auto key = string();
            expect(':');
            parseMember(key);
        });
    }

    QString string()
    {
        expect('"');
        const auto start = pos_ - 1;
        while (pos_ < data_.size()) {
            const char c = data_[pos_++];
            if (c == '\\' && pos_ < data_.size()) {
                ++pos_;
            } else if (c == '"') {
                QJsonParseError error{};
                const auto wrapped = '[' + data_.mid(start, pos_ - start) + ']';
                const auto document = QJsonDocument::fromJson(wrapped, &error);
                detail::require(error.error == QJsonParseError::NoError, error.errorString());
                const auto value = document.array()[0].toString();
                detail::require(value.isValidUtf16(), QStringLiteral("invalid Unicode in project string"));
                return value;
            }
        }
        throw Error(QStringLiteral("unterminated JSON string"));
    }

    qint64 integer()
    {
        whitespace();
        const auto start = pos_;
        if (pos_ < data_.size() && data_[pos_] == '-')
            ++pos_;
        if (pos_ < data_.size() && data_[pos_] == '0') {
            ++pos_;
        } else {
            while (pos_ < data_.size() && data_[pos_] >= '0' && data_[pos_] <= '9')
                ++pos_;
        }
        qint64 result = 0;
        const auto *end = data_.constData() + pos_;
        const auto parsed = std::from_chars(data_.constData() + start, end, result);
        const bool negativeZero = result == 0 && start < pos_ && data_[start] == '-';
        detail::require(parsed.ec == std::errc{} && parsed.ptr == end && !negativeZero,
                        QStringLiteral("expected signed 64-bit integer at byte %1").arg(start));
        return result;
    }

    Clip clip()
    {
        std::optional<QString> source;
        std::optional<qint64> in;
        std::optional<qint64> out;
        members([&](const QString &key) {
            if (key == QStringLiteral("source") && !source)
                source = string();
            else if (key == QStringLiteral("in_pts") && !in)
                in = integer();
            else if (key == QStringLiteral("out_pts") && !out)
                out = integer();
            else
                throw Error(QStringLiteral("unknown or duplicate clip field %1").arg(key));
        });
        detail::require(source && in && out, QStringLiteral("clip requires source, in_pts and out_pts"));
        return {*source, *in, *out};
    }

    QByteArray data_;
    qsizetype pos_ = 0;
};

QJsonObject clipJson(const Clip &clip)
{
    return {{QStringLiteral("source"), clip.source},
            {QStringLiteral("in_pts"), clip.inPts},
            {QStringLiteral("out_pts"), clip.outPts}};
}

} // namespace

Project Project::load(const QString &path)
{
    return detail::context(QStringLiteral("reading project %1").arg(path), [&] {
        const auto name = detail::mediaPath(path);
        QFile file(name);
        const bool opened = file.open(QIODevice::ReadOnly);
        detail::require(opened, file.errorString());
        QByteArray data = file.readAll();
        detail::require(file.error() == QFileDevice::NoError, file.errorString());
        return ProjectReader(std::move(data)).read();
    });
}

void Project::save(const QString &path) const
{
    detail::context(QStringLiteral("saving %1").arg(path), [&] {
        QJsonObject sourcePaths;
        for (const auto &[id, source] : sources)
            sourcePaths.insert(id, source);
        QJsonArray entries;
        for (const auto &clip : clips)
            entries.append(clipJson(clip));
        const auto bytes = QJsonDocument(QJsonObject{
            {QStringLiteral("sources"), sourcePaths}, {QStringLiteral("clips"), entries}}).toJson();
        QString stagedPath;
        const auto cleanup = qScopeGuard([&] {
            if (!stagedPath.isEmpty())
                QFile::remove(stagedPath);
        });
        {
            QTemporaryFile staged(QDir(detail::canonicalPath(projectDirectory(path))).filePath(
                QStringLiteral(".spoolcut-project-XXXXXX")));
            const bool opened = staged.open();
            detail::require(opened, staged.errorString());
            const bool written = staged.write(bytes) == bytes.size() && staged.flush();
            detail::require(written, staged.errorString());
            stagedPath = staged.fileName();
            staged.setAutoRemove(false);
            // QTemporaryFile::close() keeps the native handle open for reuse.
            // Destroy it here so Windows can move the staged file.
        }
        detail::replaceProject(stagedPath, path);
    });
}

QString projectDirectory(const QString &projectFile)
{
    const auto path = QDir::fromNativeSeparators(detail::absolutePath(projectFile));
    const auto slash = path.lastIndexOf(QLatin1Char('/'));
    // Include the slash for a filesystem root (including C:/).
    if (slash == 0 || (slash == 2 && path[1] == QLatin1Char(':')))
        return path.left(slash + 1);
    return path.left(slash);
}

QString resolveProjectSource(const QString &root, const QString &path)
{
    return QDir::fromNativeSeparators(detail::absolutePath(detail::rootedPath(root, path)));
}

QString projectSourcePath(const QString &root, const QString &path)
{
    auto prefix = resolveProjectSource({}, root);
    if (!prefix.endsWith(QLatin1Char('/')))
        prefix += QLatin1Char('/');
    const auto absolute = resolveProjectSource({}, path);
    return absolute.startsWith(prefix) ? absolute.mid(prefix.size()) : absolute;
}

double TimeBase::seconds(qint64 ticks) const
{
    return static_cast<double>(ticks) * numerator / denominator;
}

QJsonObject TimeBase::json() const
{
    return {{QStringLiteral("numerator"), numerator}, {QStringLiteral("denominator"), denominator}};
}

QJsonObject MediaInfo::json() const
{
    QJsonArray cuts;
    for (const auto pts : safeInPts)
        cuts.append(pts);
    return {{QStringLiteral("video_codec"), videoCodec}, {QStringLiteral("audio_codec"), audioCodec},
            {QStringLiteral("video_time_base"), videoBase.json()}, {QStringLiteral("safe_in_pts"), cuts},
            {QStringLiteral("video_end_pts"), videoEndPts}};
}

QJsonObject PacketRange::json() const
{
    return {{QStringLiteral("start"), static_cast<qint64>(start)}, {QStringLiteral("end"), static_cast<qint64>(end)}};
}

QJsonObject PlannedClip::json() const
{
    auto result = clipJson(clip);
    result.insert(QStringLiteral("timeline_start_us"), timelineStartUs);
    result.insert(QStringLiteral("duration_us"), durationUs);
    result.insert(QStringLiteral("video_packets"), videoPackets.json());
    result.insert(QStringLiteral("audio_packets"), audioPackets.json());
    return result;
}

QJsonObject ExportPlan::json() const
{
    QJsonArray entries;
    for (const auto &clip : clips)
        entries.append(clip.json());
    return {{QStringLiteral("clips"), entries}, {QStringLiteral("duration_us"), durationUs}};
}

namespace detail {

TimeBase parseTimeBase(const QString &text)
{
    const auto components = text.split(QLatin1Char('/'));
    require(components.size() == 2, QStringLiteral("invalid time base %1").arg(text));
    bool validNumerator = false;
    bool validDenominator = false;
    const int numerator = components[0].toInt(&validNumerator);
    const int denominator = components[1].toInt(&validDenominator);
    require(validNumerator && validDenominator && numerator > 0 && denominator > 0,
            QStringLiteral("invalid time base %1").arg(text));
    return {numerator, denominator};
}

std::strong_ordering compare(TimeBase base, Wide ticks, TimeBase other, Wide otherTicks)
{
    return (ticks * base.numerator * other.denominator) <=> (otherTicks * other.numerator * base.denominator);
}

qint64 checkedTicks(Wide value, const QString &message)
{
    require(value >= std::numeric_limits<qint64>::min() && value <= std::numeric_limits<qint64>::max(), message);
    return static_cast<qint64>(value);
}

qint64 micros(TimeBase base, Wide ticks)
{
    const auto scaled = ticks * base.numerator * 1'000'000;
    const auto half = base.denominator / 2;
    const auto rounded = (scaled + (scaled < 0 ? -half : half)) / base.denominator;
    return checkedTicks(rounded, QStringLiteral("timestamp outside FFmpeg's microsecond range"));
}

QString ffmpegTime(qint64 value)
{
    const auto magnitude = static_cast<quint64>(value < 0 ? -Wide{value} : value);
    return QStringLiteral("%1%2.%3").arg(value < 0 ? QStringLiteral("-") : QString{})
        .arg(magnitude / 1'000'000).arg(magnitude % 1'000'000, 6, 10, QLatin1Char('0'));
}

} // namespace detail
} // namespace spoolcut
