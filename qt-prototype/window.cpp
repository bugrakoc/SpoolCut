#include "window.h"
#include "preview.h"
#include "timeline.h"

#include <QAction>
#include <QCoreApplication>
#include <QDir>
#include <QDragEnterEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeySequence>
#include <QListWidget>
#include <QMenuBar>
#include <QMimeData>
#include <QSignalBlocker>
#include <QSplitter>
#include <QStandardPaths>
#include <QStatusBar>
#include <QToolBar>
#include <QUrl>

#include <algorithm>
#include <cmath>

namespace {
QString ffprobe() {
#ifdef Q_OS_WIN
    const QString executable = QStringLiteral("ffprobe.exe");
#else
    const QString executable = QStringLiteral("ffprobe");
#endif
    if (qEnvironmentVariableIsSet("SPOOLCUT_FFPROBE")) {
        const QFileInfo configured(qEnvironmentVariable("SPOOLCUT_FFPROBE"));
        return configured.isFile() && configured.isExecutable() ? configured.absoluteFilePath() : QString{};
    }
    const QDir app(QCoreApplication::applicationDirPath());
    for (const auto &name : {executable, QStringLiteral("bin/") + executable}) {
        const QFileInfo candidate(app.filePath(name));
        if (candidate.isFile() && candidate.isExecutable())
            return candidate.absoluteFilePath();
    }
    return QStandardPaths::findExecutable(executable);
}
}

Window::Window()
    : preview_(new Preview(this)), timeline_(new Timeline(this)), media_(new QListWidget(this)) {
    setWindowTitle(tr("SpoolCut Qt prototype"));
    setAcceptDrops(true);
    resize(960, 660);
    auto *upper = new QSplitter(Qt::Horizontal, this);
    media_->setAccessibleName(tr("Media bin"));
    media_->setMinimumWidth(140);
    upper->addWidget(media_);
    upper->addWidget(preview_);
    upper->setSizes({220, 740});
    auto *layout = new QSplitter(Qt::Vertical, this);
    layout->addWidget(upper);
    layout->addWidget(timeline_);
    layout->setStretchFactor(0, 1);
    setCentralWidget(layout);

    auto *file = menuBar()->addMenu(tr("&File"));
    auto *open = file->addAction(tr("&Open media…"), QKeySequence::Open, this, [this] {
        importFiles(QFileDialog::getOpenFileNames(this, tr("Open media"), {},
                                                tr("MP4 and MOV (*.mp4 *.mov *.MP4 *.MOV)")));
    });
    file->addAction(tr("Clear media"), this, &Window::clear);
    file->addAction(tr("E&xit"), QKeySequence::Quit, this, &Window::close);

    auto *playback = menuBar()->addMenu(tr("&Playback"));
    play_ = playback->addAction(tr("Play"), QKeySequence(Qt::Key_Space), this, &Window::togglePlayback);
    play_->setEnabled(false);
    auto *view = menuBar()->addMenu(tr("&View"));
    auto *out = view->addAction(tr("Zoom out"), QKeySequence::ZoomOut, this, [this] { timeline_->zoom(1 / 1.5); });
    auto *in = view->addAction(tr("Zoom in"), QKeySequence::ZoomIn, this, [this] { timeline_->zoom(1.5); });
    auto *fit = view->addAction(tr("Fit timeline"), this, [this] { timeline_->fit(); });

    auto *toolbar = addToolBar(tr("Media controls"));
    toolbar->setMovable(false);
    toolbar->addAction(open);
    toolbar->addAction(play_);
    toolbar->addSeparator();
    toolbar->addActions({out, in, fit});

    connect(media_, &QListWidget::currentRowChanged, this, [this](int row) {
        if (row >= 0)
            select(row, 0, false);
    });
    connect(timeline_, &Timeline::scrubbed, this, &Window::scrub);
    connect(preview_, &Preview::rendererChanged, statusBar(), [this](const QString &renderer) {
        statusBar()->showMessage(renderer);
        qInfo().noquote() << renderer;
    });
    connect(preview_, &Preview::failed, this, &Window::report);
    connect(preview_, &Preview::positionChanged, this, [this](double seconds) {
        if (current_ >= 0) {
            const auto &clip = clips_[current_];
            timeline_->setPosition(clip.start + std::clamp(seconds, 0.0, clip.duration));
        }
    });
    connect(preview_, &Preview::ended, this, [this] {
        if (playing_ && current_ + 1 < clipCount())
            select(current_ + 1, 0, true);
        else
            setPlaying(false);
    });

    probeTimeout_.setSingleShot(true);
    connect(&probeTimeout_, &QTimer::timeout, this, [this] {
        const QString path = std::exchange(probing_, {});
        report(tr("Timed out probing %1").arg(path));
        probe_.kill();
    });
    connect(&probe_, &QProcess::finished, this, &Window::finishProbe);
    connect(&probe_, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error != QProcess::FailedToStart)
            return;
        probeTimeout_.stop();
        probing_.clear();
        report(probe_.errorString());
        QTimer::singleShot(0, this, &Window::probeNext);
    });
}

void Window::report(const QString &message) {
    statusBar()->showMessage(message);
    emit problem(message);
}

void Window::importFiles(const QStringList &paths) {
    for (const auto &path : paths)
        queue_.push_back(QFileInfo(path).absoluteFilePath());
    probeNext();
}

void Window::probeNext() {
    if (queue_.isEmpty() || probe_.state() != QProcess::NotRunning)
        return;
    const QString program = ffprobe();
    if (program.isEmpty()) {
        queue_.clear();
        report(tr("Could not find ffprobe. Check SPOOLCUT_FFPROBE or install ffprobe."));
        return;
    }
    probing_ = queue_.takeFirst();
    probeTimeout_.start(10000);
    probe_.start(program, {"-v", "error", "-show_entries",
                           "format=duration,format_name:stream=codec_name,codec_type,width,height",
                           "-of", "json", "-i", QDir::toNativeSeparators(probing_)});
}

void Window::finishProbe(int exitCode, QProcess::ExitStatus state) {
    probeTimeout_.stop();
    const auto result = QJsonDocument::fromJson(probe_.readAllStandardOutput()).object();
    const auto detail = QString::fromUtf8(probe_.readAllStandardError()).trimmed();
    const QString path = std::exchange(probing_, {});
    const auto format = result.value("format").toObject();
    const double duration = format.value("duration").toString().toDouble();
    QString codec;
    for (const auto &entry : result.value("streams").toArray()) {
        const auto stream = entry.toObject();
        if (stream.value("codec_type").toString() == "video") {
            codec = stream.value("codec_name").toString();
            break;
        }
    }
    if (!path.isEmpty()) {
        if (exitCode == 0 && state == QProcess::NormalExit && std::isfinite(duration)
            && duration > 0 && duration <= 30 * 24 * 3600
            && format.value("format_name").toString().split(',').contains("mov")
            && (codec == "h264" || codec == "hevc")) {
            const QString label = QFileInfo(path).fileName();
            clips_.push_back({path, timeline_->duration(), duration});
            timeline_->append({label, duration});
            media_->addItem(tr("%1 · %2").arg(label, codec.toUpper()));
            play_->setEnabled(true);
            if (current_ < 0)
                select(0, 0, false);
            emit imported(path);
        } else {
            report(tr("Cannot open %1. %2").arg(QFileInfo(path).fileName(), detail.isEmpty()
                     ? tr("Expected an MP4 or MOV with H.264 or HEVC video.") : detail));
        }
    }
    probeNext();
}

void Window::select(int row, double localTime, bool playing) {
    const auto &clip = clips_[row];
    const QSignalBlocker guard(media_);
    media_->setCurrentRow(row);
    timeline_->setPosition(clip.start + localTime);
    if (current_ != row) {
        current_ = row;
        preview_->open(clip.path, localTime, playing);
    } else {
        preview_->seek(localTime);
    }
    setPlaying(playing);
}

void Window::scrub(double seconds) {
    if (clips_.empty())
        return;
    const auto after = std::upper_bound(clips_.begin(), clips_.end(), seconds,
        [](double position, const Clip &clip) { return position < clip.start; });
    const int row = std::max(0, static_cast<int>(after - clips_.begin()) - 1);
    const auto &clip = clips_[row];
    select(row, std::clamp(seconds - clip.start, 0.0, std::max(0.0, clip.duration - 0.001)), false);
}

void Window::togglePlayback() {
    if (current_ >= 0)
        setPlaying(!playing_);
}

void Window::setPlaying(bool playing) {
    playing_ = playing;
    play_->setText(playing ? tr("Pause") : tr("Play"));
    preview_->setPlaying(playing);
}

void Window::clear() {
    queue_.clear();
    probing_.clear();
    if (probe_.state() != QProcess::NotRunning)
        probe_.kill();
    current_ = -1;
    clips_.clear();
    media_->clear();
    timeline_->clear();
    preview_->stop();
    setPlaying(false);
    play_->setEnabled(false);
}

void Window::dragEnterEvent(QDragEnterEvent *event) {
    if (event->mimeData()->hasUrls())
        event->acceptProposedAction();
}

void Window::dropEvent(QDropEvent *event) {
    QStringList paths;
    for (const auto &url : event->mimeData()->urls()) {
        if (url.isLocalFile())
            paths.push_back(url.toLocalFile());
    }
    importFiles(paths);
    event->acceptProposedAction();
}
