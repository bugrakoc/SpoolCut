// SPDX-License-Identifier: GPL-3.0-only
#pragma once

#include "spoolcut.h"

#include <QByteArray>
#include <QFile>
#include <QStringList>

#include <array>
#include <compare>
#include <limits>

namespace spoolcut::detail {

inline void require(bool condition, const QString &message)
{
    if (!condition)
        throw Error(message);
}

template<class Function>
decltype(auto) context(const QString &message, Function &&function)
{
    try {
        return std::forward<Function>(function)();
    } catch (const Error &error) {
        throw Error(message + QStringLiteral(": ") + QString::fromUtf8(error.what()));
    }
}

// The supported GCC/Clang toolchains provide signed 128-bit integers. Products of a
// difference of two i64 ticks and two positive i32 time-base terms fit in this type.
__extension__ using Wide = __int128;

TimeBase parseTimeBase(const QString &text);
std::strong_ordering compare(TimeBase base, Wide ticks, TimeBase other, Wide otherTicks);
qint64 checkedTicks(Wide value, const QString &message);
qint64 micros(TimeBase base, Wide ticks);
QString ffmpegTime(qint64 value);
void checkCancelled(const Cancel &cancel);

QString canonicalPath(const QString &path);
QString mediaPath(const QString &path);
void publish(const QString &staged, const QString &destination);
void replaceProject(const QString &staged, const QString &destination);

struct SourceIdentity {
    quint64 volume;
    std::array<unsigned char, 16> id;
    quint64 size;
    qint64 modified;
    qint64 modifiedFraction;
    bool extended = false;
    static SourceIdentity read(QFile &file);
    bool operator==(const SourceIdentity &) const = default;
};

QString findTool(const QString &name, const QString &appDirectory);
QByteArray runTool(const QString &name, const QStringList &arguments, const Cancel &cancel,
                   bool capture = false, const QString &directory = {});

struct Stream {
    qint64 index;
    QString type;
    QString timeBase;
    QString averageFrameRate;
    QJsonObject parameters;
    [[nodiscard]] QString parameter(const QString &name) const;
};

struct Packet {
    qint64 streamIndex;
    std::optional<qint64> pts;
    std::optional<qint64> dts;
    std::optional<qint64> duration;
    QString position;
    QString size;
    bool key;
    bool usable;
};

struct Media {
    QString path;
    QString sourcePath;
    SourceIdentity identity;
    Stream video;
    Stream audio;
    TimeBase videoBase;
    TimeBase audioBase;
    qint64 audioFrameTicks;
    std::vector<Packet> videoPackets;
    std::vector<Packet> audioPackets;
    std::map<qint64, qsizetype> cuts;
    qint64 videoEndPts;

    static Media open(const QString &path, const Cancel &cancel);
    [[nodiscard]] MediaInfo info() const;
    void checkSource() const;
};

QByteArray decodeExtradata(const QString &dump);
QJsonObject probe(const QString &path, const QString &format, const QStringList &options,
                  const QString &fields, const Cancel &cancel);
std::vector<Packet> readPackets(const QString &path, const QString &format, const Cancel &cancel);

struct Prepared {
    ExportPlan plan;
    std::map<QString, Media> sources;
};

PlannedClip planClip(const Media &media, const Clip &clip, qint64 timelineStart);
Prepared prepare(const Project &project, const QString &root, const Cancel &cancel);

} // namespace spoolcut::detail
