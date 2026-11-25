// SPDX-License-Identifier: GPL-3.0-only
#pragma once

#include "spoolcut.h"

#include <QStringList>
#include <variant>

namespace spoolcut {

struct Segment {
    QString path;
    qint64 startUs;
    qint64 endUs;
    double sourceStart;
    double sourceEnd;
    std::vector<qint64> safePoints;
};

using SourceResult = std::variant<IndexedSource, QString>;

class Editor {
public:
    void open(const QString &path);
    void save(const QString &path);
    bool dirty() const { return project_ != saved_; }
    const Project &project() const { return project_; }
    const QString &path() const { return path_; }

    void setIndex(const QString &path, SourceResult result);
    const SourceResult *indexResult(const QString &path) const;
    const IndexedSource &index(const QString &id) const;
    void import(const QStringList &paths);
    void relink(const QString &id, const QString &path, IndexedSource index);
    std::vector<Segment> segments() const;

    size_t insertSource(const QString &id, size_t slot);
    void remove(size_t index);
    size_t move(size_t index, size_t slot);
    void trim(size_t index, bool in, double sourceSeconds);
    void split(size_t index, double sourceSeconds);
    bool copy(size_t index);
    bool hasClipboard() const { return clipboard_.has_value(); }
    void paste(size_t slot);
    bool canUndo() const { return !undo_.empty(); }
    bool canRedo() const { return !redo_.empty(); }
    void undo();
    void redo();

private:
    void change(Project next);
    Project project_;
    Project saved_;
    QString path_;
    std::map<QString, SourceResult> media_;
    std::optional<std::pair<Clip, QString>> clipboard_;
    std::vector<Project> undo_;
    std::vector<Project> redo_;
};

} // namespace spoolcut
