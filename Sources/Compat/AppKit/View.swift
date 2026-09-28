import Foundation
import CoreGraphics
import FoundationCompat

/// Objective-C selectors are just names here (target/action wiring is the Qt shell's job).
public struct Selector: Hashable, ExpressibleByStringLiteral, Sendable {
    public let name: String
    public init(_ name: String) { self.name = name }
    public init(stringLiteral value: String) { name = value }
}

/// The AppKit responder chain, headless: enough hierarchy, geometry and event dispatch for the editor's canvas view to be
/// driven by synthesised events (from tests, or from Qt translating its own events).
@MainActor open class NSResponder {
    public init() {}
    open var acceptsFirstResponder: Bool { false }
    open func becomeFirstResponder() -> Bool { true }
    open func resignFirstResponder() -> Bool { true }
    open var nextResponder: NSResponder?
    open var undoManager: UndoManager? { nil }
    open func mouseDown(with event: NSEvent) {}
    open func mouseUp(with event: NSEvent) {}
    open func mouseDragged(with event: NSEvent) {}
    open func mouseMoved(with event: NSEvent) {}
    open func mouseEntered(with event: NSEvent) {}
    open func mouseExited(with event: NSEvent) {}
    open func rightMouseDown(with event: NSEvent) {}
    open func rightMouseUp(with event: NSEvent) {}
    open func rightMouseDragged(with event: NSEvent) {}
    open func otherMouseDown(with event: NSEvent) {}
    open func otherMouseUp(with event: NSEvent) {}
    open func otherMouseDragged(with event: NSEvent) {}
    open func scrollWheel(with event: NSEvent) {}
    open func magnify(with event: NSEvent) {}
    open func keyDown(with event: NSEvent) {}
    open func keyUp(with event: NSEvent) {}
    open func flagsChanged(with event: NSEvent) {}
    open func cursorUpdate(with event: NSEvent) {}
    open func tabletPoint(with event: NSEvent) {}
    open func tabletProximity(with event: NSEvent) {}
    open func performKeyEquivalent(with event: NSEvent) -> Bool { false }
}

public struct NSTrackingAreaOptions: OptionSet, Sendable {
    public let rawValue: UInt
    public init(rawValue: UInt) { self.rawValue = rawValue }
    public static let mouseEnteredAndExited = NSTrackingAreaOptions(rawValue: 1 << 0), mouseMoved = NSTrackingAreaOptions(rawValue: 1 << 1)
    public static let cursorUpdate = NSTrackingAreaOptions(rawValue: 1 << 2)
    public static let activeWhenFirstResponder = NSTrackingAreaOptions(rawValue: 1 << 4)
    public static let activeInKeyWindow = NSTrackingAreaOptions(rawValue: 1 << 5), activeInActiveApp = NSTrackingAreaOptions(rawValue: 1 << 6)
    public static let activeAlways = NSTrackingAreaOptions(rawValue: 1 << 7), inVisibleRect = NSTrackingAreaOptions(rawValue: 1 << 9)
}
public final class NSTrackingArea {
    public typealias Options = NSTrackingAreaOptions
    public let rect: CGRect, options: NSTrackingAreaOptions
    public weak var owner: AnyObject?
    public init(rect: CGRect, options: NSTrackingAreaOptions, owner: AnyObject?, userInfo: [AnyHashable: Any]? = nil) {
        self.rect = rect; self.options = options; self.owner = owner
    }
}

@MainActor open class NSView: NSResponder, @unchecked Sendable {
    public var frame: CGRect { didSet { if frame.size != oldValue.size { needsLayout = true } } }
    private var boundsOverride: CGRect?
    public var bounds: CGRect {
        get { boundsOverride ?? CGRect(origin: .zero, size: frame.size) }
        set { boundsOverride = newValue }
    }
    public var frameRotation: CGFloat = 0
    public private(set) var subviews: [NSView] = []
    public private(set) weak var superview: NSView?
    public fileprivate(set) weak var hostWindow: NSWindow?
    public var window: NSWindow? { hostWindow ?? superview?.window }
    public var isHidden = false
    public var wantsLayer = false
    public var autoresizingMask: AutoresizingMask = []
    public struct AutoresizingMask: OptionSet, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let width = AutoresizingMask(rawValue: 1 << 0)
        public static let height = AutoresizingMask(rawValue: 1 << 1)
        public static let minXMargin = AutoresizingMask(rawValue: 1 << 2)
        public static let minYMargin = AutoresizingMask(rawValue: 1 << 3)
        public static let maxXMargin = AutoresizingMask(rawValue: 1 << 4)
        public static let maxYMargin = AutoresizingMask(rawValue: 1 << 5)
    }
    open var layer: CALayer? { wantsLayer ? backingLayer : nil }
    private lazy var backingLayer = CALayer()
    public var clipsToBounds = false
    public var needsDisplay = false
    public var needsLayout = false
    public private(set) var trackingAreas: [NSTrackingArea] = []
    public var backingScaleFactor: CGFloat { window?.backingScaleFactor ?? 1 }
    open var isFlipped: Bool { false }
    open var isOpaque: Bool { false }
    open func acceptsFirstMouse(for event: NSEvent?) -> Bool { false }
    open var visibleRect: CGRect { bounds }

    public init(frame: CGRect) { self.frame = frame; super.init() }
    public override convenience init() { self.init(frame: .zero) }
    public required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }

    open func addSubview(_ view: NSView) {
        view.removeFromSuperview()
        view.superview = self; subviews.append(view)
        view.windowDidChange()
    }
    open func removeFromSuperview() {
        superview?.subviews.removeAll { $0 === self }
        superview = nil
        windowDidChange()
    }
    /// Tells this view and everything below it that the window it belongs to may have changed (AppKit sends
    /// `viewDidMoveToWindow` down the whole subtree when a hierarchy is attached to or detached from a window).
    func windowDidChange() {
        notifiedWindow = window
        viewDidMoveToWindow()
        for child in subviews where child.notifiedWindow !== window { child.windowDidChange() }
    }
    private weak var notifiedWindow: NSWindow?
    open func setFrameOrigin(_ origin: CGPoint) { frame.origin = origin }
    open func setFrameSize(_ size: CGSize) { frame.size = size }
    open func setNeedsDisplay(_ rect: CGRect) { needsDisplay = true }
    open func setNeedsDisplay() { needsDisplay = true }
    /// A bitmap sized for this view's rect at the window's backing scale, to draw into with `cacheDisplay`.
    open func bitmapImageRepForCachingDisplay(in rect: CGRect) -> NSBitmapImageRep? {
        let w = max(1, Int((rect.width * backingScaleFactor).rounded(.up))), h = max(1, Int((rect.height * backingScaleFactor).rounded(.up)))
        // A real Core Graphics bitmap context: origin bottom-left, so images draw upright in a non-flipped view.
        guard let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        return NSBitmapImageRep(context: ctx)
    }
    /// Renders the view into `rep` by running its `draw(_:)` with the rep's context current, in the view's own
    /// coordinates: y-up for regular views, y-down for flipped ones (the context itself is top-left based).
    open func cacheDisplay(in rect: CGRect, to rep: NSBitmapImageRep) {
        guard let ctx = rep.context else { return }
        NSGraphicsContext.saveGraphicsState()
        defer { NSGraphicsContext.restoreGraphicsState() }
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: isFlipped)
        ctx.saveGState()
        ctx.scaleBy(x: backingScaleFactor, y: backingScaleFactor)
        // The bitmap context is y-up like Core Graphics's, so a flipped view flips it back to its own y-down space.
        if isFlipped { ctx.translateBy(x: 0, y: rect.height); ctx.scaleBy(x: 1, y: -1) }
        ctx.translateBy(x: -rect.minX, y: -rect.minY)
        viewWillDraw()
        draw(rect)
        for child in subviews where !child.isHidden {
            ctx.saveGState()
            ctx.translateBy(x: child.frame.minX, y: child.frame.minY)
            child.draw(child.bounds)
            ctx.restoreGState()
        }
        ctx.restoreGState()
    }
    open func displayIfNeeded() { if needsDisplay { viewWillDraw(); needsDisplay = false } }
    open func layoutSubtreeIfNeeded() { if needsLayout { layout(); needsLayout = false } }
    open func layout() {}
    open func viewWillDraw() {}
    open func viewDidMoveToWindow() {}
    open func viewDidChangeBackingProperties() {}
    open func updateTrackingAreas() {}
    open func addTrackingArea(_ area: NSTrackingArea) { trackingAreas.append(area) }
    open func removeTrackingArea(_ area: NSTrackingArea) { trackingAreas.removeAll { $0 === area } }
    open func addCursorRect(_ rect: CGRect, cursor: NSCursor) {}
    open func resetCursorRects() {}
    open func discardCursorRects() {}
    open func hitTest(_ point: CGPoint) -> NSView? {
        guard !isHidden, frame.contains(point) else { return nil }
        let local = CGPoint(x: point.x - frame.minX, y: point.y - frame.minY)
        for child in subviews.reversed() { if let hit = child.hitTest(local) { return hit } }
        return self
    }
    open func draw(_ dirtyRect: CGRect) {}
    open func setAccessibilityElement(_ isElement: Bool) {}
    open func setAccessibilityIdentifier(_ identifier: String?) {}
    open func setAccessibilityLabel(_ label: String?) {}
    open func setAccessibilityRole(_ role: NSAccessibility.Role?) {}

    // MARK: Geometry conversion (translation only; enough for unrotated view trees)

    /// Origin of this view in window coordinates.
    private var originInWindow: CGPoint {
        var p = frame.origin, v = superview
        while let s = v { p.x += s.frame.origin.x; p.y += s.frame.origin.y; v = s.superview }
        return p
    }
    open func convert(_ point: CGPoint, from view: NSView?) -> CGPoint {
        if view == nil {
            let winHeight = window?.frame.height ?? bounds.height
            let localX = point.x - originInWindow.x
            let localY = isFlipped ? (winHeight - point.y - originInWindow.y) : (point.y - originInWindow.y)
            return CGPoint(x: localX, y: localY)
        }
        let source = view?.originInWindow ?? .zero, mine = originInWindow
        var result = CGPoint(x: point.x + source.x - mine.x, y: point.y + source.y - mine.y)
        if (view?.isFlipped ?? false) != self.isFlipped {
            result.y = bounds.height - result.y
        }
        return result
    }
    open func convert(_ point: CGPoint, to view: NSView?) -> CGPoint {
        if view == nil {
            let winHeight = window?.frame.height ?? bounds.height
            let winX = point.x + originInWindow.x
            let winY = isFlipped ? (winHeight - (point.y + originInWindow.y)) : (point.y + originInWindow.y)
            return CGPoint(x: winX, y: winY)
        }
        let target = view?.originInWindow ?? .zero, mine = originInWindow
        var result = CGPoint(x: point.x + mine.x - target.x, y: point.y + mine.y - target.y)
        if self.isFlipped != (view?.isFlipped ?? false) {
            result.y = (view?.bounds.height ?? bounds.height) - result.y
        }
        return result
    }
    open func convert(_ rect: CGRect, from view: NSView?) -> CGRect { CGRect(origin: convert(rect.origin, from: view), size: rect.size) }
    open func convert(_ rect: CGRect, to view: NSView?) -> CGRect { CGRect(origin: convert(rect.origin, to: view), size: rect.size) }
    open func convertToBacking(_ point: CGPoint) -> CGPoint { CGPoint(x: point.x * backingScaleFactor, y: point.y * backingScaleFactor) }
    open func convertFromBacking(_ point: CGPoint) -> CGPoint { CGPoint(x: point.x / backingScaleFactor, y: point.y / backingScaleFactor) }
    open func convertToBacking(_ size: CGSize) -> CGSize { CGSize(width: size.width * backingScaleFactor, height: size.height * backingScaleFactor) }
    open func convertToBacking(_ rect: CGRect) -> CGRect { rect.applying(CGAffineTransform(scaleX: backingScaleFactor, y: backingScaleFactor)) }
}

public enum NSAccessibility {
    public struct Role: RawRepresentable, Sendable { public let rawValue: String; public init(rawValue: String) { self.rawValue = rawValue }
        public static let button = Role(rawValue: "AXButton"), image = Role(rawValue: "AXImage"), group = Role(rawValue: "AXGroup")
        public static let unknown = Role(rawValue: "AXUnknown") }
}

@MainActor open class NSViewController: NSResponder {
    open var representedRootView: Any?
    open var view = NSView(frame: .zero)
    public override init() { super.init() }
}

@MainActor public protocol SheetAutoResolving { func resolveWithoutHost() }

@MainActor open class NSWindow: NSResponder {
    public struct StyleMask: OptionSet, Sendable {
        public let rawValue: UInt
        public init(rawValue: UInt) { self.rawValue = rawValue }
        public static let borderless = StyleMask([])
        public static let titled = StyleMask(rawValue: 1 << 0), closable = StyleMask(rawValue: 1 << 1)
        public static let miniaturizable = StyleMask(rawValue: 1 << 2), resizable = StyleMask(rawValue: 1 << 3)
        public static let fullSizeContentView = StyleMask(rawValue: 1 << 15)
    }
    public enum BackingStoreType: UInt, Sendable { case retained, nonretained, buffered }

    nonisolated(unsafe) private static var registry: [Int: WeakWindow] = [:]
    nonisolated(unsafe) private static var counter = 100
    private final class WeakWindow { weak var window: NSWindow?; init(_ w: NSWindow) { window = w } }
    nonisolated static func window(numbered n: Int) -> NSWindow? { registry[n]?.window }
    static var allWindows: [NSWindow] { registry.keys.sorted().compactMap { registry[$0]?.window } }

    public var title = ""
    public var isDocumentEdited = false
    public var representedURL: URL?
    public var styleMask: StyleMask = []
    public var backingScaleFactor: CGFloat = 1
    public var frame: CGRect
    public let windowNumber: Int
    public var contentViewController: NSViewController?
    public var contentView: NSView? {
        didSet {
            // AppKit fills the window's content rect with its content view.
            contentView?.frame = CGRect(origin: .zero, size: frame.size)
            contentView?.hostWindow = self
            contentView?.windowDidChange()
        }
    }
    public private(set) var firstResponder: NSResponder?
    public var isKeyWindow = true
    public override var undoManager: UndoManager? { windowUndoManager }
    private let windowUndoManager = UndoManager()

    public init(contentRect: CGRect, styleMask: StyleMask, backing: BackingStoreType, defer flag: Bool) {
        frame = contentRect; self.styleMask = styleMask
        NSWindow.counter += 1; windowNumber = NSWindow.counter
        super.init()
        NSWindow.registry[windowNumber] = WeakWindow(self)
    }
    public override convenience init() { self.init(contentRect: .zero, styleMask: [], backing: .buffered, defer: true) }

    @discardableResult open func makeFirstResponder(_ responder: NSResponder?) -> Bool {
        if let current = firstResponder, !current.resignFirstResponder() { return false }
        guard responder?.becomeFirstResponder() ?? true else { return false }
        // Like AppKit, focusing an editable text field puts the shared field editor (an NSText) in the responder chain.
        if let field = responder as? NSTextField, field.isEditable {
            let editor = fieldEditor(true, for: field)
            firstResponder = editor
        } else {
            firstResponder = responder
        }
        return true
    }
    private var sharedFieldEditor: NSTextView?
    open func fieldEditor(_ createIfNeeded: Bool, for object: Any?) -> NSText? {
        if sharedFieldEditor == nil, createIfNeeded {
            let editor = NSTextView(frame: .zero)
            editor.isFieldEditor = true
            sharedFieldEditor = editor
        }
        return sharedFieldEditor
    }
    public var isVisible = false
    public var identifier: NSUserInterfaceItemIdentifier?
    open func makeKeyAndOrderFront(_ sender: Any?) { isVisible = true }
    open func orderFront(_ sender: Any?) { isVisible = true }
    open func orderFrontRegardless() { isVisible = true }
    public var attachedSheet: NSWindow?
    /// The pointer in window coordinates; the host updates `NSEvent.mouseLocation`.
    open var mouseLocationOutsideOfEventStream: CGPoint { convertPoint(fromScreen: NSEvent.mouseLocation) }
    open func close() { isVisible = false }
    open func orderOut(_ sender: Any?) { isVisible = false }
    open func disableCursorRects() {}
    open func enableCursorRects() {}
    open func invalidateCursorRects(for view: NSView) {}
    open func convertPoint(toScreen point: CGPoint) -> CGPoint { CGPoint(x: point.x + frame.minX, y: point.y + frame.minY) }
    open func convertPoint(fromScreen point: CGPoint) -> CGPoint { CGPoint(x: point.x - frame.minX, y: point.y - frame.minY) }

    /// Routes an event to the first responder like `NSApplication.sendEvent`: local monitors first, then the responder.
    open func sendEvent(_ event: NSEvent) {
        guard let event = NSEvent.filterThroughLocalMonitors(event) else { return }
        let target: NSResponder? = (event.type == .keyDown || event.type == .keyUp || event.type == .flagsChanged)
            ? firstResponder : (contentView?.hitTest(event.locationInWindow) ?? contentView)
        switch event.type {
        case .leftMouseDown: target?.mouseDown(with: event)
        case .leftMouseUp: target?.mouseUp(with: event)
        case .leftMouseDragged: target?.mouseDragged(with: event)
        case .mouseMoved: target?.mouseMoved(with: event)
        case .rightMouseDown: target?.rightMouseDown(with: event)
        case .rightMouseUp: target?.rightMouseUp(with: event)
        case .rightMouseDragged: target?.rightMouseDragged(with: event)
        case .scrollWheel: target?.scrollWheel(with: event)
        case .magnify: target?.magnify(with: event)
        case .keyDown: if !(target?.performKeyEquivalent(with: event) ?? false) { target?.keyDown(with: event) }
        case .keyUp: target?.keyUp(with: event)
        case .flagsChanged: target?.flagsChanged(with: event)
        case .mouseEntered: target?.mouseEntered(with: event)
        case .mouseExited: target?.mouseExited(with: event)
        case .cursorUpdate: target?.cursorUpdate(with: event)
        case .tabletPoint: target?.tabletPoint(with: event)
        case .tabletProximity: target?.tabletProximity(with: event)
        case .otherMouseDown, .otherMouseUp, .otherMouseDragged: break
        }
    }

    /// The Qt shell installs this to show a sheet; without a host the sheet's root view settles itself.
    nonisolated(unsafe) public static var sheetPresenter: ((_ parent: NSWindow, _ sheet: NSWindow) -> Void)?
    open func beginSheet(_ sheet: NSWindow, completionHandler: ((NSApplication.ModalResponse) -> Void)? = nil) {
        if let present = NSWindow.sheetPresenter { present(self, sheet); return }
        (sheet.contentViewController?.representedRootView as? SheetAutoResolving)?.resolveWithoutHost()
    }
    open func endSheet(_ sheet: NSWindow) {}
}

public struct NSScreen {
    public var frame = CGRect(x: 0, y: 0, width: 1440, height: 900)
    public var backingScaleFactor: CGFloat = 1
    @MainActor public static var main: NSScreen? = NSScreen()
    @MainActor public static var screens: [NSScreen] = [NSScreen()]
}

@MainActor public let NSApp = NSApplicationShared()
@MainActor public final class NSApplicationShared {
    public var currentEvent: NSEvent?
    public var keyWindow: NSWindow?
    public var mainWindow: NSWindow?
    public func sendEvent(_ event: NSEvent) { (event.window ?? keyWindow)?.sendEvent(event) }
    /// Live windows, oldest first.
    public var windows: [NSWindow] { NSWindow.allWindows }
    public func activate(ignoringOtherApps: Bool = true) {}
}

// MARK: - Controls the model layer instantiates (inert containers)

@MainActor open class NSControl: NSView {
    public var doubleValue = 0.0, floatValue: Float = 0, intValue: Int32 = 0
    public var stringValue = "", isEnabled = true
    public var target: AnyObject?
    public var action: Selector?
}
@MainActor open class NSTextField: NSControl {
    open var isEditable = true
    open var placeholderString: String?
    open var isBezeled = true
    public static func labelWithString(_ string: String) -> NSTextField { let f = NSTextField(frame: .zero); f.stringValue = string; return f }
}
@MainActor open class NSButton: NSControl {
    public var title = "", state = 0
    public var isBordered = true
}
@MainActor open class NSTableColumn { public var identifier: NSUserInterfaceItemIdentifier; public var width: CGFloat = 100
    public init(identifier: NSUserInterfaceItemIdentifier) { self.identifier = identifier } }
public struct NSUserInterfaceItemIdentifier: Hashable, RawRepresentable, ExpressibleByStringLiteral, Sendable {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public init(_ rawValue: String) { self.rawValue = rawValue }
    public init(stringLiteral value: String) { rawValue = value }
}

/// Table view container (inert): tests and the layers panel subclass it; rows are the Qt shell's job.
@MainActor open class NSTableView: NSControl {
    public weak var dataSource: AnyObject?
    public weak var delegate: AnyObject?
    public private(set) var tableColumns: [NSTableColumn] = []
    open var selectedRow: Int { selectedRowIndexes.first ?? -1 }
    open var selectedRowIndexes = IndexSet()
    open func addTableColumn(_ column: NSTableColumn) { tableColumns.append(column) }
    open func selectRowIndexes(_ indexes: IndexSet, byExtendingSelection extend: Bool) {
        selectedRowIndexes = extend ? selectedRowIndexes.union(indexes) : indexes
    }
    open func reloadData() {}
    open var numberOfRows: Int { 0 }
}
