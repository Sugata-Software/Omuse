#pragma once

// IUpdateService — cross-platform application update abstraction.
//
// Architecture:
//   UpdateService
//   ├── macOS
//   │   └── Sparkle 2.10.x (SPUStandardUpdaterController / SPUUpdater)
//   │
//   └── Linux / Flatpak
//       └── org.freedesktop.portal.Flatpak.UpdateMonitor
//
// High-level editor UI calls checkForUpdates(interactive). Platform-specific
// mechanisms handle background checks, portal interaction, and update staging.

#include <QString>
#include <memory>

enum class UpdateStatus {
    Idle,
    Checking,
    UpdateAvailable,
    NoUpdate,
    Downloading,
    ReadyToRestart,
    Error,
    Unsupported
};

struct UpdateInfo {
    QString runningCommit;
    QString localCommit;
    QString remoteCommit;
};

class IUpdateService {
public:
    virtual ~IUpdateService() = default;

    /// Checks for updates. If interactive is true, provides user feedback dialogs.
    virtual void checkForUpdates(bool interactive = true) = 0;

    /// Requests installation of an available update.
    virtual void installUpdate() = 0;

    /// Current status of the updater.
    virtual UpdateStatus status() const = 0;

    /// Current update metadata (commits, release notes).
    virtual UpdateInfo updateInfo() const = 0;

    /// Returns true if update monitoring is supported in this runtime environment.
    virtual bool isSupported() const = 0;
};
