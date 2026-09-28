// AppKit compat for Linux (headless). Upstream's document/IO/rendering code reaches a small AppKit surface:
// colours, graphics-context state, cursors as tokens, alerts/open panels/pasteboard from model code, and a text
// stack for the Type tool. Real behaviour lives where it can (NSColor, NSGraphicsContext, NSPasteboard); user
// interaction (alerts, panels) goes through injectable hooks the Qt host installs; the text stack is inert until
// the Skia paragraph backend lands (see docs/upstream-shim-worklist.md).
//
// Like Apple's AppKit, this module re-exports Foundation, CoreGraphics and Observation.

@_exported import Foundation
@_exported import CoreGraphics
@_exported import Observation

public typealias NSPoint = CGPoint
public typealias NSSize = CGSize
public typealias NSRect = CGRect
