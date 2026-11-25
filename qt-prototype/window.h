#pragma once

#include <QMainWindow>
#include <QProcess>
#include <QStringList>
#include <QTimer>
#include <vector>

class Preview;
class Timeline;
class QListWidget;
class QAction;

class Window final : public QMainWindow {
    Q_OBJECT
public:
    Window();
    void importFiles(const QStringList &paths);
    void scrub(double seconds);
    void togglePlayback();
    void clear();
    Preview *preview() const { return preview_; }
    Timeline *timeline() const { return timeline_; }
    qsizetype clipCount() const { return static_cast<qsizetype>(clips_.size()); }

signals:
    void imported(const QString &path);
    void problem(const QString &message);

protected:
    void dragEnterEvent(QDragEnterEvent *event) override;
    void dropEvent(QDropEvent *event) override;

private:
    void probeNext();
    void finishProbe(int exitCode, QProcess::ExitStatus status);
    void select(int row, double localTime, bool playing);
    void setPlaying(bool playing);
    void report(const QString &message);

    struct Clip {
        QString path;
        double start;
        double duration;
    };
    std::vector<Clip> clips_;
    Preview *preview_;
    Timeline *timeline_;
    QListWidget *media_;
    QAction *play_;
    QProcess probe_;
    QTimer probeTimeout_;
    QStringList queue_;
    QString probing_;
    int current_ = -1;
    bool playing_ = false;
};
