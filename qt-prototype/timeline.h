// SPDX-License-Identifier: GPL-3.0-only
#pragma once

#include <QColor>
#include <QWidget>

#include <optional>
#include <vector>

struct PreviewClip {
    QString label;
    double duration;
    std::vector<double> safeOffsets;
    QColor color{};
    QString unavailable{};
};

QColor sourceColor(size_t row);

class Timeline final : public QWidget {
    Q_OBJECT
public:
    explicit Timeline(QWidget *parent = nullptr);
    void setClips(std::vector<PreviewClip> clips, const QString &timingProblem = {});
    void setSelected(std::optional<size_t> row);
    void setPosition(double seconds, bool follow = false);
    void zoom(double factor);
    void fit();
    void cancelDrag();
    double xAt(double seconds) const;
    double duration() const { return duration_; }
    bool hasTiming() const { return timingProblem_.isEmpty(); }
    const std::vector<PreviewClip> &clips() const { return clips_; }

signals:
    void scrubbed(double seconds);
    void selected(size_t row);
    void moved(size_t row, size_t slot);
    void trimmed(size_t row, bool in, double seconds);
    void sourceDropped(const QString &id, size_t slot);

protected:
    void paintEvent(QPaintEvent *) override;
    void resizeEvent(QResizeEvent *) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void dragEnterEvent(QDragEnterEvent *event) override;
    void dragMoveEvent(QDragMoveEvent *event) override;
    void dragLeaveEvent(QDragLeaveEvent *event) override;
    void dropEvent(QDropEvent *event) override;

private:
    enum class Drag { Scrub, Move, TrimIn, TrimOut };
    struct Gesture {
        Drag type;
        size_t row = 0;
        double x = 0;
    };
    double scale() const;
    double timeAt(double x) const;
    double nearestSafe(double time) const;
    size_t slotAt(double x) const;
    std::optional<Gesture> hit(double x) const;
    void zoomAt(double factor, double x);
    void clampOffset();

    std::vector<PreviewClip> clips_;
    std::vector<double> starts_;
    std::vector<double> safePoints_;
    std::optional<size_t> selected_;
    std::optional<Gesture> drag_;
    std::optional<double> draft_;
    QString timingProblem_;
    double duration_ = 0;
    double position_ = 0;
    double zoom_ = 1;
    double offset_ = 0;
};
