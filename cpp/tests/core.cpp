// SPDX-License-Identifier: GPL-3.0-only
#include "detail.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QScopeGuard>
#include <QTemporaryDir>

#include <chrono>
#include <cstdio>
#include <thread>

#ifdef Q_OS_WIN
#include <qt_windows.h>
#endif

using namespace Qt::StringLiterals;
using namespace spoolcut;
using namespace spoolcut::detail;

namespace {

void write(const QString &path, const QByteArray &bytes)
{
    QFile file(path);
    require(file.open(QIODevice::WriteOnly), u"opening %1 for writing"_s.arg(path));
    require(file.write(bytes) == bytes.size() && file.flush(), file.errorString());
}

QByteArray read(QFile &file)
{
    const bool opened = file.open(QIODevice::ReadOnly);
    require(opened, file.errorString());
    return file.readAll();
}

QByteArray read(const QString &path)
{
    QFile file(path);
    return read(file);
}

template<class Function>
void rejects(Function function, const QString &error)
{
    try {
        function();
    } catch (const Error &actual) {
        require(QString::fromUtf8(actual.what()).contains(error),
                u"expected %1, got %2"_s.arg(error, QString::fromUtf8(actual.what())));
        return;
    }
    throw Error(u"expected failure %1"_s.arg(error));
}

void exactProject(const QTemporaryDir &scratch)
{
    const auto path = scratch.filePath(u"kurgu ğ.json"_s);
    const Project project{{{u"A"_s, u"../medya/çok özel.mov"_s}},
                          {{u"A"_s, std::numeric_limits<qint64>::min(), 9007199254740993LL},
                           {u"A"_s, -9007199254740993LL, std::numeric_limits<qint64>::max()}}};
    project.save(path);
    require(Project::load(path) == project, u"exact integer and relative path round trip"_s);
    require(read(path).contains("9007199254740993"), u"PTS must remain a JSON integer"_s);
    const std::array malformed{
        R"({"sources":{},"clips":[{"source":"A","in_pts":1.0,"out_pts":2}]})",
        R"({"sources":{},"clips":[{"source":"A","in_pts":-0,"out_pts":2}]})",
        R"({"sources":{},"clips":[{"source":"A","in_pts":1e0,"out_pts":2}]})",
        R"({"sources":{},"clips":[{"source":"A","in_pts":0,"out_pts":9223372036854775808}]})",
        R"({"sources":{},"clips":[{"source":"A","in_pts":0,"in_pts":1,"out_pts":2}]})",
        R"({"sources":{},"clips":[{"source":"A","in_pts":0,"out_pts":2,"other":0}]})",
        R"({"sources":{},"clips":[],"clips":[]})",
        R"({"sources":{},"clips":[],})",
        R"({"sources":{},"clips":[]} [])",
    };
    for (const auto *json : malformed) {
        write(path, json);
        rejects([&] { Project::load(path); }, u"reading project"_s);
    }
    project.save(path);
#ifdef Q_OS_WIN
    const auto name = mediaPath(path).toStdWString();
    const auto lock = CreateFileW(name.c_str(), GENERIC_READ, 0, nullptr, OPEN_EXISTING, 0, nullptr);
    require(lock != INVALID_HANDLE_VALUE, u"lock project"_s);
    // Release the handle even if the save gives an unexpected result.
    bool blocked = false;
    try {
        Project{}.save(path);
    } catch (const Error &) {
        blocked = true;
    }
    CloseHandle(lock);
    require(blocked && Project::load(path) == project, u"locked project must retain its prior contents"_s);
#endif
    Project{}.save(path);
    require(Project::load(path) == Project{}, u"replace a project"_s);
}

void fileIdentity(const QTemporaryDir &scratch)
{
    const auto original = scratch.filePath(u"original.mp4"_s);
    const auto replacement = scratch.filePath(u"replacement.mp4"_s);
    write(original, "aaaa");
    write(replacement, "bbbb");
    QFile first(original);
    QFile second(replacement);
    require(first.open(QIODevice::ReadOnly) && second.open(QIODevice::ReadWrite), u"open identity fixtures"_s);
    require(second.setFileTime(first.fileTime(QFileDevice::FileModificationTime),
                               QFileDevice::FileModificationTime), u"match source modification time"_s);
    const auto a = SourceIdentity::read(first);
    const auto b = SourceIdentity::read(second);
    require(a.size == b.size && a != b && a.id != b.id, u"same-sized replacement must have a different file ID"_s);
    const auto target = scratch.filePath(u"truncated"_s);
    rejects([&] { publish(original, target + QChar::Null + u"suffix"_s); }, u"invalid file path"_s);
    require(!QFile::exists(target), u"NUL in a publication path"_s);
}

void processBoundary(const QTemporaryDir &scratch)
{
    const auto suffix = QFileInfo(QCoreApplication::applicationFilePath()).suffix();
    const auto name = suffix.isEmpty() ? u"spoolcut-test-child"_s : u"spoolcut-test-child.%1"_s.arg(suffix);
    const auto override = scratch.filePath(name);
    require(QFile::copy(QCoreApplication::applicationFilePath(), override), u"create executable fixture"_s);
    require(qputenv("SPOOLCUT_SPOOLCUT-TEST-CHILD", QFile::encodeName(override)), u"set tool override"_s);

    const auto marker = scratch.filePath(u"started"_s);
    Cancel cancel{false};
    std::jthread watcher([&](std::stop_token stop) {
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(8);
        while (!stop.stop_requested() && !QFile::exists(marker) && std::chrono::steady_clock::now() < deadline)
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        cancel.store(true);
    });
    rejects([&] { runTool(u"spoolcut-test-child"_s, {u"--hold"_s, marker}, cancel); }, u"cancelled"_s);
    require(read(marker) == "started", u"cancellation must occur after child startup"_s);
    qunsetenv("SPOOLCUT_SPOOLCUT-TEST-CHILD");

    const auto path = scratch.filePath(u"on PATH"_s);
    require(QDir().mkdir(path), u"make PATH fixture directory"_s);
    const auto fallback = QDir(path).filePath(name);
    require(QFile::copy(override, fallback), u"copy PATH executable"_s);
    const auto priorPath = qgetenv("PATH");
    require(qputenv("PATH", QFile::encodeName(path)), u"set PATH fixture"_s);
    require(findTool(u"spoolcut-test-child"_s, scratch.path()) == override, u"bundled tool precedes PATH"_s);
    require(findTool(u"spoolcut-test-child"_s, {}) == fallback, u"find tool on PATH"_s);
    qputenv("PATH", priorPath);
    qputenv("SPOOLCUT_SPOOLCUT-TEST-CHILD", "missing");
    rejects([&] { findTool(u"spoolcut-test-child"_s, scratch.path()); }, u"does not point to an executable"_s);
    qunsetenv("SPOOLCUT_SPOOLCUT-TEST-CHILD");
}

void exportBoundaries(const QString &projectPath, const QString &destination)
{
    const auto project = Project::load(projectPath);
    const auto root = QFileInfo(projectPath).absolutePath();
    const QDir destinationDir(QFileInfo(destination).absolutePath());
    const auto survivors = [&] { return destinationDir.entryList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden); };
    const auto prior = survivors();
    Cancel cancel{false};
    rejects([&] {
        exportProject(project, root, destination, cancel, [&](ExportProgress step) {
            if (step.stage == ExportStage::Copying && step.completed == 1)
                cancel.store(true);
        });
    }, u"cancelled"_s);
    require(cancel.load() && survivors() == prior, u"cancelled export left staging or output files"_s);

    cancel.store(false);
    rejects([&] {
        exportProject(project, root, destination, cancel, [&](ExportProgress step) {
            if (step.stage == ExportStage::Verifying)
                write(destination, "existing");
        });
    }, u"publishing"_s);
    require(read(destination) == "existing", u"publication overwrote a racing writer"_s);
    rejects([&] { exportProject(project, root, destination, cancel); }, u"already exists"_s);
    require(read(destination) == "existing", u"existing output changed"_s);
    require(QFile::remove(destination) && survivors() == prior, u"failed export left staging files"_s);

    {
        const auto previous = qgetenv("SPOOLCUT_FFMPEG");
        const auto restore = qScopeGuard([&] {
            if (previous.isNull())
                qunsetenv("SPOOLCUT_FFMPEG");
            else
                qputenv("SPOOLCUT_FFMPEG", previous);
        });
        rejects([&] {
            exportProject(project, root, destination, cancel, [&](ExportProgress step) {
                if (step.stage == ExportStage::Joining)
                    qputenv("SPOOLCUT_FFMPEG", "spoolcut-missing-ffmpeg");
            });
        }, u"joining copied clips"_s);
        require(survivors() == prior, u"failed join left staging or output files"_s);
    }

    {
        const auto path = QDir(root).filePath(project.sources.at(project.clips.front().source));
        const auto backup = path + u".original"_s;
        const auto restore = qScopeGuard([&] {
            if (QFile::exists(backup)) {
                QFile::remove(path);
                QFile::rename(backup, path);
            }
        });
        rejects([&] {
            exportProject(project, root, destination, cancel, [&](ExportProgress step) {
                if (step.stage == ExportStage::Verifying) {
                    require(QFile::rename(path, backup) && QFile::copy(backup, path),
                            u"replace source during verification"_s);
                }
            });
        }, u"source changed or was replaced"_s);
        require(survivors() == prior, u"source replacement left staging or output files"_s);
    }

    exportProject(project, root, destination, cancel);
    auto expected = prior;
    expected.append(QFileInfo(destination).fileName());
    expected.sort();
    auto actual = survivors();
    actual.sort();
    require(actual == expected, u"successful export left staging files"_s);

    if (const auto volume = qEnvironmentVariable("SPOOLCUT_TEST_OTHER_VOLUME"); !volume.isEmpty()) {
        const QTemporaryDir other(QDir(volume).filePath(u"spoolcut-other-XXXXXX"_s));
        require(other.isValid(), other.errorString());
        const auto crossVolume = other.filePath(u"ikinci sürücü.mkv"_s);
        exportProject(project, root, crossVolume, cancel);
        require(QFileInfo(crossVolume).isFile(), u"cross-volume export output missing"_s);
    }
}

} // namespace

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    try {
        const auto args = app.arguments();
        if (args.size() == 3 && args[1] == u"--hold"_s) {
            write(args[2], "started");
            std::this_thread::sleep_for(std::chrono::seconds(25));
        } else if (args.size() == 3) {
            exportBoundaries(args[1], args[2]);
        } else {
            const QTemporaryDir scratch;
            require(scratch.isValid(), scratch.errorString());
            exactProject(scratch);
            fileIdentity(scratch);
            processBoundary(scratch);
            require(compare({1, 90000}, 3003, {1, 30000}, 1001) == 0, u"compare exact fractional PTS"_s);
            require(micros({1, 128}, -1) == -7813 && micros({1, 128}, 1) == 7813, u"round signed half microseconds"_s);
            rejects([] { micros({1, 1}, std::numeric_limits<qint64>::max()); }, u"outside FFmpeg"_s);
            require(ffmpegTime(std::numeric_limits<qint64>::min()) == u"-9223372036854.775808"_s,
                    u"format smallest signed timestamp"_s);
        }
        return 0;
    } catch (const std::exception &error) {
        std::fprintf(stderr, "%s\n", error.what());
        return 1;
    }
}
