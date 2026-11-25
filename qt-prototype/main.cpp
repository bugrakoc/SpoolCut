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
    app.setApplicationName("SpoolCut Qt prototype");
    app.setDesktopFileName("io.github.bugrakoc.SpoolCut.QtPrototype");
    app.setWindowIcon(QIcon(":/io.github.bugrakoc.SpoolCut.png"));

    QCommandLineParser parser;
    parser.addHelpOption();
    parser.addPositionalArgument("files", "MP4 or MOV files to preview", "[files…]");
    const QCommandLineOption smoke("smoke", "Exit successfully after drawing a frame with audio.");
    parser.addOption(smoke);
    parser.process(app);

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
        QTimer::singleShot(0, &window, [&] { window.importFiles(parser.positionalArguments()); });
        const int result = app.exec();
        window.close();
        return result;
    }
    window.show();
    window.importFiles(parser.positionalArguments());
    return app.exec();
}
