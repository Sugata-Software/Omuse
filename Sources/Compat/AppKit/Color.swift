import Foundation
import CoreGraphics

/// `NSColor` for the subset upstream uses: device-independent sRGB values with a `cgColor`, and drawing helpers
/// that set the colour on the current `NSGraphicsContext`.
open class NSColor: @unchecked Sendable {
    public let redComponent: CGFloat
    public let greenComponent: CGFloat
    public let blueComponent: CGFloat
    public let alphaComponent: CGFloat

    public init(srgbRed red: CGFloat, green: CGFloat, blue: CGFloat, alpha: CGFloat) {
        redComponent = red; greenComponent = green; blueComponent = blue; alphaComponent = alpha
    }
    public convenience init(red: CGFloat, green: CGFloat, blue: CGFloat, alpha: CGFloat) {
        self.init(srgbRed: red, green: green, blue: blue, alpha: alpha)
    }
    public convenience init(calibratedRed red: CGFloat, green: CGFloat, blue: CGFloat, alpha: CGFloat) {
        self.init(srgbRed: red, green: green, blue: blue, alpha: alpha)
    }
    public convenience init(white: CGFloat, alpha: CGFloat) {
        self.init(srgbRed: white, green: white, blue: white, alpha: alpha)
    }
    public convenience init(calibratedWhite white: CGFloat, alpha: CGFloat) {
        self.init(srgbRed: white, green: white, blue: white, alpha: alpha)
    }
    public convenience init(cgColor: CGColor) {
        self.init(srgbRed: cgColor.red, green: cgColor.green, blue: cgColor.blue, alpha: cgColor.alpha)
    }

    public var cgColor: CGColor {
        CGColor(red: redComponent, green: greenComponent, blue: blueComponent, alpha: alphaComponent)
    }
    public var whiteComponent: CGFloat { (redComponent + greenComponent + blueComponent) / 3 }

    public func withAlphaComponent(_ alpha: CGFloat) -> NSColor {
        NSColor(srgbRed: redComponent, green: greenComponent, blue: blueComponent, alpha: alpha)
    }

    /// All colours here are sRGB values; converting between the spaces the model asks for keeps the components.
    public func usingColorSpace(_ space: NSColorSpace) -> NSColor? { self }

    public func setFill() { NSGraphicsContext.current?.cgContext.setFillColor(cgColor) }
    public func setStroke() { NSGraphicsContext.current?.cgContext.setStrokeColor(cgColor) }
    public func set() { setFill(); setStroke() }

    public static let white = NSColor(white: 1, alpha: 1)
    public static let black = NSColor(white: 0, alpha: 1)
    public static let clear = NSColor(white: 0, alpha: 0)
    public static let gray = NSColor(white: 0.5, alpha: 1)
    public static let red = NSColor(srgbRed: 1, green: 0, blue: 0, alpha: 1)
    public static let green = NSColor(srgbRed: 0, green: 1, blue: 0, alpha: 1)
    public static let blue = NSColor(srgbRed: 0, green: 0, blue: 1, alpha: 1)
    /// The system accent colour; a fixed blue on Linux (the Qt shell owns real theming).
    public static let controlAccentColor = NSColor(srgbRed: 0, green: 0.478, blue: 1, alpha: 1)
}

public final class NSColorSpace: @unchecked Sendable {
    public let name: String
    private init(_ name: String) { self.name = name }
    public static let sRGB = NSColorSpace("sRGB")
    public static let deviceRGB = NSColorSpace("deviceRGB")
    public static let genericRGB = NSColorSpace("genericRGB")
    public static let displayP3 = NSColorSpace("displayP3")
    public static let genericGamma22Gray = NSColorSpace("genericGamma22Gray")
    public static let deviceGray = NSColorSpace("deviceGray")
}
