#pragma once

#include <QWidget>
#include <vector>

// These times are display coordinates for whole-source playback only.
struct PreviewClip {
    QString label;
    double duration;
};

class Timeline final : public QWidget {
    Q_OBJECT
public:
    explicit Timeline(QWidget *parent = nullptr);
    void append(PreviewClip clip);
    void clear();
    void setPosition(double seconds);
    void zoom(double factor);
    void fit();
    double xAt(double seconds) const;
    double duration() const { return duration_; }

signals:
    void scrubbed(double seconds);

protected:
    void paintEvent(QPaintEvent *) override;
    void resizeEvent(QResizeEvent *) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;

private:
    double scale() const;
    void scrub(double x);
    void clampOffset();

    std::vector<PreviewClip> clips_;
    double duration_ = 0;
    double position_ = 0;
    double zoom_ = 1;
    double offset_ = 0;
};
