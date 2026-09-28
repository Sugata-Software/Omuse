#pragma once

#include "interfaces/IUpdateService.h"
#include <QObject>
#include <QDBusObjectPath>
#include <QDBusMessage>

namespace qtplatform {

/// Linux implementation of IUpdateService using Flatpak's XDG desktop portal:
///   org.freedesktop.portal.Flatpak -> CreateUpdateMonitor
///   org.freedesktop.portal.Flatpak.UpdateMonitor -> UpdateAvailable, Update, Progress, Close
class FlatpakUpdateService : public QObject, public IUpdateService {
    Q_OBJECT

public:
    explicit FlatpakUpdateService(QObject *parent = nullptr);
    ~FlatpakUpdateService() override;

    void checkForUpdates(bool interactive = true) override;
    void installUpdate() override;
    UpdateStatus status() const override { return m_status; }
    UpdateInfo updateInfo() const override { return m_info; }
    bool isSupported() const override;

public slots:
    void onUpdateAvailable(const QVariantMap &updateInfo);
    void onProgress(const QVariantMap &info);

private:
    bool ensurePortalMonitor(QString *errorMessage = nullptr);
    void closeMonitor();

    UpdateStatus m_status = UpdateStatus::Idle;
    UpdateInfo m_info;
    QDBusObjectPath m_monitorPath;
    bool m_hasUpdate = false;
    bool m_monitorActive = false;
};

}  // namespace qtplatform
