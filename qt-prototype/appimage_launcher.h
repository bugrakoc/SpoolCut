#pragma once

#include <QString>

inline constexpr auto spoolCutDesktopId = "io.github.bugrakoc.SpoolCut.QtPrototype";

QString registerAppImageLauncher();
QString installAppImageLauncher(const QString &image, const QString &dataHome);
