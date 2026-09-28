import Foundation
import CoreGraphics

// The Type tool's text stack. Layout is approximate and drawing is a no-op until the Skia paragraph backend is
// wired in (Phase 3); the API mirrors what upstream's TypeTool.swift calls.

open class NSFont: @unchecked Sendable {
    public struct Weight: RawRepresentable, Hashable, Sendable {
        public let rawValue: CGFloat
        public init(rawValue: CGFloat) { self.rawValue = rawValue }
        public static let ultraLight = Weight(rawValue: -0.8), thin = Weight(rawValue: -0.6), light = Weight(rawValue: -0.4)
        public static let regular = Weight(rawValue: 0), medium = Weight(rawValue: 0.23), semibold = Weight(rawValue: 0.3)
        public static let bold = Weight(rawValue: 0.4), heavy = Weight(rawValue: 0.56), black = Weight(rawValue: 0.62)
    }
    public let fontName: String
    public let pointSize: CGFloat
    public init?(name: String, size: CGFloat) {
        guard !name.isEmpty else { return nil }
        fontName = name; pointSize = size
    }
    private init(system size: CGFloat) { fontName = "System"; pointSize = size }
    public static func systemFont(ofSize size: CGFloat) -> NSFont { NSFont(system: size) }
    public static func systemFont(ofSize size: CGFloat, weight: Weight) -> NSFont { NSFont(system: size) }
    public static func monospacedDigitSystemFont(ofSize size: CGFloat, weight: Weight = .regular) -> NSFont { NSFont(system: size) }
}

public enum NSTextAlignment: Int, Sendable { case left, right, center, justified, natural }
public enum NSLineBreakMode: Int, Sendable { case byWordWrapping, byCharWrapping, byClipping, byTruncatingHead, byTruncatingTail, byTruncatingMiddle }

open class NSParagraphStyle: @unchecked Sendable {
    open var alignment: NSTextAlignment = .natural
    open var minimumLineHeight: CGFloat = 0
    open var maximumLineHeight: CGFloat = 0
    open var lineBreakMode: NSLineBreakMode = .byWordWrapping
    public init() {}
}
open class NSMutableParagraphStyle: NSParagraphStyle, @unchecked Sendable {}

extension NSAttributedString.Key {
    public static let font = NSAttributedString.Key("NSFont")
    public static let foregroundColor = NSAttributedString.Key("NSColor")
    public static let paragraphStyle = NSAttributedString.Key("NSParagraphStyle")
    public static let kern = NSAttributedString.Key("NSKern")
}

public struct NSStringDrawingOptions: OptionSet, Sendable {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }
    public static let usesLineFragmentOrigin = NSStringDrawingOptions(rawValue: 1 << 0)
    public static let usesFontLeading = NSStringDrawingOptions(rawValue: 1 << 1)
}

extension NSAttributedString {
    /// Approximate measure (0.55 em per character, one line height per line) until real shaping is available.
    public func boundingRect(with size: CGSize, options: NSStringDrawingOptions = []) -> CGRect {
        let attrs = length > 0 ? attributes(at: 0, effectiveRange: nil) : [:]
        let point = (attrs[.font] as? NSFont)?.pointSize ?? 12
        let lines = string.split(separator: "\n", omittingEmptySubsequences: false)
        let widest = lines.map { CGFloat($0.count) * point * 0.55 }.max() ?? 0
        let paragraph = attrs[.paragraphStyle] as? NSParagraphStyle
        let lineHeight = (paragraph?.minimumLineHeight ?? 0) > 0 ? paragraph!.minimumLineHeight : point * 1.2
        return CGRect(x: 0, y: 0, width: min(widest, size.width), height: min(CGFloat(lines.count) * lineHeight, size.height))
    }
}

open class NSTextContainer {
    public var size: CGSize
    public var lineFragmentPadding: CGFloat = 5
    public var widthTracksTextView = false, heightTracksTextView = false
    public init(size: CGSize) { self.size = size }
}

open class NSLayoutManager {
    public weak var textStorage: NSTextStorage?
    public private(set) var textContainers: [NSTextContainer] = []

    public init() {}

    open func addTextContainer(_ container: NSTextContainer) {
        textContainers.append(container)
    }

    open func glyphRange(for container: NSTextContainer) -> NSRange {
        NSRange(location: 0, length: textStorage?.attributedString.length ?? 0)
    }

    open var numberOfGlyphs: Int {
        textStorage?.attributedString.length ?? 0
    }

    open func ensureLayout(for container: NSTextContainer) {}

    open func drawGlyphs(forGlyphRange glyphsToShow: NSRange, at origin: CGPoint) {
        guard let storage = textStorage,
              let context = NSGraphicsContext.current?.cgContext else { return }
        let attrString = storage.attributedString
        let fullString = attrString.string
        guard !fullString.isEmpty else { return }

        let containerWidth = textContainers.first?.size.width ?? CGFloat.greatestFiniteMagnitude
        let attrs = attrString.length > 0 ? attrString.attributes(at: 0, effectiveRange: nil) : [:]
        let font = (attrs[.font] as? NSFont) ?? NSFont.systemFont(ofSize: 12)
        let color = (attrs[.foregroundColor] as? NSColor) ?? NSColor.black
        let paragraph = attrs[.paragraphStyle] as? NSParagraphStyle
        let tracking = (attrs[.kern] as? CGFloat) ?? 0

        let pointSize = font.pointSize > 0 ? font.pointSize : 12
        let lineHeight = (paragraph?.minimumLineHeight ?? 0) > 0 ? paragraph!.minimumLineHeight : pointSize * 1.2

        context.setFillColor(color.cgColor)

        let charAdvance = pointSize * 0.55 + tracking
        let scaleX = (pointSize * 0.55) / 6.0
        let scaleY = pointSize / 11.0

        var currentX = origin.x
        var currentY = origin.y

        let glyphTable = Self.lazyGlyphTable

        for ch in fullString {
            if ch == "\n" {
                currentX = origin.x
                currentY += lineHeight
                continue
            }
            if ch == " " {
                currentX += charAdvance
                continue
            }
            if currentX + charAdvance > origin.x + containerWidth && currentX > origin.x {
                currentX = origin.x
                currentY += lineHeight
            }

            let glyph = Self.glyphBytes(for: ch, table: glyphTable)
            for row in 0..<11 {
                let rowByte = glyph[row]
                if rowByte == 0 { continue }
                var col = 0
                while col < 6 {
                    if (rowByte & (1 << (5 - col))) != 0 {
                        let startCol = col
                        while col < 6 && (rowByte & (1 << (5 - col))) != 0 {
                            col += 1
                        }
                        let span = col - startCol
                        let rx = currentX + CGFloat(startCol) * scaleX
                        let ry = currentY + CGFloat(row) * scaleY
                        context.fill(CGRect(x: rx, y: ry, width: max(1, CGFloat(span) * scaleX), height: max(1, ceil(scaleY))))
                    } else {
                        col += 1
                    }
                }
            }
            currentX += charAdvance
        }
    }

    private static let glyphBase64 = "AAAAAAAAAAAAAAAAABAQEBAQAAAQAAAAGBgYAAAAAAAAAAAKDAweFB4UGAAABA4VFRwGBRUOBAAAHBUVHgMFCQkAAAAOEBEPERERDwAAABAQEAAAAAAAAAAACBAQEBAQEAgIAAAAICAgICAgAAAAAAAAAAQVDgoAAAAAAAAEBB8EBAAAAAAAAAAAAAAAIAAAAAAAAAAwAAAAAAAAAAAAAAAAABAAAAAQECAgIAAAAAAAAA4bERERERsOAAAADBQEBAQEBAQAAAAMEgICBAgQPgAAABwiAgwCIiIcAAAAAgYKChI/AgIAAAAeICAsMgIiHAAAAA4JER4REREOAAAAPgIEBAgIEBAAAAAOEREOERERDgAAAA4REREPERIOAAAAAAAQAAAAABAAAAAAABAAAAAAECAAAAAAAAYYEAwCAAAAAAAAHgAeAAAAAAAAAAAYBgIMEAAAAAwSEgIEBAAEAAAAAwwLFBQVFggHAAAEDAoSHhERIQAAAB4RERIfEREeAAAABwgQEBAQCA8AAAAeERAQEBARHgAAAB8QEBAeEBAfAAAAHxAQEB4QEBAAAAAHCRAQExAJDgAAABAQEBAfEBAQAAAAEBAQEBAQEBAAAAACAgICAhISDAAAABESFBQcFBIRAAAAEBAQEBAQEB8AAAAYGBgVFRUWEgAAABkZGRUVFRMTAAAABwgQEBAQCAcAAAAeERERHhAQEAAAAAcIEBAQEAgHAAAAHhERER4TEREAAAAOERAIBwERDgAAAB8EBAQEBAQEAAAAEREREREREQ4AAAAhERESCgoMBAAAACMTExUVFAwIAAAAERIKDAwKEhEAAAAREQoKBAQEBAAAAB8BAgQECBAfAAAYEBAQEBAQEBAYAAAAAAAgICAgEBAAICAgICAgICAgIAAAAAAMFBQSAAAAAAAAAAAAAAAAAD4AAAAQAAAAAAAAAAAAAAAOEgYaEh4AAAAQEB4RERERHgAAAAAADhEQEBEOAAAAAQEPEREREQ8AAAAAAA4RHxARDgAACBAQOBAQEBAQAAAAAAAPEREREQ8RAAAQEB4ZEREREQAAABAAEBAQEBAQAAAAEAAQEBAQEBAQAAAQEBIUGBQUEgAAABAQEBAQEBAYAAAAAAAdEhISEhIAAAAAAB4ZEREREQAAAAAADhEREREOAAAAAAAeERERER4QAAAAAA8RERERDwEAAAAAHBAQEBAQAAAAAAAcEhgGEh4AAAAQEDgQEBAQGAAAAAAAERERERMPAAAAAAAiIhQUFAgAAAAAACYmFhoZCQAAAAAACCgQMCgIAAAAAAAiIhQUFAgIAAAAAB4CBAgQHgAAGBAQEBAQEBAQGAAQEBAQEBAQEBAQADAQEBAQEBAQEDAAAAAAAAAaFgAAAA=="

    private static let lazyGlyphTable: [UInt8] = {
        if let data = Data(base64Encoded: glyphBase64) {
            return [UInt8](data)
        }
        return []
    }()

    private static func glyphBytes(for ch: Character, table: [UInt8]) -> [UInt8] {
        guard let scalar = ch.unicodeScalars.first, scalar.isASCII else {
            return [0, 0, 0x1E, 0x12, 0x12, 0x12, 0x12, 0x12, 0x1E, 0, 0]
        }
        let val = Int(scalar.value)
        guard val >= 32 && val <= 126 else {
            return [0, 0, 0x1E, 0x12, 0x12, 0x12, 0x12, 0x12, 0x1E, 0, 0]
        }
        let offset = (val - 32) * 11
        guard offset + 11 <= table.count else {
            return [0, 0, 0x1E, 0x12, 0x12, 0x12, 0x12, 0x12, 0x1E, 0, 0]
        }
        return Array(table[offset..<(offset + 11)])
    }
}

open class NSTextStorage {
    public let attributedString: NSAttributedString
    public private(set) var layoutManagers: [NSLayoutManager] = []

    public init(attributedString: NSAttributedString) {
        self.attributedString = attributedString
    }

    open func addLayoutManager(_ manager: NSLayoutManager) {
        layoutManagers.append(manager)
        manager.textStorage = self
    }

    open func setAttributes(_ attributes: [NSAttributedString.Key: Any]?, range: NSRange) {}
}

extension NSString {
    public func size(withAttributes attrs: [NSAttributedString.Key: Any]? = nil) -> CGSize {
        let font = (attrs?[.font] as? NSFont) ?? NSFont.systemFont(ofSize: 12)
        let w = CGFloat(length) * font.pointSize * 0.55
        let h = font.pointSize * 1.2
        return CGSize(width: w, height: h)
    }

    public func draw(at point: CGPoint, withAttributes attrs: [NSAttributedString.Key: Any]? = nil) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        let font = (attrs?[.font] as? NSFont) ?? NSFont.systemFont(ofSize: 12)
        let color = (attrs?[.foregroundColor] as? NSColor) ?? NSColor.black
        context.setFillColor(color.cgColor)
        // Delegate to NSLayoutManager glyph rasterizer
        let storage = NSTextStorage(attributedString: NSAttributedString(string: self as String, attributes: attrs ?? [:]))
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: CGSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude))
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        layout.drawGlyphs(forGlyphRange: NSRange(location: 0, length: length), at: point)
    }
}

extension String {
    public func size(withAttributes attrs: [NSAttributedString.Key: Any]? = nil) -> CGSize {
        (self as NSString).size(withAttributes: attrs)
    }

    public func draw(at point: CGPoint, withAttributes attrs: [NSAttributedString.Key: Any]? = nil) {
        (self as NSString).draw(at: point, withAttributes: attrs)
    }
}
