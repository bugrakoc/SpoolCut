// SPDX-License-Identifier: GPL-3.0-only
#include "timeline.h"

#include <QApplication>
#include <QMimeData>
#include <QMouseEvent>
#include <QFontDatabase>
#include <QPainter>
#include <QPalette>
#include <QWheelEvent>

#include <algorithm>
#include <array>
#include <cmath>

namespace {
constexpr double margin = 12;
constexpr int rulerHeight = 36;
constexpr int trackY = 46;
constexpr int clipHeight = 55;
constexpr auto sourceType = "application/x-spoolcut-source";

QString timeLabel(double seconds, int decimals)
{
    const auto factor = std::pow(10.0, decimals);
    const auto rounded = std::round(seconds * factor) / factor;
    const auto minutes = static_cast<qint64>(rounded / 60);
    return QStringLiteral("%1:%2").arg(minutes).arg(rounded - static_cast<double>(minutes) * 60,
                                                     decimals ? 3 + decimals : 2, 'f', decimals, '0');
}
}

QColor sourceColor(size_t row)
{
    static constexpr std::array<QRgb, 6> colors{0x2c5d85, 0x256454, 0x634986, 0x805b30, 0x3a6171, 0x814a63};
    return QColor::fromRgb(colors[row % colors.size()]);
}

Timeline::Timeline(QWidget *parent) : QWidget(parent)
{
    setMinimumHeight(112);
    setAccessibleName(tr("Timeline"));
    setToolTip(tr("Click to snap to a safe frame. Drag the ruler to scrub. Scroll to pan, or Ctrl+scroll to zoom."));
    setMouseTracking(true);
    setAcceptDrops(true);
}

void Timeline::setClips(std::vector<PreviewClip> clips, const QString &timingProblem)
{
    timingProblem_ = timingProblem;
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
    if (follow && hasTiming()) {
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
    zoom_ = std::clamp(zoom_ * factor, 1.0 / 16, 4096.0);
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
    const bool dark = palette().color(QPalette::Window).lightness() < 128;
    const QColor track(dark ? 0x171d26 : 0xf0f3f7);
    const QColor ruler(dark ? 0x242c38 : 0xe4eaf1);
    const QColor grid(dark ? 0x343e4c : 0xd3dde7);
    const QColor text(dark ? 0xc7d2e0 : 0x415064);
    const QColor playhead(dark ? 0xff9a6e : 0xd24d2a);
    const auto bodyHeight = std::min(clipHeight, height() - trackY - 8);
    p.fillRect(rect(), track);
    p.fillRect(0, 0, width(), rulerHeight, ruler);
    p.setPen(grid);
    p.drawLine(0, rulerHeight - 1, width(), rulerHeight - 1);
    if (clips_.empty()) {
        p.setPen(text);
        p.drawText(QRect(0, rulerHeight, width(), height() - rulerHeight), Qt::AlignCenter,
                   tr("Add clips from the media bin"));
        return;
    }
    if (hasTiming()) {
        const double desired = 92 / scale();
        const double unit = std::pow(10.0, std::floor(std::log10(desired)));
        const double tick = (desired / unit <= 2 ? 2 : (desired / unit <= 5 ? 5 : 10)) * unit;
        const int decimals = std::clamp(static_cast<int>(std::ceil(-std::log10(tick))), 0, 6);
        const auto labelFont = QFontDatabase::systemFont(QFontDatabase::FixedFont);
        p.save();
        p.setFont(labelFont);
        for (double t = std::floor(offset_ / tick) * tick; xAt(t) <= width(); t += tick) {
            const qreal x = xAt(t);
            p.setPen(grid);
            p.drawLine(QPointF(x, rulerHeight - 13), QPointF(x, height()));
            for (int minor = 1; minor < 5; ++minor) {
                const auto mark = xAt(t + tick * minor / 5);
                p.drawLine(QPointF(mark, rulerHeight - 6), QPointF(mark, rulerHeight - 1));
            }
            p.setPen(text);
            p.drawText(QPointF(x + 4, 18), timeLabel(t, decimals));
        }
        p.restore();
    } else {
        p.setPen(text);
        p.drawText(QRect(12, 0, width() - 24, rulerHeight), Qt::AlignVCenter,
                   timingProblem_);
    }

    p.save();
    p.setRenderHint(QPainter::Antialiasing);
    p.setClipRect(QRectF(margin, trackY, std::max(1.0, width() - 2 * margin), height() - trackY));
    for (size_t row = 0; row < clips_.size(); ++row) {
        const auto &clip = clips_[row];
        const auto start = starts_[row];
        const double x = xAt(start);
        const double end = xAt(start + clip.duration);
        if (end < margin)
            continue;
        if (x > width() - margin)
            break;
        const auto span = QRectF(x + 0.5, trackY + 0.5, std::max(1.0, end - x - 2), bodyHeight - 1);
        const bool active = selected_ == row;
        const bool available = clip.unavailable.isEmpty();
        const auto color = available ? (clip.color.isValid() ? clip.color : sourceColor(row))
                                     : UnavailableMediaStyle::fill;
        p.setBrush(color);
        p.setPen(available ? QPen(active ? QColor(0xb4d9ff) : color.lighter(135), active ? 2 : 1)
                           : QPen(UnavailableMediaStyle::border, active ? 2 : 1, Qt::DashLine));
        p.drawRoundedRect(span, 5, 5);
        p.save();
        p.setClipRect(span.adjusted(2, 1, -2, -1), Qt::IntersectClip);
        const auto visible = span.intersected(QRectF(rect()));
        const auto label = visible.adjusted(10, 4, -8, available ? -13 : -23);
        p.setPen(Qt::white);
        auto titleFont = p.font();
        titleFont.setWeight(QFont::DemiBold);
        p.setFont(titleFont);
        p.drawText(label, Qt::AlignVCenter,
                   p.fontMetrics().elidedText(clip.label, Qt::ElideRight,
                                              std::max(0, static_cast<int>(label.width()))));
        if (!available) {
            p.setFont(font());
            p.setPen(UnavailableMediaStyle::detail);
            const auto note = visible.adjusted(10, 23, -8, -3);
            p.drawText(note, Qt::AlignVCenter,
                       p.fontMetrics().elidedText(clip.unavailable, Qt::ElideRight,
                                                  std::max(0, static_cast<int>(note.width()))));
        }
        const auto first = std::lower_bound(clip.safeOffsets.begin(), clip.safeOffsets.end(), offset_ - start);
        double lastMark = margin - 5;
        p.setPen(QColor(218, 233, 248, 170));
        for (auto point = first; point != clip.safeOffsets.end(); ++point) {
            const auto mark = xAt(start + *point);
            if (mark > width() - margin)
                break;
            if (mark > lastMark + 4) {
                p.drawLine(QPointF(mark, span.bottom() - 9), QPointF(mark, span.bottom() - 4));
                lastMark = mark;
            }
        }
        if (hasTiming() && span.width() >= 26) {
            p.setPen(Qt::NoPen);
            p.setBrush(QColor(255, 255, 255, active ? 150 : 66));
            p.drawRoundedRect(QRectF(span.left() + 3, span.center().y() - 8, 2, 16), 1, 1);
            p.drawRoundedRect(QRectF(span.right() - 5, span.center().y() - 8, 2, 16), 1, 1);
        }
        p.restore();
    }
    p.restore();
    if (draft_) {
        p.setPen(QPen(palette().highlight().color(), 2, Qt::DashLine));
        p.drawLine(QPointF(*draft_, trackY - 4), QPointF(*draft_, trackY + bodyHeight + 4));
    }
    const auto x = xAt(position_);
    if (hasTiming() && x >= margin && x <= width() - margin) {
        p.setRenderHint(QPainter::Antialiasing);
        p.setPen(QPen(playhead, 1.75));
        p.drawLine(QPointF(x, 12), QPointF(x, height() - 5));
        p.setPen(Qt::NoPen);
        p.setBrush(playhead);
        p.drawPolygon(QPolygonF{{x - 6, 0}, {x + 6, 0}, {x + 6, 8}, {x, 14}, {x - 6, 8}});
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
    if (hasTiming() && x - start <= edge)
        return Gesture{Drag::TrimIn, row, x};
    if (hasTiming() && end - x <= edge)
        return Gesture{Drag::TrimOut, row, x};
    return Gesture{Drag::Move, row, x};
}

void Timeline::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton || clips_.empty())
        return;
    const auto x = event->position().x();
    if (event->position().y() < trackY) {
        if (!hasTiming())
            return;
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
    } else if (hasTiming() && event->position().y() < trackY) {
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
