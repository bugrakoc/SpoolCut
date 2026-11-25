// SPDX-License-Identifier: GPL-3.0-only
#include "timeline.h"

#include <QApplication>
#include <QMimeData>
#include <QMouseEvent>
#include <QPainter>
#include <QPalette>
#include <QWheelEvent>

#include <algorithm>
#include <cmath>

namespace {
constexpr double margin = 12;
constexpr int trackY = 42;
constexpr auto sourceType = "application/x-spoolcut-source";

QString timeLabel(double seconds, int decimals)
{
    const auto minutes = static_cast<qint64>(seconds / 60);
    return QStringLiteral("%1:%2").arg(minutes).arg(seconds - static_cast<double>(minutes) * 60,
                                                     decimals ? 3 + decimals : 2, 'f', decimals, '0');
}
}

Timeline::Timeline(QWidget *parent) : QWidget(parent)
{
    setMinimumHeight(112);
    setAccessibleName(tr("Timeline"));
    setMouseTracking(true);
    setAcceptDrops(true);
}

void Timeline::setClips(std::vector<PreviewClip> clips)
{
    clips_ = std::move(clips);
    duration_ = 0;
    starts_.clear();
    safePoints_.clear();
    for (const auto &clip : clips_) {
        starts_.push_back(duration_);
        for (const auto point : clip.safeOffsets)
            safePoints_.push_back(duration_ + point);
        duration_ += clip.duration;
    }
    position_ = std::min(position_, duration_);
    cancelDrag();
    clampOffset();
    update();
}

void Timeline::setSelected(std::optional<size_t> row)
{
    selected_ = row;
    update();
}

void Timeline::setPosition(double seconds, bool follow)
{
    position_ = std::clamp(seconds, 0.0, duration_);
    if (follow) {
        const auto visible = (width() - 2 * margin) / scale();
        if (position_ < offset_ || position_ > offset_ + visible)
            offset_ = position_ - 0.15 * visible;
        clampOffset();
    }
    update();
}

double Timeline::scale() const
{
    return std::max(1.0, width() - 2 * margin) * zoom_ / std::max(0.001, duration_);
}

double Timeline::xAt(double seconds) const { return margin + (seconds - offset_) * scale(); }
double Timeline::timeAt(double x) const { return offset_ + (x - margin) / scale(); }

void Timeline::clampOffset()
{
    offset_ = std::clamp(offset_, 0.0, std::max(0.0, duration_ - (width() - 2 * margin) / scale()));
}

void Timeline::zoom(double factor) { zoomAt(factor, xAt(position_)); }

void Timeline::zoomAt(double factor, double x)
{
    x = std::clamp(x, margin, std::max(margin, width() - margin));
    const double anchor = timeAt(x);
    zoom_ = std::clamp(zoom_ * factor, 1.0, 4096.0);
    offset_ = anchor - (x - margin) / scale();
    clampOffset();
    update();
}

void Timeline::fit()
{
    zoom_ = 1;
    offset_ = 0;
    update();
}

void Timeline::cancelDrag()
{
    drag_.reset();
    draft_.reset();
    unsetCursor();
    update();
}

void Timeline::resizeEvent(QResizeEvent *) { clampOffset(); }

void Timeline::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.fillRect(rect(), palette().window());
    if (clips_.empty())
        return;
    const double desired = 84 / scale();
    const double unit = std::pow(10.0, std::floor(std::log10(desired)));
    const double tick = (desired / unit <= 2 ? 2 : (desired / unit <= 5 ? 5 : 10)) * unit;
    const int decimals = std::clamp(static_cast<int>(std::ceil(-std::log10(tick))), 0, 6);
    p.setPen(palette().windowText().color());
    for (double t = std::ceil(offset_ / tick) * tick; t <= duration_ && xAt(t) <= width(); t += tick) {
        const qreal x = xAt(t);
        p.drawLine(QPointF(x, trackY - 9), QPointF(x, trackY - 2));
        p.drawText(QPointF(x + 3, trackY - 13), timeLabel(t, decimals));
    }
    p.setClipRect(QRectF(margin, trackY, std::max(1.0, width() - 2 * margin), height() - trackY));
    for (size_t row = 0; row < clips_.size(); ++row) {
        const auto &clip = clips_[row];
        const auto start = starts_[row];
        const double x = xAt(start);
        const double end = xAt(start + clip.duration);
        if (end < margin || x > width() - margin)
            continue;
        const auto span = QRectF(x, trackY, std::max(0.0, end - x - 1), height() - trackY - 13);
        const bool active = selected_ == row;
        const auto fill = active ? palette().highlight() : palette().button();
        p.fillRect(span, fill);
        p.setPen(active ? palette().highlightedText().color() : palette().buttonText().color());
        const auto textRect = span.intersected(rect()).adjusted(7, 0, -5, 0);
        p.drawText(textRect, Qt::AlignVCenter,
                   p.fontMetrics().elidedText(clip.label, Qt::ElideRight,
                                              std::max(0, static_cast<int>(textRect.width()))));
        const auto first = std::lower_bound(clip.safeOffsets.begin(), clip.safeOffsets.end(), offset_ - start);
        double lastMark = margin - 5;
        for (auto point = first; point != clip.safeOffsets.end(); ++point) {
            const auto mark = xAt(start + *point);
            if (mark > width() - margin)
                break;
            if (mark > lastMark + 4) {
                p.drawLine(QPointF(mark, span.bottom() - 8), QPointF(mark, span.bottom()));
                lastMark = mark;
            }
        }
    }
    p.setClipping(false);
    if (draft_) {
        p.setPen(QPen(palette().highlight().color(), 2, Qt::DashLine));
        p.drawLine(QPointF(*draft_, trackY), QPointF(*draft_, height() - 7));
    }
    const auto x = xAt(position_);
    if (x >= margin && x <= width() - margin) {
        p.setPen(QPen(palette().windowText().color(), 2));
        p.drawLine(QPointF(x, 10), QPointF(x, height() - 7));
    }
}

double Timeline::nearestSafe(double time) const
{
    if (safePoints_.empty())
        return time;
    const auto next = std::lower_bound(safePoints_.begin(), safePoints_.end(), time);
    if (next == safePoints_.begin())
        return *next;
    if (next == safePoints_.end() || time - *std::prev(next) <= *next - time)
        return *std::prev(next);
    return *next;
}

size_t Timeline::slotAt(double x) const
{
    const auto time = timeAt(x);
    const auto after = std::upper_bound(starts_.begin(), starts_.end(), time);
    if (after == starts_.begin())
        return 0;
    const auto row = static_cast<size_t>(after - starts_.begin() - 1);
    return row + (time >= starts_[row] + clips_[row].duration / 2 ? 1 : 0);
}

std::optional<Timeline::Gesture> Timeline::hit(double x) const
{
    const auto time = timeAt(x);
    if (time < 0 || time > duration_ || starts_.empty())
        return {};
    const auto after = std::upper_bound(starts_.begin(), starts_.end(), time);
    const auto row = static_cast<size_t>(after - starts_.begin() - 1);
    const auto start = xAt(starts_[row]);
    const auto end = xAt(starts_[row] + clips_[row].duration);
    const auto edge = std::min(8.0, (end - start) / 3);
    if (x - start <= edge)
        return Gesture{Drag::TrimIn, row, x};
    if (end - x <= edge)
        return Gesture{Drag::TrimOut, row, x};
    return Gesture{Drag::Move, row, x};
}

void Timeline::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton || clips_.empty())
        return;
    const auto x = event->position().x();
    if (event->position().y() < trackY) {
        drag_ = Gesture{Drag::Scrub};
        emit scrubbed(nearestSafe(std::clamp(timeAt(x), 0.0, duration_)));
    } else if (const auto point = hit(x)) {
        drag_ = point;
        emit selected(point->row);
    }
}

void Timeline::mouseMoveEvent(QMouseEvent *event)
{
    const auto x = event->position().x();
    if (drag_) {
        if (drag_->type == Drag::Scrub) {
            emit scrubbed(std::clamp(timeAt(x), 0.0, duration_));
        } else if (std::abs(x - drag_->x) >= QApplication::startDragDistance()) {
            const auto slot = slotAt(x);
            draft_ = drag_->type == Drag::Move
                         ? xAt(slot < starts_.size() ? starts_[slot] : duration_)
                         : std::clamp(x, margin, std::max(margin, width() - margin));
            update();
        }
    } else if (event->position().y() < trackY) {
        setCursor(Qt::CrossCursor);
    } else if (const auto point = hit(x)) {
        setCursor(point->type == Drag::Move ? Qt::OpenHandCursor : Qt::SizeHorCursor);
    } else {
        unsetCursor();
    }
}

void Timeline::mouseReleaseEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton || !drag_)
        return;
    const auto gesture = *drag_;
    const bool movedFar = std::abs(event->position().x() - gesture.x) >= QApplication::startDragDistance();
    cancelDrag();
    if (!movedFar || gesture.type == Drag::Scrub)
        return;
    const auto x = event->position().x();
    if (gesture.type == Drag::Move)
        emit moved(gesture.row, slotAt(x));
    else
        emit trimmed(gesture.row, gesture.type == Drag::TrimIn, timeAt(x));
}

void Timeline::wheelEvent(QWheelEvent *event)
{
    if (drag_)
        return;
    const auto pixel = event->pixelDelta();
    const auto angular = event->angleDelta();
    const auto delta = [](QPoint amount) { return amount.x() != 0 ? amount.x() : amount.y(); };
    if (event->modifiers() & Qt::ControlModifier) {
        const auto steps = pixel.isNull() ? delta(angular) / 120.0 : delta(pixel) / 45.0;
        zoomAt(std::pow(1.35, steps), event->position().x());
    } else {
        offset_ -= (pixel.isNull() ? delta(angular) * 65.0 / 120 : delta(pixel)) / scale();
        clampOffset();
        update();
    }
    event->accept();
}

void Timeline::dragEnterEvent(QDragEnterEvent *event)
{
    if (event->mimeData()->hasFormat(sourceType))
        event->acceptProposedAction();
}

void Timeline::dragMoveEvent(QDragMoveEvent *event)
{
    const auto slot = slotAt(event->position().x());
    draft_ = xAt(slot < starts_.size() ? starts_[slot] : duration_);
    update();
    event->acceptProposedAction();
}

void Timeline::dragLeaveEvent(QDragLeaveEvent *) { cancelDrag(); }

void Timeline::dropEvent(QDropEvent *event)
{
    cancelDrag();
    const auto id = QString::fromUtf8(event->mimeData()->data(sourceType));
    if (!id.isEmpty()) {
        emit sourceDropped(id, slotAt(event->position().x()));
        event->acceptProposedAction();
    }
}
