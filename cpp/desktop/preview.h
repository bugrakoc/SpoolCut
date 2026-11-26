#pragma once

#include <QTimer>
#include <QWidget>
#include <optional>

class QStackedLayout;
class QLabel;
class Surface;

class Preview final : public QWidget {
    Q_OBJECT
public:
    explicit Preview(QWidget *parent = nullptr);
    ~Preview() override;
    void open(const QString &path, double seconds, bool playing, std::optional<double> end = {});
    void seek(double seconds);
    void setPlaying(bool playing);
    void stop(bool missingMedia = false);
    QString renderer() const;
    QString activeRenderer() const;
    QString openGlRenderer() const { return openGlRenderer_; }
    QString openGlVersion() const { return openGlVersion_; }
    double position() const { return position_; }

signals:
    void positionChanged(double seconds);
    void rendererChanged(const QString &renderer);
    void failed(const QString &message);
    void ended();
    void frameDrawn();
    void audioChanged(const QString &parameters);

private:
    void installSurface(bool software);
    void fallback(const QString &reason);
    void handleRenderer(const QString &renderer, const QString &glRenderer, const QString &glVersion);
    void handleFailure(const QString &reason);

    QStackedLayout *layout_;
    QLabel *placeholder_;
    QWidget *view_ = nullptr;
    Surface *surface_ = nullptr;
    QTimer fallbackTimer_;
    QString path_;
    QString renderer_;
    QString openGlRenderer_;
    QString openGlVersion_;
    QString fallbackReason_;
    double position_ = 0;
    std::optional<double> end_;
    bool playing_ = false;
    bool software_ = false;
};
