// SPDX-License-Identifier: GPL-3.0-only
#pragma once

#include "editor.h"

#include <QMainWindow>
#include <thread>

class Preview;
class Timeline;
class QListWidget;
class QProgressBar;

class Window final : public QMainWindow {
    Q_OBJECT
public:
    Window();
    ~Window() override;
    void importFiles(const QStringList &paths, bool append = false);
    bool openProject(const QString &path);
    bool saveProject(const QString &path);
    void startExport(const QString &path);
    void cancelJob();
    void scrub(double seconds);
    void togglePlayback();
    void clear();
    Preview *preview() const { return preview_; }
    Timeline *timeline() const { return timeline_; }
    size_t clipCount() const { return editor_.project().clips.size(); }
    size_t sourceCount() const { return editor_.project().sources.size(); }

signals:
    void imported(const QString &path);
    void problem(const QString &message);
    void idle();
    void exported(const QString &path);

protected:
    void closeEvent(QCloseEvent *event) override;
    void dragEnterEvent(QDragEnterEvent *event) override;
    void dropEvent(QDropEvent *event) override;

private:
    enum class IndexIntent { Import, Append, Open, Relink };
    using Results = std::vector<std::pair<QString, spoolcut::SourceResult>>;

    void startIndex(QStringList paths, IndexIntent intent, const QString &id = {});
    void finishIndex(Results results, IndexIntent intent, const QString &id);
    void finishJob();
    void progress(spoolcut::ExportProgress value);
    bool confirmDiscard();
    bool pickSave(bool saveAs);
    void pickRelink();
    void refresh();
    void controls();
    void report(const QString &message);
    void edit(const std::function<void()> &operation);
    QString selectedSource() const;
    void insert(const QString &id, size_t slot);
    void select(size_t row);
    void remove();
    void clipboard(bool cut);
    void paste();
    void split();
    void move(size_t row, size_t slot);
    void trim(size_t row, bool in, double seconds);
    void history(bool redo);
    void keyframe(bool forward);
    std::optional<size_t> segmentAt(qint64 time) const;
    void seek(qint64 time, bool playing);
    void setPlaying(bool playing);
    void advance();

    spoolcut::Editor editor_;
    std::vector<spoolcut::Segment> segments_;
    std::optional<size_t> selected_;
    std::optional<size_t> current_;
    qint64 positionUs_ = 0;
    bool playing_ = false;
    std::shared_ptr<spoolcut::Cancel> cancel_;
    std::jthread worker_;
    Preview *preview_;
    Timeline *timeline_;
    QListWidget *media_;
    QProgressBar *progress_;
    QAction *play_;
    QAction *append_;
    QAction *relink_;
    QAction *split_;
    QAction *remove_;
    QAction *copy_;
    QAction *cut_;
    QAction *paste_;
    QAction *undo_;
    QAction *redo_;
    QAction *export_;
    QAction *cancelAction_;
    QList<QAction *> projectActions_;
};
