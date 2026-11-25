#include "preview.h"
#include "timeline.h"
#include "window.h"

#include <QDir>
#include <QFileInfo>
#include <QOpenGLWidget>
#include <QProcess>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QtTest>

class PrototypeTest final : public QObject {
    Q_OBJECT

private slots:
    void proportionalTimeline() {
        Timeline timeline;
        timeline.resize(824, 180);
        timeline.append({"A", 2});
        timeline.append({"B", 6});
        QCOMPARE(timeline.xAt(2) - timeline.xAt(0), 100 * 2);
        QCOMPARE(timeline.xAt(8) - timeline.xAt(2), 100 * 6);
        timeline.setPosition(2);
        timeline.zoom(2);
        QCOMPARE(timeline.xAt(2), 212);
        QCOMPARE(timeline.xAt(2) - timeline.xAt(1), 200);
        timeline.fit();
        QSignalSpy scrub(&timeline, &Timeline::scrubbed);
        QTest::mouseClick(&timeline, Qt::LeftButton, Qt::NoModifier, QPoint(412, 24));
        QCOMPARE(scrub.size(), 1);
        QCOMPARE(scrub.takeFirst().at(0).toDouble(), 4);
    }

    void previewLifecycle() {
        QTemporaryDir directory(QDir::tempPath() + "/SpoolCut ü clips-XXXXXX");
        QVERIFY(directory.isValid());
        const auto make = [&](const QString &name, const QString &color, bool hevc) {
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
                args << "-tag:v" << "hvc1" << "-x265-params" << "pools=1:frame-threads=1:log-level=error";
            args << "-c:a" << "aac" << QDir::toNativeSeparators(path);
            process.start(ffmpeg, args);
            if (!process.waitForFinished(20000) || process.exitStatus() != QProcess::NormalExit
                || process.exitCode() != 0) {
                qWarning().noquote() << process.errorString() << process.readAllStandardError();
                return QString{};
            }
            return path;
        };
        const QString avc = make("01 kırmızı H264.mp4", "red", false);
        const QString hevc = make("02 mavi HEVC.mp4", "blue", true);
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
            window.importFiles({avc, hevc});
            QTRY_COMPARE_WITH_TIMEOUT(window.clipCount(), 2, 10000);
            QTRY_VERIFY_WITH_TIMEOUT(!frames.isEmpty() && !audio.isEmpty(), 12000);
            const QString renderer = window.preview()->renderer();
            QVERIFY2(!renderer.isEmpty(), "No renderer came up");
            if (qEnvironmentVariable("SPOOLCUT_QT_PREVIEW") == "software")
                QCOMPARE(renderer, QString("Software"));
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
            QTRY_VERIFY_WITH_TIMEOUT(color().red() > 180 && color().blue() < 70, 3000);
            window.togglePlayback();
            QTRY_VERIFY_WITH_TIMEOUT(window.preview()->position() > 0.20, 5000);
            for (int i = 0; i < 20; ++i) {
                window.scrub(i % 2 ? 7.0 - i * 0.06 : 1.5 + i * 0.04);
                window.resize(850 + i * 9, 570 + i * 4);
                QTest::qWait(12);
            }
            QSignalSpy positions(window.preview(), &Preview::positionChanged);
            window.scrub(5.25);
            QTRY_VERIFY_WITH_TIMEOUT(color().blue() > 180 && color().red() < 70, 10000);
            QTRY_VERIFY_WITH_TIMEOUT(!positions.isEmpty()
                && qAbs(positions.last().at(0).toDouble() - 1.25) < 0.12, 5000);
            QTest::qWait(160); // Allow the seek to settle on a decoded frame timestamp.
            const double paused = window.preview()->position();
            QTest::qWait(160);
            QCOMPARE(window.preview()->position(), paused);
            window.scrub(0.75);
            QTRY_VERIFY_WITH_TIMEOUT(color().red() > 180 && color().blue() < 70, 10000);
            QCOMPARE(failures.size(), 0);
            window.clear();
            QCOMPARE(window.clipCount(), 0);
            window.importFiles({hevc});
            QTRY_COMPARE_WITH_TIMEOUT(window.clipCount(), 1, 10000);
            QTRY_VERIFY_WITH_TIMEOUT(color().blue() > 180, 10000);
            window.close();
        }
    }
};

QTEST_MAIN(PrototypeTest)
#include "prototype_test.moc"
