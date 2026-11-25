#include "timeline.h"

#include <QMouseEvent>
#include <QPainter>
#include <QPalette>
#include <QWheelEvent>
#include <algorithm>
#include <cmath>

namespace {
constexpr double margin = 12;
constexpr int trackY = 42;

QString timeLabel(double seconds, int decimals) {
    const int minutes = static_cast<int>(seconds / 60);
    return QStringLiteral("%1:%2").arg(minutes).arg(seconds - minutes * 60,
                                                     decimals ? 4 : 2, 'f', decimals, '0');
}
}

Timeline::Timeline(QWidget *parent) : QWidget(parent) {
    setMinimumHeight(112);
    setAccessibleName(tr("Preview timeline"));
}

void Timeline::append(PreviewClip clip) {
    duration_ += clip.duration;
    clips_.push_back(std::move(clip));
    clampOffset();
    update();
}

void Timeline::clear() {
    clips_.clear();
    duration_ = position_ = 0;
    fit();
}

void Timeline::setPosition(double seconds) {
    position_ = std::clamp(seconds, 0.0, duration_);
    update();
}

double Timeline::scale() const {
    return std::max(1.0, width() - 2 * margin) * zoom_ / std::max(0.001, duration_);
}

double Timeline::xAt(double seconds) const { return margin + (seconds - offset_) * scale(); }

void Timeline::clampOffset() {
    offset_ = std::clamp(offset_, 0.0, std::max(0.0, duration_ - (width() - 2 * margin) / scale()));
}

void Timeline::zoom(double factor) {
    const double x = std::clamp(xAt(position_), margin, std::max(margin, width() - margin));
    const double anchor = offset_ + (x - margin) / scale();
    zoom_ = std::clamp(zoom_ * factor, 1.0, 128.0);
    offset_ = anchor - (x - margin) / scale();
    clampOffset();
    update();
}

void Timeline::fit() {
    zoom_ = 1;
    offset_ = 0;
    update();
}

void Timeline::resizeEvent(QResizeEvent *) { clampOffset(); }

void Timeline::paintEvent(QPaintEvent *) {
    QPainter p(this);
    p.fillRect(rect(), palette().window());
    if (clips_.empty())
        return;
    const auto pixelsPerSecond = scale();
    const double desired = 84 / pixelsPerSecond;
    const double unit = std::pow(10.0, std::floor(std::log10(desired)));
    const double tick = (desired / unit <= 2 ? 2 : (desired / unit <= 5 ? 5 : 10)) * unit;
    const int decimals = tick < 1 ? 1 : 0;
    p.setPen(palette().windowText().color());
    for (double t = std::ceil(offset_ / tick) * tick; t <= duration_ && xAt(t) <= width(); t += tick) {
        const qreal x = xAt(t);
        p.drawLine(QPointF(x, trackY - 9), QPointF(x, trackY - 2));
        p.drawText(QPointF(x + 3, trackY - 13), timeLabel(t, decimals));
    }
    p.setClipRect(QRectF(margin, trackY, std::max(1.0, width() - 2 * margin), height() - trackY));
    double start = 0;
    for (const auto &clip : clips_) {
        const double x = xAt(start);
        start += clip.duration;
        const double end = xAt(start);
        if (end < margin || x > width() - margin)
            continue;
        const auto span = QRectF(x, trackY, std::max(0.0, end - x - 1), height() - trackY - 13);
        p.fillRect(span, palette().highlight());
        p.setPen(palette().highlightedText().color());
        const auto textRect = span.intersected(rect()).adjusted(7, 0, -5, 0);
        p.drawText(textRect, Qt::AlignVCenter,
                   p.fontMetrics().elidedText(clip.label, Qt::ElideRight,
                                              std::max(0, int(textRect.width()))));
    }
    p.setClipping(false);
    const auto x = xAt(position_);
    if (x >= margin && x <= width() - margin) {
        p.setPen(QPen(palette().windowText().color(), 2));
        p.drawLine(QPointF(x, 10), QPointF(x, height() - 7));
    }
}

void Timeline::scrub(double x) {
    if (!clips_.empty())
        emit scrubbed(std::clamp(offset_ + (x - margin) / scale(), 0.0, duration_));
}

void Timeline::mousePressEvent(QMouseEvent *event) {
    if (event->button() == Qt::LeftButton)
        scrub(event->position().x());
}

void Timeline::mouseMoveEvent(QMouseEvent *event) {
    if (event->buttons() & Qt::LeftButton)
        scrub(event->position().x());
}

void Timeline::wheelEvent(QWheelEvent *event) {
    const auto delta = event->angleDelta().y();
    if (event->modifiers() & Qt::ControlModifier) {
        zoom(std::pow(1.2, delta / 120.0));
    } else {
        offset_ -= delta / scale();
        clampOffset();
        update();
    }
}
