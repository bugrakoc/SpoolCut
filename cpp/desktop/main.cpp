#include "appimage_launcher.h"
#include "window.h"
#include "preview.h"

#include <QApplication>
#include <QCommandLineParser>
#include <QIcon>
#include <QTimer>

#include <clocale>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    std::setlocale(LC_NUMERIC, "C");
    app.setApplicationName("SpoolCut");
    app.setApplicationVersion(QStringLiteral(SPOOLCUT_VERSION));
    app.setDesktopFileName(QLatin1String(spoolCutDesktopId));
    app.setWindowIcon(QIcon(":/io.github.bugrakoc.SpoolCut.png"));

    QCommandLineParser parser;
    parser.addHelpOption();
    parser.addVersionOption();
    parser.addPositionalArgument("files", "MP4 or MOV files to preview", "[files…]");
    const QCommandLineOption smoke("smoke", "Exit successfully after drawing a frame with audio.");
    parser.addOption(smoke);
    parser.process(app);
#ifdef Q_OS_LINUX
    const QString launcherError = registerAppImageLauncher();
    if (!launcherError.isEmpty())
        qWarning().noquote() << launcherError;
#endif

    Window window;
    if (parser.isSet(smoke)) {
        bool frame = false;
        bool audio = false;
        auto check = [&] {
            if (frame && audio)
                app.exit(0);
        };
        QObject::connect(window.preview(), &Preview::frameDrawn, &app, [&] { frame = true; check(); });
        QObject::connect(window.preview(), &Preview::audioChanged, &app, [&] { audio = true; check(); });
        QObject::connect(&window, &Window::problem, &app, [&](const QString &error) {
            qCritical().noquote() << error;
            app.exit(1);
        });
        QTimer::singleShot(15000, &app, [&] { app.exit(2); });
        window.show();
        QTimer::singleShot(0, &window, [&] { window.importFiles(parser.positionalArguments(), true); });
        const int result = app.exec();
        window.clear();
        return result;
    }
    window.show();
    const auto files = parser.positionalArguments();
    if (files.size() == 1 && files.front().endsWith(".json", Qt::CaseInsensitive))
        window.openProject(files.front());
    else
        window.importFiles(files, true);
    return app.exec();
}
