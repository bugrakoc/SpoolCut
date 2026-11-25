// SPDX-License-Identifier: GPL-3.0-only
#include "window.h"
#include "preview.h"
#include "timeline.h"

#include <QAction>
#include <QCloseEvent>
#include <QDir>
#include <QDragEnterEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QKeySequence>
#include <QListWidget>
#include <QMenuBar>
#include <QMessageBox>
#include <QMimeData>
#include <QProgressBar>
#include <QSignalBlocker>
#include <QSplitter>
#include <QStatusBar>
#include <QToolBar>
#include <QUrl>

#include <algorithm>
#include <cmath>

using namespace Qt::StringLiterals;
using spoolcut::Error;
using spoolcut::IndexedSource;

namespace {
constexpr auto mediaFilter = "MP4 and MOV (*.mp4 *.mov *.MP4 *.MOV)";
constexpr auto projectFilter = "SpoolCut projects (*.json)";

class MediaBin final : public QListWidget {
public:
    using QListWidget::QListWidget;
protected:
    QMimeData *mimeData(const QList<QListWidgetItem *> &items) const override
    {
        auto *data = new QMimeData;
        if (!items.isEmpty())
            data->setData("application/x-spoolcut-source", items.front()->data(Qt::UserRole).toString().toUtf8());
        return data;
    }
};

double seconds(qint64 time) { return static_cast<double>(time) / 1'000'000; }

qint64 timelineTime(double time, qint64 end)
{
    const auto micros = std::round(static_cast<long double>(time) * 1'000'000);
    if (micros >= static_cast<long double>(end))
        return end;
    return micros <= 0 ? 0 : static_cast<qint64>(micros);
}

QString errorText(const std::exception &error) { return QString::fromUtf8(error.what()); }
}

Window::Window()
    : preview_(new Preview(this)), timeline_(new Timeline(this)), media_(new MediaBin(this)),
      progress_(new QProgressBar(this))
{
    setAcceptDrops(true);
    resize(960, 660);
    auto *upper = new QSplitter(Qt::Horizontal, this);
    media_->setAccessibleName(tr("Media bin"));
    media_->setMinimumWidth(140);
    media_->setDragEnabled(true);
    media_->setDragDropMode(QAbstractItemView::DragOnly);
    media_->setDefaultDropAction(Qt::CopyAction);
    upper->addWidget(media_);
    upper->addWidget(preview_);
    upper->setSizes({240, 720});
    upper->setChildrenCollapsible(false);
    auto *layout = new QSplitter(Qt::Vertical, this);
    layout->addWidget(upper);
    layout->addWidget(timeline_);
    layout->setStretchFactor(0, 1);
    layout->setChildrenCollapsible(false);
    setCentralWidget(layout);

    auto *file = menuBar()->addMenu(tr("&File"));
    auto *fresh = file->addAction(tr("&New project"), QKeySequence::New, this, [this] {
        if (confirmDiscard())
            clear();
    });
    auto *open = file->addAction(tr("&Open project…"), QKeySequence::Open, this, [this] {
        if (!confirmDiscard())
            return;
        const auto path = QFileDialog::getOpenFileName(this, tr("Open project"), {}, tr(projectFilter));
        if (!path.isEmpty())
            openProject(path);
    });
    auto *save = file->addAction(tr("&Save project"), QKeySequence::Save, this, [this] { pickSave(false); });
    auto *saveAs = file->addAction(tr("Save project &as…"), QKeySequence::SaveAs, this, [this] { pickSave(true); });
    auto *import = file->addAction(tr("&Import media…"), QKeySequence("Ctrl+I"), this, [this] {
        importFiles(QFileDialog::getOpenFileNames(this, tr("Import media"), {}, tr(mediaFilter)));
    });
    relink_ = file->addAction(tr("&Relink source…"), this, &Window::pickRelink);
    export_ = file->addAction(tr("&Export Matroska…"), QKeySequence("Ctrl+E"), this, [this] {
        const auto path = QFileDialog::getSaveFileName(this, tr("Export timeline"), u"SpoolCut.mkv"_s,
                                                       tr("Matroska video (*.mkv)"), nullptr,
                                                       QFileDialog::DontConfirmOverwrite);
        if (!path.isEmpty())
            startExport(path.endsWith(u".mkv"_s, Qt::CaseInsensitive) ? path : path + u".mkv"_s);
    });
    cancelAction_ = file->addAction(tr("Cancel operation"), this, &Window::cancelJob);
    file->addAction(tr("E&xit"), QKeySequence::Quit, this, &Window::close);
    projectActions_ = {fresh, open, save, saveAs, import};

    auto *editMenu = menuBar()->addMenu(tr("&Edit"));
    append_ = editMenu->addAction(tr("Append selected source"), this, [this] { insert(selectedSource(), clipCount()); });
    append_->setObjectName(u"appendSource"_s);
    split_ = editMenu->addAction(tr("&Split at playhead"), QKeySequence(Qt::Key_S), this, &Window::split);
    remove_ = editMenu->addAction(tr("&Remove clip"), QKeySequence(Qt::Key_Delete), this, &Window::remove);
    cut_ = editMenu->addAction(tr("Cut clip"), QKeySequence::Cut, this, [this] { clipboard(true); });
    copy_ = editMenu->addAction(tr("Copy clip"), QKeySequence::Copy, this, [this] { clipboard(false); });
    paste_ = editMenu->addAction(tr("Paste clip"), QKeySequence::Paste, this, &Window::paste);
    undo_ = editMenu->addAction(tr("Undo"), QKeySequence::Undo, this, [this] { history(false); });
    redo_ = editMenu->addAction(tr("Redo"), QKeySequence("Ctrl+Shift+Z"), this, [this] { history(true); });
    editMenu->addAction(tr("Move clip left"), QKeySequence("Ctrl+Left"), this, [this] {
        if (selected_ && *selected_ > 0)
            move(*selected_, *selected_ - 1);
    });
    editMenu->addAction(tr("Move clip right"), QKeySequence("Ctrl+Right"), this, [this] {
        if (selected_ && *selected_ + 1 < clipCount())
            move(*selected_, *selected_ + 2);
    });
    editMenu->addAction(tr("Cancel timeline drag"), QKeySequence(Qt::Key_Escape), timeline_, &Timeline::cancelDrag);

    auto *playback = menuBar()->addMenu(tr("&Playback"));
    play_ = playback->addAction(tr("Play"), QKeySequence(Qt::Key_Space), this, &Window::togglePlayback);
    playback->addAction(tr("Previous safe keyframe"), QKeySequence(Qt::Key_Left), this, [this] { keyframe(false); });
    playback->addAction(tr("Next safe keyframe"), QKeySequence(Qt::Key_Right), this, [this] { keyframe(true); });
    auto *view = menuBar()->addMenu(tr("&View"));
    auto *out = view->addAction(tr("Zoom out"), QKeySequence::ZoomOut, this, [this] { timeline_->zoom(1 / 1.5); });
    auto *in = view->addAction(tr("Zoom in"), QKeySequence::ZoomIn, this, [this] { timeline_->zoom(1.5); });
    auto *fit = view->addAction(tr("Fit timeline"), this, [this] { timeline_->fit(); });

    auto *toolbar = addToolBar(tr("Editing controls"));
    toolbar->setMovable(false);
    toolbar->addActions({import, append_, play_, split_});
    toolbar->addSeparator();
    toolbar->addActions({out, in, fit});
    toolbar->addSeparator();
    toolbar->addActions({export_, cancelAction_});
    progress_->setMaximumWidth(155);
    progress_->hide();
    statusBar()->addPermanentWidget(progress_);

    connect(media_, &QListWidget::currentItemChanged, this, &Window::controls);
    connect(media_, &QListWidget::itemDoubleClicked, this, [this] { insert(selectedSource(), clipCount()); });
    connect(timeline_, &Timeline::scrubbed, this, &Window::scrub);
    connect(timeline_, &Timeline::selected, this, &Window::select);
    connect(timeline_, &Timeline::moved, this, &Window::move);
    connect(timeline_, &Timeline::trimmed, this, &Window::trim);
    connect(timeline_, &Timeline::sourceDropped, this, &Window::insert);
    connect(preview_, &Preview::rendererChanged, statusBar(), [this](const QString &renderer) {
        statusBar()->showMessage(renderer);
        qInfo().noquote() << renderer;
    });
    connect(preview_, &Preview::failed, this, [this](const QString &message) {
        current_.reset();
        setPlaying(false);
        report(message);
    });
    connect(preview_, &Preview::positionChanged, this, [this](double sourceTime) {
        if (!playing_ || !current_)
            return;
        const auto &segment = segments_[*current_];
        positionUs_ = segment.startUs + timelineTime(sourceTime - segment.sourceStart, segment.endUs - segment.startUs);
        timeline_->setPosition(seconds(positionUs_), true);
    });
    connect(preview_, &Preview::ended, this, &Window::advance);
    controls();
}

Window::~Window()
{
    if (cancel_)
        cancel_->store(true, std::memory_order_relaxed);
    if (worker_.joinable())
        worker_.join();
}

void Window::report(const QString &message)
{
    statusBar()->showMessage(message);
    emit problem(message);
}

bool Window::confirmDiscard()
{
    if (!editor_.dirty())
        return true;
    const auto choice = QMessageBox::warning(this, tr("Unsaved project"),
                                             tr("Save changes to this project?"),
                                             QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel,
                                             QMessageBox::Save);
    return choice == QMessageBox::Discard || (choice == QMessageBox::Save && pickSave(false));
}

bool Window::pickSave(bool saveAs)
{
    auto path = editor_.path();
    if (saveAs || path.isEmpty()) {
        if (path.isEmpty())
            path = u"Untitled.spoolcut.json"_s;
        path = QFileDialog::getSaveFileName(this, tr("Save project"), path, tr(projectFilter));
    }
    return !path.isEmpty() && saveProject(path);
}

bool Window::saveProject(const QString &path)
{
    try {
        editor_.save(path);
        controls();
        statusBar()->showMessage(tr("Saved %1").arg(QFileInfo(path).fileName()));
        return true;
    } catch (const std::exception &error) {
        report(errorText(error));
        return false;
    }
}

bool Window::openProject(const QString &path)
{
    if (cancel_)
        return false;
    try {
        editor_.open(path);
        selected_.reset();
        positionUs_ = 0;
        timeline_->fit();
        refresh();
        QStringList paths;
        for (const auto &[id, source] : editor_.project().sources)
            paths.append(source);
        startIndex(std::move(paths), IndexIntent::Open);
        return true;
    } catch (const std::exception &error) {
        report(errorText(error));
        return false;
    }
}

void Window::importFiles(const QStringList &paths, bool append)
{
    if (cancel_)
        return;
    QStringList absolute;
    for (const auto &path : paths)
        absolute.append(spoolcut::resolveProjectSource({}, path));
    startIndex(std::move(absolute), append ? IndexIntent::Append : IndexIntent::Import);
}

void Window::pickRelink()
{
    if (const auto id = selectedSource(); !id.isEmpty()) {
        const auto path = QFileDialog::getOpenFileName(this, tr("Locate source file"), {}, tr(mediaFilter));
        if (!path.isEmpty())
            startIndex({spoolcut::resolveProjectSource({}, path)}, IndexIntent::Relink, id);
    }
}

void Window::startIndex(QStringList paths, IndexIntent intent, const QString &id)
{
    if (paths.isEmpty() || cancel_)
        return;
    paths.removeDuplicates();
    cancel_ = std::make_shared<spoolcut::Cancel>(false);
    setPlaying(false);
    controls();
    progress_->setRange(0, 0);
    progress_->show();
    statusBar()->showMessage(tr("Inspecting %1 source file(s)…").arg(paths.size()));
    worker_ = std::jthread([this, paths = std::move(paths), intent, id, cancel = cancel_] {
        Results results;
        for (const auto &path : paths) {
            if (cancel->load(std::memory_order_relaxed))
                break;
            try {
                results.emplace_back(path, IndexedSource::open(path, *cancel));
            } catch (const std::exception &error) {
                results.emplace_back(path, errorText(error));
            }
        }
        QMetaObject::invokeMethod(this, [this, results = std::move(results), intent, id]() mutable {
            finishIndex(std::move(results), intent, id);
        }, Qt::QueuedConnection);
    });
}

void Window::finishIndex(Results results, IndexIntent intent, const QString &id)
{
    const bool cancelled = cancel_->load(std::memory_order_relaxed);
    finishJob();
    QStringList added;
    QStringList errors;
    if (!cancelled || intent == IndexIntent::Open) {
        for (auto &[path, result] : results) {
            if (const auto *problem = std::get_if<QString>(&result))
                errors.append(*problem);
            else
                added.append(path);
            if (intent == IndexIntent::Relink) {
                if (const auto *source = std::get_if<IndexedSource>(&result)) {
                    try {
                        editor_.relink(id, path, *source);
                    } catch (const std::exception &error) {
                        errors.append(errorText(error));
                    }
                }
            } else {
                editor_.setIndex(path, std::move(result));
            }
        }
        if (intent == IndexIntent::Import || intent == IndexIntent::Append)
            editor_.import(added);
    }
    if (intent == IndexIntent::Append && !cancelled) {
        for (const auto &path : added) {
            for (const auto &[sourceId, sourcePath] : editor_.project().sources) {
                if (sourcePath != path)
                    continue;
                // insertSource replaces the project snapshot, so take the ID before the edit.
                const auto source = sourceId;
                try {
                    editor_.insertSource(source, clipCount());
                } catch (const std::exception &error) {
                    errors.append(errorText(error));
                }
                break;
            }
        }
    }
    refresh();
    if (cancelled)
        statusBar()->showMessage(tr("Source inspection cancelled."));
    else if (!errors.isEmpty())
        report(errors.front());
    else
        statusBar()->showMessage(tr("Source files ready."));
    if (!cancelled) {
        for (const auto &path : added)
            emit imported(path);
    }
    emit idle();
}

void Window::finishJob()
{
    if (worker_.joinable())
        worker_.join();
    cancel_.reset();
    progress_->hide();
    controls();
}

void Window::cancelJob()
{
    if (cancel_) {
        cancel_->store(true, std::memory_order_relaxed);
        cancelAction_->setEnabled(false);
        statusBar()->showMessage(tr("Cancelling…"));
    }
}

void Window::startExport(const QString &path)
{
    if (cancel_ || segments_.empty())
        return;
    cancel_ = std::make_shared<spoolcut::Cancel>(false);
    setPlaying(false);
    controls();
    progress_->show();
    progress({spoolcut::ExportStage::Planning});
    worker_ = std::jthread([this, path, project = editor_.project(), cancel = cancel_] {
        QString failure;
        try {
            spoolcut::exportProject(project, {}, path, *cancel, [this](spoolcut::ExportProgress value) {
                QMetaObject::invokeMethod(this, [this, value] { progress(value); }, Qt::QueuedConnection);
            });
        } catch (const std::exception &error) {
            failure = errorText(error);
        }
        QMetaObject::invokeMethod(this, [this, path, failure] {
            finishJob();
            if (failure.isEmpty()) {
                statusBar()->showMessage(tr("Exported %1").arg(QFileInfo(path).fileName()));
                emit exported(path);
            } else {
                report(failure);
            }
            emit idle();
        }, Qt::QueuedConnection);
    });
}

void Window::progress(spoolcut::ExportProgress value)
{
    if (!cancel_ || cancel_->load(std::memory_order_relaxed))
        return;
    progress_->setRange(0, 0);
    switch (value.stage) {
    case spoolcut::ExportStage::Planning:
        statusBar()->showMessage(tr("Checking timeline and stream compatibility…"));
        break;
    case spoolcut::ExportStage::Copying:
        if (value.total != 0) {
            progress_->setRange(0, 100);
            progress_->setValue(static_cast<int>(100.0 * static_cast<double>(value.completed)
                                                  / static_cast<double>(value.total)));
        }
        statusBar()->showMessage(tr("Copying %1 of %2 clips…").arg(value.completed).arg(value.total));
        break;
    case spoolcut::ExportStage::Joining:
        statusBar()->showMessage(tr("Joining copied packets…"));
        break;
    case spoolcut::ExportStage::Verifying:
        statusBar()->showMessage(tr("Verifying the export…"));
        break;
    }
}

QString Window::selectedSource() const
{
    if (const auto *item = media_->currentItem())
        return item->data(Qt::UserRole).toString();
    return {};
}

void Window::refresh()
{
    const auto sourceSelection = selectedSource();
    const QSignalBlocker block(media_);
    media_->clear();
    for (const auto &[id, path] : editor_.project().sources) {
        const auto name = QFileInfo(path).fileName();
        const auto *result = editor_.indexResult(path);
        auto detail = tr("Awaiting inspection");
        auto tooltip = path;
        if (result) {
            if (const auto *source = std::get_if<IndexedSource>(result)) {
                const auto info = source->info();
                const auto rate = info.averageFrameRate
                                      ? tr("%1 fps").arg(QString::number(info.averageFrameRate->seconds(1), 'g', 5))
                                      : tr("Unknown frame rate");
                const auto size = info.resolution.isValid()
                                      ? tr("%1 × %2").arg(info.resolution.width()).arg(info.resolution.height())
                                      : tr("Unknown resolution");
                detail = tr("%1 · %2 · %3").arg(size, info.videoCodec.toUpper(), rate);
            } else {
                detail = QFileInfo::exists(path) ? tr("Unavailable") : tr("Missing file");
                tooltip += u"\n"_s + std::get<QString>(*result);
            }
        }
        auto *item = new QListWidgetItem(tr("%1\n%2").arg(name, detail), media_);
        item->setToolTip(tooltip);
        item->setData(Qt::UserRole, id);
        if (id == sourceSelection)
            media_->setCurrentItem(item);
    }
    if (media_->currentRow() < 0 && media_->count() > 0)
        media_->setCurrentRow(0);

    setPlaying(false);
    current_.reset();
    preview_->stop();
    segments_.clear();
    try {
        segments_ = editor_.segments();
    } catch (const std::exception &error) {
        statusBar()->showMessage(errorText(error));
    }
    std::vector<PreviewClip> clips;
    for (const auto &segment : segments_) {
        std::vector<double> offsets;
        offsets.reserve(segment.safePoints.size());
        for (const auto time : segment.safePoints)
            offsets.push_back(seconds(time - segment.startUs));
        clips.push_back({QFileInfo(segment.path).fileName(), seconds(segment.endUs - segment.startUs), std::move(offsets)});
    }
    if (selected_ && *selected_ >= clipCount())
        selected_.reset();
    timeline_->setClips(std::move(clips));
    timeline_->setSelected(selected_);
    if (!segments_.empty())
        seek(positionUs_, false);
    else
        positionUs_ = 0;
    controls();
}

void Window::controls()
{
    const bool ready = !cancel_;
    setWindowTitle(tr("%1[*] — SpoolCut").arg(editor_.path().isEmpty()
                                                  ? tr("Untitled") : QFileInfo(editor_.path()).fileName()));
    setWindowModified(editor_.dirty());
    for (auto *action : projectActions_)
        action->setEnabled(ready);
    const auto source = editor_.project().sources.find(selectedSource());
    const auto *index = source == editor_.project().sources.end() ? nullptr : editor_.indexResult(source->second);
    append_->setEnabled(ready && index && std::holds_alternative<IndexedSource>(*index));
    relink_->setEnabled(ready && source != editor_.project().sources.end());
    play_->setEnabled(ready && !segments_.empty());
    split_->setEnabled(ready && !segments_.empty());
    remove_->setEnabled(ready && selected_.has_value());
    cut_->setEnabled(remove_->isEnabled());
    copy_->setEnabled(remove_->isEnabled());
    paste_->setEnabled(ready && editor_.hasClipboard());
    undo_->setEnabled(ready && editor_.canUndo());
    redo_->setEnabled(ready && editor_.canRedo());
    export_->setEnabled(ready && !segments_.empty());
    cancelAction_->setEnabled(!ready && !cancel_->load(std::memory_order_relaxed));
}

void Window::edit(const std::function<void()> &operation)
{
    if (cancel_)
        return;
    try {
        operation();
        refresh();
    } catch (const std::exception &error) {
        report(errorText(error));
    }
}

void Window::insert(const QString &id, size_t slot)
{
    edit([&] { selected_ = editor_.insertSource(id, slot); });
    if (selected_)
        select(*selected_);
}

void Window::select(size_t row)
{
    if (cancel_ || row >= clipCount())
        return;
    selected_ = row;
    timeline_->setSelected(row);
    if (row < segments_.size())
        seek(segments_[row].startUs, false);
    controls();
}

void Window::remove()
{
    if (!selected_)
        return;
    edit([&] {
        editor_.remove(*selected_);
        if (clipCount() == 0)
            selected_.reset();
        else
            selected_ = std::min(*selected_, clipCount() - 1);
    });
}

void Window::clipboard(bool cut)
{
    if (!selected_ || cancel_ || !editor_.copy(*selected_))
        return;
    if (cut)
        remove();
    else
        controls();
}

void Window::paste()
{
    if (!editor_.hasClipboard() || cancel_)
        return;
    size_t slot = selected_ ? *selected_ + 1 : clipCount();
    if (const auto row = segmentAt(positionUs_)) {
        const auto &segment = segments_[*row];
        slot = *row + (positionUs_ - segment.startUs >= segment.endUs - positionUs_ ? 1 : 0);
    }
    edit([&] { editor_.paste(slot); });
    select(slot);
}

void Window::split()
{
    if (const auto row = segmentAt(positionUs_)) {
        const auto &segment = segments_[*row];
        if (positionUs_ == segment.startUs || positionUs_ == segment.endUs) {
            report(tr("The playhead is already at a clip boundary."));
            return;
        }
        edit([&] {
            editor_.split(*row, segment.sourceStart + seconds(positionUs_ - segment.startUs));
            selected_ = *row + 1;
        });
        if (selected_)
            select(*selected_);
    }
}

void Window::move(size_t row, size_t slot)
{
    edit([&] { selected_ = editor_.move(row, slot); });
    if (selected_)
        select(*selected_);
}

void Window::trim(size_t row, bool in, double time)
{
    if (row >= segments_.size())
        return;
    edit([&] {
        const auto &segment = segments_[row];
        editor_.trim(row, in, segment.sourceStart + time - seconds(segment.startUs));
        selected_ = row;
    });
}

void Window::history(bool redo)
{
    edit([&] { redo ? editor_.redo() : editor_.undo(); });
}

std::optional<size_t> Window::segmentAt(qint64 time) const
{
    if (segments_.empty())
        return {};
    const auto segment = std::partition_point(segments_.begin(), segments_.end(),
                                              [time](const auto &item) { return item.endUs <= time; });
    return segment == segments_.end() ? segments_.size() - 1 : static_cast<size_t>(segment - segments_.begin());
}

void Window::keyframe(bool forward)
{
    if (cancel_ || segments_.empty())
        return;
    if (forward) {
        for (const auto &segment : segments_) {
            const auto next = std::upper_bound(segment.safePoints.begin(), segment.safePoints.end(), positionUs_);
            if (next != segment.safePoints.end()) {
                seek(*next, false);
                return;
            }
        }
        seek(segments_.back().endUs, false);
    } else {
        for (auto segment = segments_.rbegin(); segment != segments_.rend(); ++segment) {
            const auto previous = std::lower_bound(segment->safePoints.begin(), segment->safePoints.end(), positionUs_);
            if (previous != segment->safePoints.begin()) {
                seek(*std::prev(previous), false);
                return;
            }
        }
        seek(0, false);
    }
}

void Window::seek(qint64 time, bool playing)
{
    if (cancel_ || segments_.empty())
        return;
    positionUs_ = std::clamp(time, qint64{0}, segments_.back().endUs);
    timeline_->setPosition(seconds(positionUs_), playing);
    const auto row = *segmentAt(positionUs_);
    const auto &segment = segments_[row];
    const auto &clip = editor_.project().clips[row];
    try {
        const auto &index = editor_.index(clip.source);
        const auto frame = index.frameAt(clip, segment.sourceStart + seconds(positionUs_ - segment.startUs) + 0.0000005);
        if (current_ == row && frame >= 0) {
            preview_->seek(frame);
        } else {
            index.checkSource();
            current_ = row;
            preview_->open(segment.path, frame, playing, segment.sourceEnd);
        }
        setPlaying(playing);
    } catch (const std::exception &error) {
        current_.reset();
        preview_->stop();
        setPlaying(false);
        report(errorText(error));
    }
}

void Window::scrub(double time)
{
    if (!segments_.empty() && std::isfinite(time))
        seek(timelineTime(time, segments_.back().endUs), false);
}

void Window::togglePlayback()
{
    if (cancel_ || segments_.empty())
        return;
    if (playing_)
        setPlaying(false);
    else
        seek(positionUs_ >= segments_.back().endUs ? 0 : positionUs_, true);
}

void Window::setPlaying(bool playing)
{
    playing_ = playing;
    play_->setText(playing ? tr("Pause") : tr("Play"));
    preview_->setPlaying(playing);
}

void Window::advance()
{
    if (!playing_ || !current_)
        return;
    const auto next = *current_ + 1;
    if (next < segments_.size())
        seek(segments_[next].startUs, true);
    else {
        positionUs_ = segments_.back().endUs;
        timeline_->setPosition(seconds(positionUs_), true);
        setPlaying(false);
    }
}

void Window::clear()
{
    if (cancel_)
        return;
    editor_ = {};
    selected_.reset();
    positionUs_ = 0;
    timeline_->fit();
    refresh();
}

void Window::closeEvent(QCloseEvent *event)
{
    if (confirmDiscard()) {
        cancelJob();
        event->accept();
    } else {
        event->ignore();
    }
}

void Window::dragEnterEvent(QDragEnterEvent *event)
{
    if (!cancel_ && event->mimeData()->hasUrls())
        event->acceptProposedAction();
}

void Window::dropEvent(QDropEvent *event)
{
    QStringList paths;
    for (const auto &url : event->mimeData()->urls()) {
        if (url.isLocalFile())
            paths.append(url.toLocalFile());
    }
    if (!paths.isEmpty() && !cancel_) {
        importFiles(paths);
        event->acceptProposedAction();
    }
}
