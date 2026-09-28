// CoreGraphicsCompat/Gradient.swift — CGGradient and gradient drawing options.

import Foundation

public struct CGGradientDrawingOptions: OptionSet, Sendable {
    public let rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }
    public static let drawsBeforeStartLocation = CGGradientDrawingOptions(rawValue: 1 << 0)
    public static let drawsAfterEndLocation = CGGradientDrawingOptions(rawValue: 1 << 1)
}

public final class CGGradient: @unchecked Sendable {
    public let colorSpace: CGColorSpace
    /// Straight RGBA per stop, expanded from whatever component layout the caller used.
    let stops: [CGColor]
    let locations: [CGFloat]

    /// `CGGradient(colorSpace:colorComponents:locations:count:)` — components are packed per colour space
    /// (gray + alpha, or RGB + alpha).
    public init?(colorSpace: CGColorSpace, colorComponents components: [CGFloat]?, locations: [CGFloat]?, count: Int) {
        guard count >= 2, let components else { return nil }
        let per = colorSpace.model == .monochrome ? 2 : 4
        guard components.count >= count * per else { return nil }
        self.colorSpace = colorSpace
        var stops: [CGColor] = []
        for i in 0..<count {
            let slice = Array(components[(i * per)..<((i + 1) * per)])
            guard let color = CGColor(colorSpace: colorSpace, components: slice) else { return nil }
            stops.append(color)
        }
        self.stops = stops
        self.locations = Self.normalized(locations, count: count)
    }

    /// `CGGradient(colorsSpace:colors:locations:)` — `colors` holds `CGColor` values.
    public init?(colorsSpace: CGColorSpace?, colors: CFArray, locations: [CGFloat]?) {
        let cgColors = colors.compactMap { $0 as? CGColor }
        guard cgColors.count >= 2, cgColors.count == colors.count else { return nil }
        self.colorSpace = colorsSpace ?? .srgbSpace
        self.stops = cgColors
        self.locations = Self.normalized(locations, count: cgColors.count)
    }

    private static func normalized(_ locations: [CGFloat]?, count: Int) -> [CGFloat] {
        if let locations, locations.count >= count { return Array(locations.prefix(count)) }
        return (0..<count).map { CGFloat($0) / CGFloat(count - 1) }
    }

    /// Packed r,g,b,a floats and locations for the Skia ABI.
    var packedColors: [Float] {
        stops.flatMap { [Float($0.red), Float($0.green), Float($0.blue), Float($0.alpha)] }
    }
    var packedLocations: [Float] { locations.map { Float($0) } }
}
