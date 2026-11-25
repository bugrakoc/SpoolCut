#include "appimage_launcher.h"
#include "preview.h"
#include "timeline.h"
#include "window.h"

#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QLabel>
#include <QOpenGLWidget>
#include <QListWidget>
#include <QProcess>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QtTest>
#include <cstdio>

namespace {
QString makeMedia(const QTemporaryDir &directory, const QString &name, const QString &color, bool hevc = false)
{
    const QString path = directory.filePath(name);
    QProcess process;
    const QString ffmpeg = QStandardPaths::findExecutable("ffmpeg");
    const QString encoder = hevc ? "libx265" : "libx264";
    QStringList args{"-nostdin", "-v", "error", "-f", "lavfi", "-i",
                     QString("color=%1:size=320x180:rate=25:duration=4").arg(color),
                     "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=4",
                     "-c:v", encoder, "-threads", "2", "-g", "25",
                     "-pix_fmt", hevc ? "yuv420p10le" : "yuv420p"};
    if (hevc)
        args << "-tag:v" << "hvc1" << "-x265-params" << "pools=1:frame-threads=1:log-level=error:open-gop=0";
    args << "-c:a" << "aac" << QDir::toNativeSeparators(path);
    process.start(ffmpeg, args);
    if (!process.waitForFinished(20000) || process.exitStatus() != QProcess::NormalExit
        || process.exitCode() != 0) {
        qWarning().noquote() << process.errorString() << process.readAllStandardError();
        return QString{};
    }
    return path;
}
}

class PrototypeTest final : public QObject {
    Q_OBJECT

private slots:
    void proportionalTimeline() {
        Timeline timeline;
        timeline.resize(824, 180);
        timeline.setClips({{"A", 2, {}}, {"B", 6, {}}});
        QCOMPARE(timeline.xAt(2) - timeline.xAt(0), 100 * 2);
        QCOMPARE(timeline.xAt(8) - timeline.xAt(2), 100 * 6);
        timeline.setPosition(2);
        timeline.zoom(2);
        QCOMPARE(timeline.xAt(2), 212);
        QCOMPARE(timeline.xAt(2) - timeline.xAt(1), 200);
        timeline.fit();
        timeline.zoom(0.5);
        QCOMPARE(timeline.xAt(8) - timeline.xAt(0), 400);
        QVERIFY(timeline.xAt(8) < timeline.width() * 0.60);
        timeline.fit();
        QCOMPARE(timeline.xAt(8), 812);
        QSignalSpy scrub(&timeline, &Timeline::scrubbed);
        QTest::mouseClick(&timeline, Qt::LeftButton, Qt::NoModifier, QPoint(412, 24));
        QCOMPARE(scrub.size(), 1);
        QCOMPARE(scrub.takeFirst().at(0).toDouble(), 4);

        timeline.setClips({{"A", 2, {0, 1}}, {"B", 6, {0, 2, 4}}});
        QTest::mouseClick(&timeline, Qt::LeftButton, Qt::NoModifier, QPoint(131, 24));
        QCOMPARE(scrub.takeFirst()[0].toDouble(), 1);
        QSignalSpy moved(&timeline, &Timeline::moved);
        QTest::mousePress(&timeline, Qt::LeftButton, Qt::NoModifier, QPoint(100, 66));
        QTest::mouseRelease(&timeline, Qt::LeftButton, Qt::NoModifier, QPoint(810, 66));
        QCOMPARE(moved.size(), 1);
        QCOMPARE(moved[0][0].toULongLong(), quint64{0});
        QCOMPARE(moved[0][1].toULongLong(), quint64{2});
        QSignalSpy trimmed(&timeline, &Timeline::trimmed);
        QTest::mousePress(&timeline, Qt::LeftButton, Qt::NoModifier, QPoint(210, 66));
        QTest::mouseRelease(&timeline, Qt::LeftButton, Qt::NoModifier, QPoint(112, 66));
        QCOMPARE(trimmed.size(), 1);
        QVERIFY(!trimmed[0][1].toBool());
        QCOMPARE(trimmed[0][2].toDouble(), 1);
    }

    void projectEditingAndExport()
    {
        using spoolcut::Project;
        QTemporaryDir directory(QDir::tempPath() + "/SpoolCut ü project-XXXXXX");
        QVERIFY(directory.isValid());
        const auto a = makeMedia(directory, "a kırmızı.mp4", "red");
        const auto b = makeMedia(directory, "b mavi.mp4", "blue");
        QVERIFY(!a.isEmpty() && !b.isEmpty());

        spoolcut::Cancel cancel{false};
        spoolcut::Editor editor;
        editor.import({a, b});
        editor.setIndex(a, spoolcut::IndexedSource::open(a, cancel));
        editor.setIndex(b, spoolcut::IndexedSource::open(b, cancel));
        QCOMPARE(editor.insertSource("source-1", 0), size_t{0});
        QCOMPARE(editor.insertSource("source-2", 1), size_t{1});
        editor.trim(0, true, 1.04);
        editor.split(0, 2.06);
        QCOMPARE(editor.project().clips.size(), size_t{3});
        QCOMPARE(editor.project().clips[0].outPts, editor.project().clips[1].inPts);
        const auto edited = editor.project();
        QCOMPARE(editor.move(2, 0), size_t{0});
        QCOMPARE(editor.project().clips.front().source, QString("source-2"));
        editor.undo();
        QVERIFY(editor.project() == edited);
        editor.redo();
        QVERIFY(editor.copy(0));
        editor.remove(0);
        editor.paste(editor.project().clips.size());
        QCOMPARE(editor.project().clips.back().source, QString("source-2"));
        const auto path = directory.filePath("kurgu ü.spoolcut.json");
        editor.save(path);
        QVERIFY(!editor.dirty());
        const auto portable = Project::load(path);
        QCOMPARE(portable.sources.at("source-1"), QFileInfo(a).fileName());
        const auto plan = spoolcut::plan(portable, directory.path(), cancel);

        Window window;
        QSignalSpy failures(&window, &Window::problem);
        QSignalSpy idle(&window, &Window::idle);
        QSignalSpy exports(&window, &Window::exported);
        QSignalSpy frames(window.preview(), &Preview::frameDrawn);
        window.show();
        QVERIFY(window.openProject(path));
        QTRY_COMPARE_WITH_TIMEOUT(idle.size(), 1, 15000);
        QCOMPARE(window.sourceCount(), size_t{2});
        QCOMPARE(window.clipCount(), size_t{3});
        QVERIFY(qAbs(window.timeline()->duration() - static_cast<double>(plan.durationUs) / 1e6) < 0.000001);
        QTRY_VERIFY_WITH_TIMEOUT(!frames.isEmpty(), 10000);
        QTRY_VERIFY_WITH_TIMEOUT(qAbs(window.preview()->position() - 1.0) < 0.06, 5000);
        const auto output = directory.filePath("kayıt ü.mkv");
        window.startExport(output);
        QTRY_COMPARE_WITH_TIMEOUT(exports.size(), 1, 25000);
        QVERIFY(QFileInfo(output).size() > 0);
        QCOMPARE(failures.size(), 0);

        window.startExport(output);
        QTRY_COMPARE_WITH_TIMEOUT(failures.size(), 1, 10000);
        QVERIFY(failures.takeFirst()[0].toString().contains("output already exists"));
        QVERIFY(QFileInfo(output).size() > 0);

        // Repeated clips keep the worker active while the event loop delivers cancellation.
        auto repeated = portable;
        repeated.clips.assign(35, portable.clips.front());
        const auto repeatedPath = directory.filePath("cancel.spoolcut.json");
        repeated.save(repeatedPath);
        QVERIFY(window.openProject(repeatedPath));
        QTRY_COMPARE_WITH_TIMEOUT(idle.size(), 4, 15000);
        const auto cancelledOutput = directory.filePath("cancel.mkv");
        window.startExport(cancelledOutput);
        const auto staging = QDir(directory.path());
        QTRY_VERIFY_WITH_TIMEOUT(!staging.entryList({".spoolcut-*"}, QDir::Dirs | QDir::Hidden).isEmpty(), 10000);
        window.cancelJob();
        QTRY_COMPARE_WITH_TIMEOUT(idle.size(), 5, 15000);
        QVERIFY(!QFileInfo::exists(cancelledOutput));
        QVERIFY(staging.entryList({".spoolcut-*"}, QDir::Dirs | QDir::Hidden).isEmpty());
        QCOMPARE(failures.size(), 1);
        QVERIFY(failures.takeFirst()[0].toString().contains("cancelled"));

        const auto movedPath = directory.filePath("moved ü clip.mp4");
        QVERIFY(QFile::rename(b, movedPath));
        QVERIFY(window.openProject(path));
        QTRY_COMPARE_WITH_TIMEOUT(idle.size(), 6, 15000);
        QCOMPARE(window.clipCount(), size_t{3});
        QCOMPARE(window.sourceCount(), size_t{2});
        QCOMPARE(failures.size(), 1);
        auto *bin = window.findChild<QListWidget *>();
        QVERIFY(bin);
        auto *missingRow = bin->item(1);
        QVERIFY(missingRow->text().contains("Missing file"));
        auto *timeline = window.timeline();
        QCOMPARE(timeline->clips().size(), size_t{3});
        QVERIFY(!timeline->hasTiming());
        QVERIFY(timeline->clips()[0].unavailable.isEmpty());
        QVERIFY(timeline->clips()[1].unavailable.isEmpty());
        QCOMPARE(timeline->clips()[2].unavailable, QString("Missing media"));
        QVERIFY(timeline->clips()[0].color != timeline->clips()[2].color);

        const auto missingX = timeline->xAt(timeline->duration() - timeline->clips().back().duration / 2);
        QTest::mouseClick(timeline, Qt::LeftButton, Qt::NoModifier, QPoint(qRound(missingX), 66));
        QCOMPARE(bin->currentItem(), missingRow);
        window.relinkSource("source-2", movedPath);
        QTRY_COMPARE_WITH_TIMEOUT(idle.size(), 7, 15000);
        QCOMPARE(bin->currentItem(), missingRow);
        QVERIFY(timeline->hasTiming());
        QCOMPARE(timeline->clips().size(), size_t{3});
        QVERIFY(timeline->clips().back().unavailable.isEmpty());
        QVERIFY(window.saveProject(path));
        const auto relinked = Project::load(path);
        QVERIFY(relinked.clips == portable.clips);
        QCOMPARE(relinked.sources.at("source-2"), QFileInfo(movedPath).fileName());
        window.close();
    }

    void sourceMovedDuringPlayback()
    {
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        const auto a = makeMedia(directory, "first.mp4", "red");
        const auto b = makeMedia(directory, "second.mp4", "blue");
        QVERIFY(!a.isEmpty() && !b.isEmpty());

        Window window;
        QSignalSpy idle(&window, &Window::idle);
        QSignalSpy problems(&window, &Window::problem);
        QSignalSpy frames(window.preview(), &Preview::frameDrawn);
        window.show();
        window.importFiles({a, b}, true);
        QTRY_COMPARE_WITH_TIMEOUT(idle.size(), 1, 15000);
        QTRY_VERIFY_WITH_TIMEOUT(!frames.isEmpty(), 10000);
        const auto projectPath = directory.filePath("untitled.spoolcut.json");
        QVERIFY(window.saveProject(projectPath));
        const auto before = spoolcut::Project::load(projectPath);

        const auto moved = directory.filePath("moved second.mp4");
        window.scrub(3.0);
        window.togglePlayback();
        QVERIFY(QFile::rename(b, moved));
        auto *timeline = window.timeline();
        QVERIFY(timeline->hasTiming());
        QTRY_VERIFY_WITH_TIMEOUT(!timeline->hasTiming(), 8000);
        QCOMPARE(timeline->clips().size(), size_t{2});
        QVERIFY(timeline->clips()[0].unavailable.isEmpty());
        QCOMPARE(timeline->clips()[1].unavailable, QString("Missing media"));
        QCOMPARE(problems.size(), 1);
        QVERIFY(problems.first()[0].toString().contains("second.mp4"));
        auto *bin = window.findChild<QListWidget *>();
        QVERIFY(bin);
        QCOMPARE(bin->currentItem(), bin->item(1));
        QVERIFY(bin->currentItem()->text().contains("Missing file"));
        auto *placeholder = window.preview()->findChild<QLabel *>();
        QVERIFY(placeholder && placeholder->isVisible());
        QCOMPARE(placeholder->text(), QString("Open a video to preview it"));
        QVERIFY(!window.isWindowModified());
        QVERIFY(window.saveProject(projectPath));
        QVERIFY(spoolcut::Project::load(projectPath) == before);

        frames.clear();
        window.relinkSource("source-2", moved);
        QTRY_COMPARE_WITH_TIMEOUT(idle.size(), 2, 15000);
        QVERIFY(timeline->hasTiming());
        QVERIFY(timeline->clips()[1].unavailable.isEmpty());
        QTRY_VERIFY_WITH_TIMEOUT(!frames.isEmpty(), 10000);
        QVERIFY(!placeholder->isVisible());
        QVERIFY(window.saveProject(projectPath));

#ifdef Q_OS_LINUX
        // An open file can survive a rename on Linux. A preview error is
        // the trigger for rechecking its path, including on the same clip.
        const auto movedFirst = directory.filePath("moved first.mp4");
        QVERIFY(QFile::rename(a, movedFirst));
        window.togglePlayback();
        QVERIFY(timeline->hasTiming());
        emit window.preview()->failed("Media load failed");
        QVERIFY(!timeline->hasTiming());
        QCOMPARE(timeline->clips()[0].unavailable, QString("Missing media"));
        QVERIFY(timeline->clips()[1].unavailable.isEmpty());
        QVERIFY(placeholder->isVisible());
#endif
        window.close();
    }

#ifdef Q_OS_LINUX
    void launcherInstallation() {
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        const QString data = directory.filePath("data");
        const QString launcher = QDir(data).filePath(QString("applications/%1.desktop")
                                                     .arg(QLatin1String(spoolCutDesktopId)));
        const QStringList names{"SpoolCut test.AppImage",
                                QStringLiteral(R"(SpoolCut = "ü" %F $tag \.AppImage)")};
        const QString media = directory.filePath("example clip.mp4");
        const QString gio = QStandardPaths::findExecutable("gio");
        QVERIFY(!gio.isEmpty());
        for (const auto &name : names) {
            const QString image = directory.filePath(name);
            QFile stub(image);
            QVERIFY(stub.open(QIODevice::WriteOnly));
            const QByteArray script = "#!/bin/sh\nprintf '%s' \"$1\" > \"$0.args\"\n";
            QCOMPARE(stub.write(script), script.size());
            stub.close();
            QVERIFY(stub.setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner));
            QVERIFY2(installAppImageLauncher(image, data).isEmpty(), qPrintable(image));
            QProcess launch;
            launch.start(gio, {"launch", launcher, media});
            QVERIFY(launch.waitForFinished());
            QCOMPARE(launch.exitCode(), 0);
            QFile arguments(image + ".args");
            QTRY_VERIFY_WITH_TIMEOUT(arguments.exists(), 4000);
            QVERIFY(arguments.open(QIODevice::ReadOnly));
            QCOMPARE(arguments.readAll(), media.toUtf8());
        }
        QFile outsider(launcher);
        QVERIFY(outsider.open(QIODevice::WriteOnly | QIODevice::Truncate));
        QCOMPARE(outsider.write("custom launcher"), 15);
        outsider.close();
        QVERIFY(installAppImageLauncher(directory.filePath(names.front()), data).isEmpty());
        QVERIFY(outsider.open(QIODevice::ReadOnly));
        QCOMPARE(outsider.readAll(), QByteArray("custom launcher"));
    }
#endif

    void previewLifecycle() {
        QTemporaryDir directory(QDir::tempPath() + "/SpoolCut ü clips-XXXXXX");
        QVERIFY(directory.isValid());
        const QString avc = makeMedia(directory, "01 kırmızı H264.mp4", "red", false);
        const QString hevc = makeMedia(directory, "02 mavi HEVC.mp4", "blue", true);
        QVERIFY(!avc.isEmpty());
        QVERIFY(!hevc.isEmpty());

        {
            Window closingDuringImport;
            closingDuringImport.importFiles({avc, hevc});
        }

        for (int repeat = 0; repeat < 2; ++repeat) {
            Window window;
            QSignalSpy failures(&window, &Window::problem);
            QSignalSpy frames(window.preview(), &Preview::frameDrawn);
            QSignalSpy audio(window.preview(), &Preview::audioChanged);
            window.show();
            window.importFiles({avc, hevc}, true);
            QTRY_COMPARE_WITH_TIMEOUT(window.clipCount(), 2, 10000);
            QTRY_VERIFY_WITH_TIMEOUT(!frames.isEmpty() && !audio.isEmpty(), 12000);
            const QString renderer = window.preview()->renderer();
            QVERIFY2(!renderer.isEmpty(), "No renderer came up");
            std::fprintf(stdout, "Preview renderer: %s\n", qPrintable(window.preview()->toolTip()));
            std::fflush(stdout);
            if (qEnvironmentVariable("SPOOLCUT_QT_PREVIEW") == "software")
                QCOMPARE(renderer, QString("Software"));
#ifdef Q_OS_WIN
            if (qEnvironmentVariable("SPOOLCUT_QT_PREVIEW") == "auto") {
                QCOMPARE(renderer, QString("Software"));
                QVERIFY(window.preview()->findChild<QOpenGLWidget *>() == nullptr);
            }
#endif
            if (qEnvironmentVariableIsSet("SPOOLCUT_QT_REQUIRE_OPENGL")
                && qEnvironmentVariable("SPOOLCUT_QT_PREVIEW") != "software")
                QVERIFY2(renderer.startsWith("OpenGL"), qPrintable(renderer));
            if (qEnvironmentVariableIsSet("SPOOLCUT_QT_REQUIRE_FALLBACK")
                && qEnvironmentVariable("SPOOLCUT_QT_PREVIEW") != "software") {
                QCOMPARE(renderer, QString("Software"));
                QVERIFY(window.preview()->toolTip().contains("fallback"));
            }

            auto color = [&] {
                auto *gl = window.preview()->findChild<QOpenGLWidget *>();
                const auto shot = gl ? gl->grabFramebuffer() : window.preview()->grab().toImage();
                return shot.pixelColor(shot.width() / 2, shot.height() / 2);
            };
            // Grabbing the widget can render a new frame. Read both channels
            // from the same image and avoid re-evaluating a successful sample.
            const auto redShown = [&] {
                const auto pixel = color();
                return pixel.red() > 180 && pixel.blue() < 70;
            };
            const auto blueShown = [&] {
                const auto pixel = color();
                return pixel.blue() > 180 && pixel.red() < 70;
            };
            QVERIFY(QTest::qWaitFor(redShown, 3000));
            window.togglePlayback();
            QTRY_VERIFY_WITH_TIMEOUT(window.preview()->position() > 0.20, 5000);
            if (renderer == "Software") {
                for (const auto sample : {0.65, 1.35, 2.25}) {
                    QTRY_VERIFY_WITH_TIMEOUT(window.preview()->position() >= sample, 5000);
                    QVERIFY(redShown());
                }
            }

            window.scrub(3.6);
            QTest::qWait(120);
            QElapsedTimer join;
            const auto ended = connect(window.preview(), &Preview::ended, &window, [&] { join.start(); });
            window.togglePlayback();
            QTRY_VERIFY_WITH_TIMEOUT(join.isValid(), 5000);
            QVERIFY(QTest::qWaitFor(blueShown, 5000));
            std::fprintf(stdout, "Automatic join to visible frame: %lld ms\n",
                         static_cast<long long>(join.elapsed()));
            std::fflush(stdout);
            disconnect(ended);

            for (int i = 0; i < 20; ++i) {
                window.scrub(i % 2 ? 7.0 - i * 0.06 : 1.5 + i * 0.04);
                window.resize(850 + i * 9, 570 + i * 4);
                QTest::qWait(12);
            }
            QSignalSpy positions(window.preview(), &Preview::positionChanged);
            window.scrub(5.25);
            QVERIFY(QTest::qWaitFor(blueShown, 10000));
            QTRY_VERIFY_WITH_TIMEOUT(!positions.isEmpty()
                && qAbs(positions.last().at(0).toDouble() - 1.25) < 0.12, 5000);
            QTest::qWait(160); // Allow the seek to settle on a decoded frame timestamp.
            const double paused = window.preview()->position();
            QTest::qWait(160);
            QCOMPARE(window.preview()->position(), paused);
            window.scrub(0.75);
            QVERIFY(QTest::qWaitFor(redShown, 10000));
            QCOMPARE(failures.size(), 0);
            window.clear();
            QCOMPARE(window.clipCount(), 0);
            window.importFiles({hevc}, true);
            QTRY_COMPARE_WITH_TIMEOUT(window.clipCount(), 1, 10000);
            QVERIFY(QTest::qWaitFor(blueShown, 10000));
            window.clear();
            window.close();
        }
    }
};

QTEST_MAIN(PrototypeTest)
#include "prototype_test.moc"
