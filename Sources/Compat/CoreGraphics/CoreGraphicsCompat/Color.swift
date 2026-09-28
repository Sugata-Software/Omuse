// CoreGraphicsCompat/Color.swift — CGColor, CGColorSpace, CGBlendMode shims.
//
// Plan §3, §5: CoreGraphics compatibility types for color and blending.
// Maps blend modes to Skia blend mode integers (SkBlendMode).

import Foundation

// MARK: - CGColorSpace

public final class CGColorSpace: @unchecked Sendable, Equatable {
    public enum Model: Sendable {
        case rgb
        case monochrome
        case unknown
    }

    public let name: String
    public let model: Model

    // Apple's colour-space names are CFString constants (`CGColorSpace.sRGB`), and `CGColorSpace(name:)` is failable.
    public static let sRGB: CFString = "kCGColorSpaceSRGB"
    public static let extendedSRGB: CFString = "kCGColorSpaceExtendedSRGB"
    public static let linearSRGB: CFString = "kCGColorSpaceLinearSRGB"
    public static let extendedLinearSRGB: CFString = "kCGColorSpaceExtendedLinearSRGB"
    public static let displayP3: CFString = "kCGColorSpaceDisplayP3"
    public static let adobeRGB1998: CFString = "kCGColorSpaceAdobeRGB1998"
    public static let genericRGB: CFString = "kCGColorSpaceGenericRGB"
    public static let genericGrayGamma2_2: CFString = "kCGColorSpaceGenericGrayGamma2_2"
    public static let extendedGray: CFString = "kCGColorSpaceExtendedGray"
    public static let linearGray: CFString = "kCGColorSpaceLinearGray"

    /// The colour spaces this compat knows, as shared instances (all pixel storage here is 8-bit sRGB-like).
    public static let srgbSpace = CGColorSpace(name: sRGB, model: .rgb)
    public static let deviceRGBSpace = CGColorSpace(name: "kCGColorSpaceDeviceRGB", model: .rgb)
    public static let deviceGraySpace = CGColorSpace(name: "kCGColorSpaceDeviceGray", model: .monochrome)

    public init?(name: CFString) {
        switch name {
        case Self.sRGB, Self.extendedSRGB, Self.linearSRGB, Self.extendedLinearSRGB, Self.displayP3,
             Self.adobeRGB1998, Self.genericRGB, "kCGColorSpaceDeviceRGB":
            self.name = name; self.model = .rgb
        case Self.genericGrayGamma2_2, Self.extendedGray, Self.linearGray, "kCGColorSpaceDeviceGray":
            self.name = name; self.model = .monochrome
        default: return nil
        }
    }

    init(name: String, model: Model) {
        self.name = name
        self.model = model
    }

    public static func == (lhs: CGColorSpace, rhs: CGColorSpace) -> Bool {
        lhs.name == rhs.name && lhs.model == rhs.model
    }
}

public func CGColorSpaceCreateDeviceRGB() -> CGColorSpace {
    .deviceRGBSpace
}

public func CGColorSpaceCreateDeviceGray() -> CGColorSpace {
    .deviceGraySpace
}

public func CGColorSpaceCreateWithName(_ name: CFString) -> CGColorSpace? {
    CGColorSpace(name: name)
}

// MARK: - CGColor

public struct CGColor: Equatable, Sendable {
    public var red: CGFloat
    public var green: CGFloat
    public var blue: CGFloat
    public var alpha: CGFloat
    public var colorSpace: CGColorSpace

    public var components: [CGFloat]? {
        if colorSpace.model == .monochrome {
            return [red, alpha]
        }
        return [red, green, blue, alpha]
    }

    public var numberOfComponents: Int {
        colorSpace.model == .monochrome ? 2 : 4
    }

    public init(red: CGFloat, green: CGFloat, blue: CGFloat, alpha: CGFloat) {
        self.red = red
        self.green = green
        self.blue = blue
        self.alpha = alpha
        self.colorSpace = .srgbSpace
    }

    public init(gray: CGFloat, alpha: CGFloat) {
        self.red = gray
        self.green = gray
        self.blue = gray
        self.alpha = alpha
        self.colorSpace = .deviceGraySpace
    }

    /// sRGB colour from components already in sRGB (Apple's `CGColor(srgbRed:green:blue:alpha:)`).
    public init(srgbRed red: CGFloat, green: CGFloat, blue: CGFloat, alpha: CGFloat) {
        self.init(red: red, green: green, blue: blue, alpha: alpha)
    }

    public init(genericGrayGamma2_2Gray gray: CGFloat, alpha: CGFloat) {
        self.init(gray: gray, alpha: alpha)
    }

    /// Failable like Apple's: nil when `components` is shorter than the space needs (gray + alpha, or RGB + alpha).
    public init?(colorSpace: CGColorSpace, components: [CGFloat]) {
        let needed = colorSpace.model == .monochrome ? 2 : 4
        guard components.count >= needed else { return nil }
        self.colorSpace = colorSpace
        if colorSpace.model == .monochrome {
            self.red = components.first ?? 0
            self.green = components.first ?? 0
            self.blue = components.first ?? 0
            self.alpha = components.count > 1 ? components[1] : 1
        } else {
            self.red = components.count > 0 ? components[0] : 0
            self.green = components.count > 1 ? components[1] : 0
            self.blue = components.count > 2 ? components[2] : 0
            self.alpha = components.count > 3 ? components[3] : 1
        }
    }

    public static let white = CGColor(gray: 1, alpha: 1)
    public static let black = CGColor(gray: 0, alpha: 1)
    public static let clear = CGColor(red: 0, green: 0, blue: 0, alpha: 0)
}

// MARK: - CGBlendMode

public enum CGBlendMode: Int32, Sendable, CaseIterable {
    case normal = 0
    case multiply = 1
    case screen = 2
    case overlay = 3
    case darken = 4
    case lighten = 5
    case colorDodge = 6
    case colorBurn = 7
    case softLight = 8
    case hardLight = 9
    case difference = 10
    case exclusion = 11
    case hue = 12
    case saturation = 13
    case color = 14
    case luminosity = 15
    case clear = 16
    case copy = 17
    case sourceIn = 18
    case sourceOut = 19
    case sourceAtop = 20
    case destinationOver = 21
    case destinationIn = 22
    case destinationOut = 23
    case destinationAtop = 24
    case xor = 25
    case plusDarker = 26
    case plusLighter = 27
}

