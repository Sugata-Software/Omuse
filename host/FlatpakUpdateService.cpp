#include "FlatpakUpdateService.h"

#include <QApplication>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusReply>
#include <QFile>
#include <QMessageBox>

namespace qtplatform {

FlatpakUpdateService::FlatpakUpdateService(QObject *parent)
    : QObject(parent) {
    if (isSupported()) {
        ensurePortalMonitor();
    }
}

FlatpakUpdateService::~FlatpakUpdateService() {
    closeMonitor();
}

bool FlatpakUpdateService::isSupported() const {
    return QFile::exists(QStringLiteral("/.flatpak-info")) || qEnvironmentVariableIsSet("FLATPAK_ID");
}

bool FlatpakUpdateService::ensurePortalMonitor(QString *errorMessage) {
    if (m_monitorActive) return true;

    QDBusConnection bus = QDBusConnection::sessionBus();
    if (!bus.isConnected()) {
        if (errorMessage) *errorMessage = tr("D-Bus session bus is not connected.");
        return false;
    }

    QDBusMessage msg = QDBusMessage::createMethodCall(
        QStringLiteral("org.freedesktop.portal.Flatpak"),
        QStringLiteral("/org/freedesktop/portal/Flatpak"),
        QStringLiteral("org.freedesktop.portal.Flatpak"),
        QStringLiteral("CreateUpdateMonitor")
    );
    QVariantMap options;
    msg << options;

    QDBusMessage reply = bus.call(msg);
    if (reply.type() == QDBusMessage::ErrorMessage) {
        if (errorMessage) *errorMessage = reply.errorMessage();
        return false;
    }

    if (reply.arguments().isEmpty()) {
        if (errorMessage) *errorMessage = tr("No monitor object path returned by Flatpak portal.");
        return false;
    }

    m_monitorPath = reply.arguments().at(0).value<QDBusObjectPath>();
    if (m_monitorPath.path().isEmpty()) {
        if (errorMessage) *errorMessage = tr("Invalid monitor object path returned by Flatpak portal.");
        return false;
    }

    bus.connect(
        QStringLiteral("org.freedesktop.portal.Flatpak"),
        m_monitorPath.path(),
        QStringLiteral("org.freedesktop.portal.Flatpak.UpdateMonitor"),
        QStringLiteral("UpdateAvailable"),
        this,
        SLOT(onUpdateAvailable(QVariantMap))
    );

    bus.connect(
        QStringLiteral("org.freedesktop.portal.Flatpak"),
        m_monitorPath.path(),
        QStringLiteral("org.freedesktop.portal.Flatpak.UpdateMonitor"),
        QStringLiteral("Progress"),
        this,
        SLOT(onProgress(QVariantMap))
    );

    m_monitorActive = true;
    return true;
}

void FlatpakUpdateService::closeMonitor() {
    if (!m_monitorActive) return;

    QDBusConnection bus = QDBusConnection::sessionBus();
    if (bus.isConnected() && !m_monitorPath.path().isEmpty()) {
        QDBusMessage closeMsg = QDBusMessage::createMethodCall(
            QStringLiteral("org.freedesktop.portal.Flatpak"),
            m_monitorPath.path(),
            QStringLiteral("org.freedesktop.portal.Flatpak.UpdateMonitor"),
            QStringLiteral("Close")
        );
        bus.call(closeMsg, QDBus::NoBlock);
    }
    m_monitorActive = false;
    m_monitorPath = QDBusObjectPath();
}

void FlatpakUpdateService::checkForUpdates(bool interactive) {
    if (!isSupported()) {
        m_status = UpdateStatus::Unsupported;
        if (interactive) {
            QMessageBox::information(
                QApplication::activeWindow(),
                tr("Check for Updates"),
                tr("<h3>Compositor</h3>"
                   "<p>Running in host developer mode.</p>"
                   "<p>On GNU/Linux Flatpak releases, updates are monitored automatically via "
                   "<code>org.freedesktop.portal.Flatpak.UpdateMonitor</code>.</p>"
                   "<p>On macOS, updates are managed by Sparkle 2.10.x.</p>")
            );
        }
        return;
    }

    QString error;
    if (!ensurePortalMonitor(&error)) {
        m_status = UpdateStatus::Error;
        if (interactive) {
            QMessageBox::warning(
                QApplication::activeWindow(),
                tr("Check for Updates"),
                tr("Unable to contact Flatpak update portal:<br><br>%1").arg(error)
            );
        }
        return;
    }

    if (m_status == UpdateStatus::ReadyToRestart) {
        if (interactive) {
            auto res = QMessageBox::question(
                QApplication::activeWindow(),
                tr("Update Ready"),
                tr("An update has already been installed.<br><br>Restart Compositor now to use the updated version?"),
                QMessageBox::Yes | QMessageBox::No
            );
            if (res == QMessageBox::Yes) {
                QApplication::quit();
            }
        }
        return;
    }

    if (m_hasUpdate) {
        if (interactive) {
            auto res = QMessageBox::question(
                QApplication::activeWindow(),
                tr("Update Available"),
                tr("A newer version of Compositor is available.<br><br>Would you like to install it now?"),
                QMessageBox::Yes | QMessageBox::No
            );
            if (res == QMessageBox::Yes) {
                installUpdate();
            }
        }
        return;
    }

    m_status = UpdateStatus::NoUpdate;
    if (interactive) {
        QMessageBox::information(
            QApplication::activeWindow(),
            tr("Check for Updates"),
            tr("You are running the latest version of Compositor.<br><br>"
               "Update monitoring is active via <code>org.freedesktop.portal.Flatpak.UpdateMonitor</code>.")
        );
    }
}

void FlatpakUpdateService::installUpdate() {
    if (!m_monitorActive || m_monitorPath.path().isEmpty()) return;

    QDBusMessage msg = QDBusMessage::createMethodCall(
        QStringLiteral("org.freedesktop.portal.Flatpak"),
        m_monitorPath.path(),
        QStringLiteral("org.freedesktop.portal.Flatpak.UpdateMonitor"),
        QStringLiteral("Update")
    );
    msg << QString() << QVariantMap();

    m_status = UpdateStatus::Downloading;
    QDBusConnection::sessionBus().call(msg, QDBus::NoBlock);
}

void FlatpakUpdateService::onUpdateAvailable(const QVariantMap &updateInfo) {
    m_info.runningCommit = updateInfo.value(QStringLiteral("running-commit")).toString();
    m_info.localCommit = updateInfo.value(QStringLiteral("local-commit")).toString();
    m_info.remoteCommit = updateInfo.value(QStringLiteral("remote-commit")).toString();

    if (!m_info.localCommit.isEmpty() && m_info.localCommit != m_info.runningCommit) {
        m_status = UpdateStatus::ReadyToRestart;
        m_hasUpdate = true;
    } else if (!m_info.remoteCommit.isEmpty() && m_info.remoteCommit != m_info.runningCommit) {
        m_status = UpdateStatus::UpdateAvailable;
        m_hasUpdate = true;
    }
}

void FlatpakUpdateService::onProgress(const QVariantMap &info) {
    const uint status = info.value(QStringLiteral("status")).toUInt();
    if (status == 1) {
        // Success
        m_status = UpdateStatus::ReadyToRestart;
        QMessageBox::information(
            QApplication::activeWindow(),
            tr("Update Installed"),
            tr("The update has been installed successfully.<br><br>Please restart Compositor.")
        );
    } else if (status > 1) {
        // Error
        m_status = UpdateStatus::Error;
        const QString err = info.value(QStringLiteral("error")).toString();
        QMessageBox::warning(
            QApplication::activeWindow(),
            tr("Update Error"),
            tr("Failed to install update: %1").arg(err.isEmpty() ? tr("Unknown error") : err)
        );
    }
}

}  // namespace qtplatform
