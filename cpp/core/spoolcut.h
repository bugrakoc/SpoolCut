// SPDX-License-Identifier: GPL-3.0-only
#pragma once

#include <QJsonObject>
#include <QSize>
#include <QString>

#include <atomic>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <stdexcept>
#include <utility>
#include <vector>

namespace spoolcut {

using Cancel = std::atomic_bool;

class Error : public std::runtime_error {
public:
    explicit Error(const QString &message) : std::runtime_error(message.toUtf8().constData()) {}
};

struct TimeBase {
    int numerator;
    int denominator;
    [[nodiscard]] double seconds(qint64 ticks) const;
    [[nodiscard]] QJsonObject json() const;
};

struct Clip {
    QString source;
    qint64 inPts;
    qint64 outPts;
    bool operator==(const Clip &) const = default;
};

struct Project {
    std::map<QString, QString> sources;
    std::vector<Clip> clips;
    static Project load(const QString &path);
    void save(const QString &path) const;
    bool operator==(const Project &) const = default;
};

struct MediaInfo {
    QString videoCodec;
    QString audioCodec;
    TimeBase videoBase;
    std::vector<qint64> safeInPts;
    qint64 videoEndPts;
    QSize resolution;
    std::optional<TimeBase> averageFrameRate;
    std::vector<qint64> keyframePts;
    [[nodiscard]] QJsonObject json() const;
};

struct PacketRange {
    qsizetype start;
    qsizetype end;
    [[nodiscard]] QJsonObject json() const;
};

struct PlannedClip {
    Clip clip;
    qint64 timelineStartUs;
    qint64 durationUs;
    PacketRange videoPackets;
    PacketRange audioPackets;
    [[nodiscard]] QJsonObject json() const;
};

struct ExportPlan {
    std::vector<PlannedClip> clips;
    qint64 durationUs;
    [[nodiscard]] QJsonObject json() const;
};

class IndexedSource {
public:
    static IndexedSource open(const QString &path, const Cancel &cancel);
    [[nodiscard]] MediaInfo info() const;
    [[nodiscard]] PlannedClip planClip(const Clip &clip) const;
    [[nodiscard]] double frameAt(const Clip &clip, double sourceSeconds) const;
    void checkSource() const;

private:
    struct Data;
    explicit IndexedSource(std::shared_ptr<const Data> data) : data_(std::move(data)) {}
    std::shared_ptr<const Data> data_;
};

enum class ExportStage { Planning, Copying, Joining, Verifying };
struct ExportProgress {
    ExportStage stage;
    size_t completed = 0;
    size_t total = 0;
};

MediaInfo inspect(const QString &path, const Cancel &cancel);
ExportPlan plan(const Project &project, const QString &root, const Cancel &cancel);
void exportProject(const Project &project, const QString &root, const QString &output,
                   const Cancel &cancel, const std::function<void(ExportProgress)> &progress = {});

// Retain parent components that can resolve through a symbolic link.
QString projectDirectory(const QString &projectFile);

// Store paths relative to the project directory when the two locations share a root.
QString projectSourcePath(const QString &root, const QString &path);

} // namespace spoolcut
