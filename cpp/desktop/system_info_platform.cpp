// SPDX-License-Identifier: GPL-3.0-only
#include "system_info.h"
#include "detail.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QProcess>
#include <QRegularExpression>
#include <QSysInfo>

#include <mpv/client.h>

#include <memory>

#ifdef Q_OS_WIN
#include <qt_windows.h>
#include <setupapi.h>
#endif

using namespace Qt::StringLiterals;

namespace {
QString releaseToken(const QString &value)
{
    static const QRegularExpression version(uR"(\A(?:[vn]?[0-9]|N-[0-9])[0-9A-Za-z.+_~-]{0,63}\z)"_s);
    return version.match(value).hasMatch() ? value : QString{};
}

QString toolVersion(const QString &tool, const QString &appDirectory)
{
    QString executable;
    try {
        executable = spoolcut::detail::findTool(tool, appDirectory);
    } catch (const spoolcut::Error &) {
        return {};
    }

    QProcess process;
    process.setStandardInputFile(QProcess::nullDevice());
    process.setStandardErrorFile(QProcess::nullDevice());
#ifdef Q_OS_WIN
    process.setCreateProcessArgumentsModifier([](QProcess::CreateProcessArguments *arguments) {
        arguments->flags |= CREATE_NO_WINDOW;
    });
#endif
    process.start(executable, {u"-version"_s}, QIODevice::ReadOnly);
    if (!process.waitForFinished(1600)) {
        if (process.state() != QProcess::NotRunning) {
            process.kill();
            process.waitForFinished(200);
        }
        return {};
    }
    if (process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0)
        return {};
    return mediaToolVersion(process.readLine(256), tool);
}

QString mpvVersion()
{
    // This idle instance reads no configuration, scripts or media and opens no
    // output devices. Its version is available before any preview is created.
    const std::unique_ptr<mpv_handle, decltype(&mpv_terminate_destroy)>
        handle(mpv_create(), &mpv_terminate_destroy);
    if (!handle || mpv_set_option_string(handle.get(), "config", "no") < 0 ||
        mpv_set_option_string(handle.get(), "load-scripts", "no") < 0 ||
        mpv_initialize(handle.get()) < 0)
        return {};
    const std::unique_ptr<char, decltype(&mpv_free)>
        value(mpv_get_property_string(handle.get(), "mpv-version"), &mpv_free);
    if (!value)
        return {};
    auto version = QString::fromUtf8(value.get());
    if (version.startsWith(u"mpv "_s))
        version.remove(0, 4);
    return releaseToken(version);
}

QString ramInGiB(quint64 kibibytes)
{
    if (!kibibytes)
        return {};
    return QString::number(static_cast<double>(kibibytes) / (1024 * 1024), 'f', 1) + u" GiB"_s;
}

QString virtualMachine(const QString &maker, const QString &model)
{
    // Only return fixed platform names for recognized virtual hardware.
    if (maker == u"QEMU" || model == u"KVM")
        return u"Virtual machine (QEMU/KVM)"_s;
    if (maker == u"VMware, Inc." && model.startsWith(u"VMware"))
        return u"Virtual machine (VMware)"_s;
    if (model == u"VirtualBox")
        return u"Virtual machine (VirtualBox)"_s;
    if (maker == u"Microsoft Corporation" && model == u"Virtual Machine")
        return u"Virtual machine (Hyper-V)"_s;
    if (maker == u"Xen")
        return u"Virtual machine (Xen)"_s;
    return {};
}

#ifdef Q_OS_LINUX
QString sysfsValue(const QString &path)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly))
        return {};
    return QString::fromUtf8(file.read(256)).trimmed();
}

QString procField(const QString &path, const QByteArray &key)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly))
        return {};
    // procfs files can report zero size while still having lines to read.
    for (int index = 0; index < 128; ++index) {
        const auto line = file.readLine(512);
        if (line.isEmpty())
            break;
        const auto separator = line.indexOf(':');
        if (separator > 0 && line.first(separator).trimmed() == key)
            return QString::fromUtf8(line.sliced(separator + 1)).trimmed();
    }
    return {};
}

QString pciModel(const QString &vendor, const QString &device)
{
    const QStringList databases{u"/usr/share/hwdata/pci.ids"_s, u"/usr/share/misc/pci.ids"_s,
                                u"/usr/share/pci.ids"_s};
    const auto vendorKey = vendor.sliced(2).toLatin1() + "  ";
    const auto deviceKey = '\t' + device.sliced(2).toLatin1() + "  ";
    for (const auto &path : databases) {
        QFile ids(path);
        if (!ids.open(QIODevice::ReadOnly))
            continue;
        bool correctVendor = false;
        while (!ids.atEnd()) {
            const auto line = ids.readLine(512);
            if (line.startsWith(deviceKey) && correctVendor)
                return QString::fromUtf8(line.sliced(deviceKey.size())).trimmed();
            if (!line.isEmpty() && line.front() != '\t' && line.front() != '#') {
                if (correctVendor)
                    break;
                correctVendor = line.startsWith(vendorKey);
            }
        }
    }
    return {};
}

void collectLinuxGraphics(SystemInformation &info)
{
    const QDir drm(u"/sys/class/drm"_s);
    static const QRegularExpression cardName(uR"(\Acard[0-9]+\z)"_s);
    static const QRegularExpression pciId(uR"(\A0x[0-9a-f]{4}\z)"_s);
    for (const auto &card : drm.entryList({u"card*"_s}, QDir::Dirs | QDir::NoDotAndDotDot)) {
        if (!cardName.match(card).hasMatch())
            continue;
        const QString base = drm.filePath(card + u"/device/"_s);
        const auto vendor = sysfsValue(base + u"vendor"_s);
        const auto device = sysfsValue(base + u"device"_s);
        if (pciId.match(vendor).hasMatch() && pciId.match(device).hasMatch()) {
            const auto name = pciModel(vendor, device);
            if (!name.isEmpty())
                info.gpus << name;
        }
        const QFileInfo link(base + u"driver"_s);
        const auto module = QFileInfo(link.symLinkTarget()).fileName();
        if (!module.isEmpty()) {
            const auto version = releaseToken(sysfsValue(base + u"driver/module/version"_s));
            info.drivers << (version.isEmpty() ? module : module + u" "_s + version);
        }
    }
}

QString linuxDesktop()
{
    const auto names = qEnvironmentVariable("XDG_CURRENT_DESKTOP").toLower().split(u':');
    if (names.contains(u"kde"_s))
        return u"KDE Plasma"_s;
    if (names.contains(u"gnome"_s))
        return u"GNOME"_s;
    if (names.contains(u"xfce"_s))
        return u"Xfce"_s;
    if (names.contains(u"x-cinnamon"_s) || names.contains(u"cinnamon"_s))
        return u"Cinnamon"_s;
    const QStringList common{u"MATE"_s, u"LXQt"_s, u"LXDE"_s, u"COSMIC"_s, u"Sway"_s,
                             u"Hyprland"_s, u"Budgie"_s, u"Pantheon"_s};
    for (const auto &name : common) {
        if (names.contains(name.toLower()))
            return name;
    }
    return {};
}
#elif defined(Q_OS_WIN)
QString registryString(HKEY root, const wchar_t *key, const wchar_t *name)
{
    wchar_t value[256]{};
    DWORD bytes = sizeof(value);
    if (RegGetValueW(root, key, name, RRF_RT_REG_SZ | RRF_ZEROONFAILURE,
                     nullptr, value, &bytes) != ERROR_SUCCESS)
        return {};
    return QString::fromWCharArray(value).trimmed();
}

void collectWindowsGraphics(SystemInformation &info)
{
    // Display device class. Query model and driver version for present adapters.
    constexpr GUID displayClass{0x4d36e968, 0xe325, 0x11ce,
                                {0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x03, 0x18}};
    const auto devices = SetupDiGetClassDevsW(&displayClass, nullptr, nullptr, DIGCF_PRESENT);
    if (devices == INVALID_HANDLE_VALUE)
        return;
    for (DWORD index = 0; ; ++index) {
        SP_DEVINFO_DATA adapter{};
        adapter.cbSize = sizeof(adapter);
        if (!SetupDiEnumDeviceInfo(devices, index, &adapter))
            break;
        wchar_t name[256]{};
        DWORD type{};
        if (SetupDiGetDeviceRegistryPropertyW(devices, &adapter, SPDRP_DEVICEDESC, &type,
                reinterpret_cast<PBYTE>(name), sizeof(name), nullptr) && type == REG_SZ)
            info.gpus << QString::fromWCharArray(name);
        const auto driver =
            SetupDiOpenDevRegKey(devices, &adapter, DICS_FLAG_GLOBAL, 0, DIREG_DRV, KEY_QUERY_VALUE);
        if (driver != INVALID_HANDLE_VALUE) {
            const auto version = releaseToken(registryString(driver, nullptr, L"DriverVersion"));
            const auto provider = registryString(driver, nullptr, L"ProviderName");
            if (!provider.isEmpty() || !version.isEmpty())
                info.drivers << (provider + u" "_s + version).trimmed();
            RegCloseKey(driver);
        }
    }
    SetupDiDestroyDeviceInfoList(devices);
}
#endif
}

QString mediaToolVersion(const QByteArray &output, const QString &tool)
{
    const auto prefix = tool.toLatin1() + " version ";
    if (!output.startsWith(prefix) || output.size() > 255)
        return {};
    const auto banner = output.sliced(prefix.size()).simplified();
    const auto end = banner.indexOf(' ');
    return releaseToken(QString::fromLatin1(end < 0 ? banner : banner.first(end)));
}

SystemInformation collectSystemInformation(const QString &appDirectory)
{
    SystemInformation info;
#ifdef Q_OS_LINUX
    info.os = u"Linux "_s + QSysInfo::kernelVersion();
    info.distribution = QSysInfo::prettyProductName();
    info.desktop = linuxDesktop();
    info.cpu = procField(u"/proc/cpuinfo"_s, "model name");
    const auto memory = procField(u"/proc/meminfo"_s, "MemTotal");
    info.ram = ramInGiB(memory.section(u' ', 0, 0).toULongLong());
    info.virtualization = virtualMachine(sysfsValue(u"/sys/class/dmi/id/sys_vendor"_s),
                                         sysfsValue(u"/sys/class/dmi/id/product_name"_s));
    collectLinuxGraphics(info);
#elif defined(Q_OS_WIN)
    info.os = QSysInfo::prettyProductName() + u" (build "_s + QSysInfo::kernelVersion() + u')';
    info.desktop = u"Windows desktop"_s;
    info.cpu = registryString(HKEY_LOCAL_MACHINE,
        LR"(HARDWARE\DESCRIPTION\System\CentralProcessor\0)", L"ProcessorNameString");
    ULONGLONG memory{};
    if (GetPhysicallyInstalledSystemMemory(&memory))
        info.ram = ramInGiB(memory);
    constexpr auto bios = LR"(HARDWARE\DESCRIPTION\System\BIOS)";
    info.virtualization = virtualMachine(registryString(HKEY_LOCAL_MACHINE, bios, L"SystemManufacturer"),
                                         registryString(HKEY_LOCAL_MACHINE, bios, L"SystemProductName"));
    collectWindowsGraphics(info);
#endif
    info.mpv = mpvVersion();
    info.ffmpeg = toolVersion(u"ffmpeg"_s, appDirectory);
    info.ffprobe = toolVersion(u"ffprobe"_s, appDirectory);
    return info;
}
