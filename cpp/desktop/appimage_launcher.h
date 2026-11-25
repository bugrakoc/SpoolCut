#pragma once

#include <QString>

inline constexpr auto spoolCutDesktopId = "io.github.bugrakoc.SpoolCut";

QString registerAppImageLauncher();
QString installAppImageLauncher(const QString &image, const QString &dataHome);
