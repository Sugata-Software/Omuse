// SwiftUIQtRenderer — builds a real Qt widget tree from `compositor_session_render_tree`'s JSON, one generic
// interpreter for every panel instead of a hand-mirrored Qt reimplementation per panel. See
// docs/platform-abstraction.md and the "Generic SwiftUI→Qt compat runtime" plan.
#pragma once

#include <QString>
#include <QWidget>
#include <cstdint>

#include <functional>

/// Renders `panel` (a name `Sources/LinuxBridge/SwiftUIBridge.swift` recognises, e.g. "NavigationToolHeader")
/// against the session `sessionHandle` already identifies, as a standalone (unparented) `QWidget` tree. Returns
/// `nullptr` if the panel name is unknown to the Swift side, the session handle is invalid, or the tree is empty.
QWidget *swiftUIRenderPanel(uint64_t sessionHandle, const QString &panel, QWidget *existing = nullptr);

/// Registers a listener called on the owning object's event loop after any SwiftUI action handler finishes dispatching.
void registerSwiftUIActionListener(QObject *owner, std::function<void(uint64_t handle, const QString &panel)> listener);

