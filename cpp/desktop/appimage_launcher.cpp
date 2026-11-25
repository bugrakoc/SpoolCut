#include "appimage_launcher.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QLockFile>
#include <QSaveFile>
#include <QStandardPaths>
#include <QTemporaryFile>

namespace {
constexpr auto managed = "X-SpoolCut-AppImage=true";

QString desktopEntryPath() {
    return QStringLiteral("applications/") + QLatin1String(spoolCutDesktopId) + ".desktop";
}

QString entryValue(QString value) {
    return value.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
}

QString execLine(QString image) {
    const bool equalSign = image.contains('=');
    image.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$").replace('`', "\\`").replace('%', "%%");
    const QString arguments = '"' + image + "\" %F";
    // The desktop entry format disallows '=' in the executable token.
    if (equalSign)
        return entryValue(QStringLiteral(R"(/bin/sh -c "exec \"\$@\"" SpoolCut )") + arguments);
    return entryValue(arguments);
}

bool save(const QString &path, const QByteArray &data) {
    QFile existing(path);
    if (existing.open(QIODevice::ReadOnly) && existing.readAll() == data)
        return true;
    existing.close();
    QSaveFile staged(path);
    return staged.open(QIODevice::WriteOnly) && staged.write(data) == data.size() && staged.commit();
}
}

QString installAppImageLauncher(const QString &image, const QString &dataHome) {
    const QDir home(dataHome);
    const QString path = home.filePath(desktopEntryPath());
    const QString icon = home.filePath(QStringLiteral("icons/hicolor/256x256/apps/")
                                      + QLatin1String(spoolCutDesktopId) + ".png");

    if (!home.mkpath("applications"))
        return QStringLiteral("Could not create the applications directory");
    QLockFile lock(path + ".lock");
    if (!lock.tryLock(0))
        return {};

    QFile old(path);
    const QFileInfo oldInfo(path);
    const bool replacing = oldInfo.exists();
    if (oldInfo.isSymLink())
        return {};
    if (replacing && (!old.open(QIODevice::ReadOnly)
                     || !old.readAll().split('\n').contains(managed)))
        return {};
    old.close();

    QFile iconResource(":/io.github.bugrakoc.SpoolCut.png");
    if (!home.mkpath("icons/hicolor/256x256/apps")
        || !iconResource.open(QIODevice::ReadOnly) || !save(icon, iconResource.readAll()))
        return QStringLiteral("Could not install the SpoolCut application icon");

    const QByteArray entry = QStringLiteral("[Desktop Entry]\nType=Application\nName=SpoolCut\n"
        "Exec=%1\nTryExec=%2\nIcon=%3\nCategories=AudioVideo;Video;\nTerminal=false\n%4\n")
        .arg(execLine(image), entryValue(image), entryValue(icon), QLatin1String(managed)).toUtf8();
    if (replacing && save(path, entry))
        return {};
    if (!replacing) {
        QTemporaryFile staged(path + ".XXXXXX");
        if (staged.open() && staged.write(entry) == entry.size() && staged.flush()
            && staged.rename(path)) {
            staged.setAutoRemove(false);
            return {};
        }
    }
    return QStringLiteral("Could not install the SpoolCut desktop launcher");
}

QString registerAppImageLauncher() {
    const QFileInfo image(qEnvironmentVariable("APPIMAGE"));
    const QString bundle = qEnvironmentVariable("APPDIR");
    if (bundle.isEmpty() || !image.isAbsolute() || !image.isFile())
        return {};
    const QString executable = QDir(bundle).relativeFilePath(QCoreApplication::applicationFilePath());
    if (executable != QStringLiteral("bin/spoolcut-desktop"))
        return {};
    const QString data = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
    if (data.isEmpty())
        return QStringLiteral("Could not locate the desktop data directory");
    const auto installed = QStandardPaths::locate(QStandardPaths::GenericDataLocation, desktopEntryPath());
    if (!installed.isEmpty() && installed != QDir(data).filePath(desktopEntryPath()))
        return {};
    return installAppImageLauncher(image.absoluteFilePath(), data);
}
