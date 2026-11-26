// SPDX-License-Identifier: GPL-3.0-only
#include "system_info.h"
#include "preview.h"

#include <QClipboard>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QLabel>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QRegularExpression>
#include <QSysInfo>
#include <QThread>
#include <QVBoxLayout>

#include <memory>

using namespace Qt::StringLiterals;

namespace {
QString reportValue(QString value)
{
    // Accept a single line of product label text. Paths, addresses and unique
    // identifiers are outside the report schema even if a source returns one.
    static const QRegularExpression label(uR"(\A[\p{L}\p{M}\d .,/()+_~@%&®™:;\[\]-]{1,200}\z)"_s);
    static const QRegularExpression sensitive(
        uR"((?:^|\s)[^\s]*@[^\s]*\.[^\s]+|(?<! )/|/(?! )|(?<![\d.])(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?![\d.])|(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}|(?:[0-9a-f]{0,4}:){2,}[0-9a-f]{0,4}|[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12})"_s,
        QRegularExpression::CaseInsensitiveOption);
    value = value.trimmed();
    if (!label.match(value).hasMatch() || sensitive.match(value).hasMatch())
        return {};
    return value;
}

QString displaySession()
{
    const auto platform = QGuiApplication::platformName();
    if (platform.startsWith(u"wayland"))
        return u"Wayland"_s;
    if (platform == u"xcb") {
        if (qEnvironmentVariable("XDG_SESSION_TYPE").compare(u"wayland"_s, Qt::CaseInsensitive) == 0)
            return u"Wayland session (X11 Qt backend)"_s;
        return u"X11"_s;
    }
    if (platform == u"windows")
        return u"Windows"_s;
    return {};
}
}

QString SystemInformation::report() const
{
    QString result;
    const auto section = [&](const QString &title) {
        if (!result.isEmpty())
            result += u'\n';
        result += u"### "_s + title + u'\n';
    };
    const auto field = [&](const QString &label, const QStringList &values) {
        QStringList safe;
        for (const auto &value : values) {
            const auto clean = reportValue(value);
            if (!clean.isEmpty() && !safe.contains(clean))
                safe << clean;
        }
        result += u"- "_s + label + u": "_s +
            (safe.isEmpty() ? u"Unknown or unavailable"_s : safe.join(u"; "_s)) + u'\n';
    };

    section(u"Application"_s);
    field(u"SpoolCut version"_s, {version});
    field(u"Build revision"_s, {revision});
    field(u"Build architecture"_s, {architecture});
    field(u"Qt version"_s, {qt});
    section(u"Operating system"_s);
    field(u"OS and version"_s, {os});
    field(u"Linux distribution"_s, {distribution});
    field(u"Desktop environment"_s, {desktop});
    field(u"Display session"_s, {session});
    field(u"Virtualization"_s, {virtualization});
    section(u"Hardware"_s);
    field(u"CPU model"_s, {cpu});
    field(u"GPU model"_s, gpus);
    field(u"RAM (OS-reported)"_s, {ram});
    field(u"Graphics driver"_s, drivers);
    section(u"Multimedia"_s);
    field(u"Active preview renderer"_s, {renderer});
    field(u"OpenGL renderer"_s, {glRenderer});
    field(u"OpenGL version"_s, {glVersion});
    field(u"libmpv version"_s, {mpv});
    field(u"FFmpeg version"_s, {ffmpeg});
    field(u"FFprobe version"_s, {ffprobe});
    result.chop(1);
    return result;
}

void showSystemInformation(QWidget *parent, Preview *preview)
{
    auto *dialog = new QDialog(parent);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    dialog->setWindowTitle(QDialog::tr("System Information"));
    auto *layout = new QVBoxLayout(dialog);
    auto *notice = new QLabel(QDialog::tr("SpoolCut gathers this information locally. Nothing is "
        "automatically transmitted. Copy and share it only if you choose."), dialog);
    notice->setWordWrap(true);
    layout->addWidget(notice);
    auto *report = new QPlainTextEdit(dialog);
    report->setReadOnly(true);
    report->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    report->setPlainText(QDialog::tr("Gathering system information…"));
    layout->addWidget(report);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Close, dialog);
    auto *copy = buttons->addButton(QDialog::tr("Copy Information"), QDialogButtonBox::ActionRole);
    copy->setEnabled(false);
    QObject::connect(buttons, &QDialogButtonBox::rejected, dialog, &QDialog::reject);
    QObject::connect(copy, &QPushButton::clicked, dialog, [report] {
        QGuiApplication::clipboard()->setText(report->toPlainText());
    });
    layout->addWidget(buttons);

    const auto collected = std::make_shared<SystemInformation>();
    auto *probe = QThread::create([collected, appDirectory = QCoreApplication::applicationDirPath()] {
        *collected = collectSystemInformation(appDirectory);
    });
    QObject::connect(probe, &QThread::finished, probe, &QObject::deleteLater);
    QObject::connect(probe, &QThread::finished, dialog, [collected, report, copy, preview] {
        collected->version = QCoreApplication::applicationVersion();
        collected->revision = QStringLiteral(SPOOLCUT_BUILD_REVISION);
        collected->architecture = QSysInfo::buildCpuArchitecture();
        collected->qt = QString::fromLatin1(qVersion());
        collected->session = displaySession();
        const auto updateReport = [collected, report, preview] {
            collected->renderer = preview->activeRenderer();
            collected->glRenderer = preview->openGlRenderer();
            collected->glVersion = preview->openGlVersion();
            report->setPlainText(collected->report());
        };
        QObject::connect(preview, &Preview::rendererChanged, report, updateReport);
        updateReport();
        copy->setEnabled(true);
    });

    dialog->resize(660, 540);
    dialog->open();
    probe->start();
}
