// SPDX-License-Identifier: GPL-3.0-only
#pragma once

#include <QByteArray>
#include <QString>
#include <QStringList>

class Preview;
class QWidget;

struct SystemInformation {
    QString version;
    QString revision;
    QString architecture;
    QString qt;
    QString os;
    QString distribution;
    QString desktop;
    QString session;
    QString virtualization;
    QString cpu;
    QStringList gpus;
    QString ram;
    QStringList drivers;
    QString renderer;
    QString glRenderer;
    QString glVersion;
    QString mpv;
    QString ffmpeg;
    QString ffprobe;

    QString report() const;
};

SystemInformation collectSystemInformation(const QString &appDirectory);
QString mediaToolVersion(const QByteArray &output, const QString &tool);
void showSystemInformation(QWidget *parent, Preview *preview);
