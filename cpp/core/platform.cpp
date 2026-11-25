// SPDX-License-Identifier: GPL-3.0-only
#include "detail.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QProcess>
#include <QTemporaryDir>

#include <cerrno>
#include <cstring>

#ifdef Q_OS_WIN
#include <io.h>
#include <qt_windows.h>
#else
#include <sys/stat.h>
#include <unistd.h>
#endif

namespace spoolcut::detail {
namespace {

QString systemError()
{
#ifdef Q_OS_WIN
    const auto code = GetLastError();
    wchar_t buffer[512]{};
    const auto length = FormatMessageW(FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
                                       nullptr, code, 0, buffer, 512, nullptr);
    if (length != 0)
        return QString::fromWCharArray(buffer, static_cast<qsizetype>(length)).trimmed();
    return QStringLiteral("Windows error %1").arg(code);
#else
    return QString::fromLocal8Bit(std::strerror(errno));
#endif
}

void validPath(const QString &path)
{
    require(!path.isEmpty() && !path.contains(QChar::Null), QStringLiteral("invalid file path"));
}

bool executable(const QString &path)
{
    const QFileInfo info(path);
#ifdef Q_OS_WIN
    return info.isFile();
#else
    return info.isFile() && info.isExecutable();
#endif
}

} // namespace

QString canonicalPath(const QString &path)
{
    validPath(path);
    const auto resolved = QFileInfo(path).canonicalFilePath();
    require(!resolved.isEmpty(), QStringLiteral("cannot resolve %1").arg(path));
    return resolved;
}

QString mediaPath(const QString &path)
{
    validPath(path);
#ifdef Q_OS_WIN
    // Extended-length Windows paths also work with the native FFmpeg build.
    const auto native = QDir::toNativeSeparators(QFileInfo(path).absoluteFilePath());
    if (native.startsWith(QStringLiteral("\\\\?\\")))
        return native;
    if (native.startsWith(QStringLiteral("\\\\")))
        return QStringLiteral("\\\\?\\UNC\\") + native.mid(2);
    return QStringLiteral("\\\\?\\") + native;
#else
    return path;
#endif
}

void publish(const QString &staged, const QString &destination)
{
    validPath(staged);
    validPath(destination);
#ifdef Q_OS_WIN
    const auto from = mediaPath(staged).toStdWString();
    const auto to = mediaPath(destination).toStdWString();
    // Same-volume rename. No replacement or cross-volume copy flags.
    if (!MoveFileExW(from.c_str(), to.c_str(), 0))
        throw Error(systemError());
#else
    const auto from = QFile::encodeName(staged);
    const auto to = QFile::encodeName(destination);
    if (::link(from.constData(), to.constData()) != 0)
        throw Error(systemError());
#endif
}

void replaceProject(const QString &staged, const QString &destination)
{
    validPath(staged);
    validPath(destination);
#ifdef Q_OS_WIN
    const auto from = mediaPath(staged).toStdWString();
    const auto to = mediaPath(destination).toStdWString();
    if (!MoveFileExW(from.c_str(), to.c_str(), MOVEFILE_REPLACE_EXISTING))
        throw Error(systemError());
#else
    const auto from = QFile::encodeName(staged);
    const auto to = QFile::encodeName(destination);
    if (::rename(from.constData(), to.constData()) != 0)
        throw Error(systemError());
#endif
}

SourceIdentity SourceIdentity::read(QFile &file)
{
    SourceIdentity result{};
#ifdef Q_OS_WIN
    const auto raw = _get_osfhandle(file.handle());
    require(raw != -1, QStringLiteral("invalid source file handle"));
    const auto handle = reinterpret_cast<HANDLE>(raw);
    require(GetFileType(handle) == FILE_TYPE_DISK, QStringLiteral("source must be a regular file"));
    BY_HANDLE_FILE_INFORMATION legacy{};
    if (!GetFileInformationByHandle(handle, &legacy))
        throw Error(systemError());
    require((legacy.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) == 0,
            QStringLiteral("source must be a regular file"));
    FILE_ID_INFO extended{};
    if (GetFileInformationByHandleEx(handle, FileIdInfo, &extended, sizeof(extended))) {
        result.extended = true;
        result.volume = extended.VolumeSerialNumber;
        std::copy_n(extended.FileId.Identifier, result.id.size(), result.id.begin());
    } else {
        result.volume = legacy.dwVolumeSerialNumber;
        const quint64 index = (quint64{legacy.nFileIndexHigh} << 32) | legacy.nFileIndexLow;
        std::memcpy(result.id.data(), &index, sizeof(index));
    }
    result.size = (quint64{legacy.nFileSizeHigh} << 32) | legacy.nFileSizeLow;
    result.modified = legacy.ftLastWriteTime.dwHighDateTime;
    result.modifiedFraction = legacy.ftLastWriteTime.dwLowDateTime;
#else
    struct stat status{};
    if (::fstat(file.handle(), &status) != 0)
        throw Error(systemError());
    require(S_ISREG(status.st_mode), QStringLiteral("source must be a regular file"));
    result.volume = status.st_dev;
    static_assert(sizeof(status.st_ino) <= sizeof(result.id));
    std::memcpy(result.id.data(), &status.st_ino, sizeof(status.st_ino));
    result.size = static_cast<quint64>(status.st_size);
#if defined(Q_OS_DARWIN)
    result.modified = status.st_mtimespec.tv_sec;
    result.modifiedFraction = status.st_mtimespec.tv_nsec;
#else
    result.modified = status.st_mtim.tv_sec;
    result.modifiedFraction = status.st_mtim.tv_nsec;
#endif
#endif
    return result;
}

void checkCancelled(const Cancel &cancel)
{
    require(!cancel.load(std::memory_order_relaxed), QStringLiteral("cancelled"));
}

QString findTool(const QString &name, const QString &appDirectory)
{
    const auto setting = QStringLiteral("SPOOLCUT_") + name.toUpper();
#ifdef Q_OS_WIN
    const auto filename = name + QStringLiteral(".exe");
#else
    const auto filename = name;
#endif
    const auto settingBytes = setting.toLatin1();
    if (qEnvironmentVariableIsSet(settingBytes.constData())) {
        const auto override = qEnvironmentVariable(settingBytes.constData());
        require(executable(override), QStringLiteral("%1 does not point to an executable file: %2").arg(setting, override));
        return QFileInfo(override).absoluteFilePath();
    }
    QStringList directories;
    if (!appDirectory.isEmpty())
        directories << appDirectory << QDir(appDirectory).filePath(QStringLiteral("bin"));
    directories += qEnvironmentVariable("PATH").split(QDir::listSeparator(), Qt::SkipEmptyParts);
    for (const auto &directory : directories) {
        const auto candidate = QDir(directory).absoluteFilePath(filename);
        if (executable(candidate))
            return candidate;
    }
    throw Error(QStringLiteral("could not find %1; set %2, install it alongside SpoolCut, or add it to PATH")
                    .arg(filename, setting));
}

QByteArray runTool(const QString &name, const QStringList &arguments, const Cancel &cancel,
                   bool capture, const QString &directory)
{
    checkCancelled(cancel);
    const auto program = findTool(name, QCoreApplication::applicationDirPath());
    const QTemporaryDir logs(QDir(QDir::tempPath()).filePath(QStringLiteral("spoolcut-process-XXXXXX")));
    require(logs.isValid(), logs.errorString());
    const auto stdoutPath = logs.filePath(QStringLiteral("stdout"));
    const auto stderrPath = logs.filePath(QStringLiteral("stderr"));
    QProcess process;
    process.setStandardInputFile(QProcess::nullDevice());
    process.setStandardOutputFile(capture ? stdoutPath : QProcess::nullDevice());
    process.setStandardErrorFile(stderrPath);
    if (!directory.isEmpty())
        process.setWorkingDirectory(directory);
#ifdef Q_OS_WIN
    process.setCreateProcessArgumentsModifier([](QProcess::CreateProcessArguments *args) {
        args->flags |= CREATE_NO_WINDOW;
    });
#endif
    process.start(program, arguments, QIODevice::ReadOnly);
    while (process.state() != QProcess::NotRunning) {
        if (cancel.load(std::memory_order_relaxed)) {
            process.kill();
            process.waitForFinished(-1);
            break;
        }
        process.waitForFinished(20);
    }
    checkCancelled(cancel);
    require(process.error() != QProcess::FailedToStart,
            QStringLiteral("starting %1: %2").arg(program, process.errorString()));

    QFile stderrFile(stderrPath);
    const bool stderrOpened = stderrFile.open(QIODevice::ReadOnly);
    require(stderrOpened, stderrFile.errorString());
    const auto errors = QString::fromUtf8(stderrFile.read(16'384)).trimmed();
    require(stderrFile.error() == QFileDevice::NoError, stderrFile.errorString());
    require(process.exitStatus() == QProcess::NormalExit && process.exitCode() == 0 && errors.isEmpty(),
            QStringLiteral("%1 (exit %2): %3").arg(program).arg(process.exitCode()).arg(errors));

    if (!capture)
        return {};
    QFile stdoutFile(stdoutPath);
    const bool stdoutOpened = stdoutFile.open(QIODevice::ReadOnly);
    require(stdoutOpened, stdoutFile.errorString());
    const auto result = stdoutFile.readAll();
    require(stdoutFile.error() == QFileDevice::NoError, stdoutFile.errorString());
    return result;
}

} // namespace spoolcut::detail
