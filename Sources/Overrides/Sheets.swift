// `Compositor/UI/{CanvasSizeSheet,ImageSizeSheet,JPEGExportSheet}.swift` are wired in unmodified (symlinked under
// Sources/UpstreamCore/UI/); this file only adds the Linux-only `SheetAutoResolving` conformance real upstream code
// has no reason to declare. Upstream's unmodified `ProjectController` builds these, hosts them with
// `NSHostingController` and presents them with `NSWindow.beginSheet`. Presentation is the Qt shell's job: it
// installs `NSWindow.sheetPresenter`, reads the sheet from `sheet.contentViewController?.representedRootView`,
// shows its own dialog and calls the completion. With no host installed, a sheet settles as "cancelled" via
// `resolveWithoutHost()`, so headless runs and tests never hang.

import Foundation
import AppKit
import SwiftUI

// Real composite views (each has its own `var body: some View`) — only the Linux-only auto-resolve conformance is
// added; they resolve through `body` like any other panel, not as `_Native` leaves.
extension CanvasSizeSheet: SheetAutoResolving { public func resolveWithoutHost() { finish(nil) } }
extension ImageSizeSheet: SheetAutoResolving { public func resolveWithoutHost() { finish(nil) } }
extension JPEGExportSheet: SheetAutoResolving { public func resolveWithoutHost() { finish(nil) } }
