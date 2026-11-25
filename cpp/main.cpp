// SPDX-License-Identifier: GPL-3.0-only
#include "spoolcut.h"

#include <QCoreApplication>
#include <QJsonDocument>

#include <csignal>
#include <cstdio>

#ifdef Q_OS_WIN
#include <qt_windows.h>
#endif

namespace {
spoolcut::Cancel cancelled{false};
static_assert(spoolcut::Cancel::is_always_lock_free);

#ifdef Q_OS_WIN
BOOL WINAPI onConsoleEvent(DWORD event)
{
    if (event != CTRL_C_EVENT && event != CTRL_BREAK_EVENT)
        return FALSE;
    cancelled.store(true, std::memory_order_relaxed);
    return TRUE;
}
#else
void onInterrupt(int)
{
    cancelled.store(true, std::memory_order_relaxed);
}
#endif

constexpr auto usage = "SpoolCut stream-copy editor\n"
                       "  spoolcut inspect SOURCE\n"
                       "  spoolcut plan PROJECT.json\n"
                       "  spoolcut export PROJECT.json OUTPUT.mkv";

void printJson(const QJsonObject &object)
{
    const auto bytes = QJsonDocument(object).toJson();
    const auto length = static_cast<size_t>(bytes.size());
    if (std::fwrite(bytes.constData(), 1, length, stdout) != length || std::fflush(stdout) != 0)
        throw spoolcut::Error(QStringLiteral("writing JSON output"));
}
} // namespace

int main(int argc, char **argv)
{
    const QCoreApplication app(argc, argv);
    try {
#ifdef Q_OS_WIN
        if (!SetConsoleCtrlHandler(onConsoleEvent, TRUE))
#else
        if (std::signal(SIGINT, onInterrupt) == SIG_ERR)
#endif
            throw spoolcut::Error(QStringLiteral("installing Ctrl+C handler"));

        const auto args = app.arguments().mid(1);
        if (args.size() == 2 && args[0] == QStringLiteral("inspect")) {
            printJson(spoolcut::inspect(args[1], cancelled).json());
        } else if ((args.size() == 2 && args[0] == QStringLiteral("plan")) ||
                   (args.size() == 3 && args[0] == QStringLiteral("export"))) {
            const auto project = spoolcut::Project::load(args[1]);
            const auto root = spoolcut::projectDirectory(args[1]);
            if (args.size() == 2)
                printJson(spoolcut::plan(project, root, cancelled).json());
            else
                spoolcut::exportProject(project, root, args[2], cancelled);
        } else if (args.isEmpty() || (args.size() == 1 && (args[0] == QStringLiteral("-h") ||
                                                          args[0] == QStringLiteral("--help")))) {
            std::puts(usage);
        } else {
            throw spoolcut::Error(QStringLiteral("invalid arguments\n") + QString::fromLatin1(usage));
        }
    } catch (const std::exception &error) {
        std::fprintf(stderr, "spoolcut: %s\n", error.what());
        return cancelled.load(std::memory_order_relaxed) ? 130 : 1;
    }
    return 0;
}
