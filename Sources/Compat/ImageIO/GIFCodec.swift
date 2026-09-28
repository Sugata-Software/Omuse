// GIFCodec.swift — portable single-frame GIF89a writer. Qt reads GIF but cannot write it, and ImageIO can, so this
// covers `CGImageDestination` for "com.compuserve.gif". Colours are quantised to a fixed 3-3-2 palette (transparent
// pixels use index 0 as the transparent colour) and LZW is emitted with a clear code every 254 symbols, which keeps
// every code 9 bits wide: valid GIF, no dictionary, no compression.

import Foundation
import CoreGraphics

final class PortableGIFEncoder: ImageCodecBackend {
    func identify(_ data: Data) -> ImageInfo? { nil }
    func decode(_ data: Data) -> CGImage? { nil }

    func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?) -> Data? {
        guard typeIdentifier == "com.compuserve.gif", image.width <= 65535, image.height <= 65535 else { return nil }
        let w = image.width, h = image.height
        let px = image.portableImage.bytes, stride = image.bytesPerRow
        let gray = image.isGrayPlane
        var out = Data("GIF89a".utf8)
        func le16(_ v: Int) { out.append(UInt8(v & 255)); out.append(UInt8(v >> 8 & 255)) }
        le16(w); le16(h)
        out.append(contentsOf: [0xF7, 0, 0])                       // global 256-entry palette, 8 bits
        for i in 0..<256 {                                           // 3-3-2 palette; index 0 doubles as transparent
            let r = (i >> 5) * 255 / 7
            let g = ((i >> 2) & 7) * 255 / 7
            let b = (i & 3) * 255 / 3
            out.append(UInt8(r)); out.append(UInt8(g)); out.append(UInt8(b))
        }
        out.append(contentsOf: [0x21, 0xF9, 4, 1, 0, 0, 0, 0])      // graphic control: transparent index 0
        out.append(0x2C); le16(0); le16(0); le16(w); le16(h); out.append(0)
        out.append(8)                                                // LZW minimum code size

        func index(_ r: Int, _ g: Int, _ b: Int) -> UInt8 {
            let red: Int = (r >> 5) * 32
            let green: Int = (g >> 5) * 4
            let blue: Int = b >> 6
            return UInt8(red + green + blue)
        }
        var indices = [UInt8](); indices.reserveCapacity(w * h)
        for y in 0..<h {
            for x in 0..<w {
                if gray { let v = Int(px[y * stride + x]); indices.append(index(v, v, v)); continue }
                let o = y * stride + x * 4, a = Int(px[o + 3])
                if a < 128 { indices.append(0); continue }
                func straight(_ c: Int) -> Int { min(255, (Int(px[o + c]) * 255 + a / 2) / a) }
                indices.append(index(straight(0), straight(1), straight(2)))
            }
        }
        // 9-bit codes: 256 = clear, 257 = end. A clear every 254 symbols stops the code width from growing.
        var bits = 0, bitCount = 0
        var block = [UInt8]()
        func emit(_ code: Int) {
            bits |= code << bitCount; bitCount += 9
            while bitCount >= 8 { block.append(UInt8(bits & 255)); bits >>= 8; bitCount -= 8 }
        }
        var i = 0
        emit(256)
        while i < indices.count {
            let n = min(254, indices.count - i)
            for k in 0..<n { emit(Int(indices[i + k])) }
            i += n
            if i < indices.count { emit(256) }
        }
        emit(257)
        if bitCount > 0 { block.append(UInt8(bits & 255)) }
        var offset = 0
        while offset < block.count {
            let n = min(255, block.count - offset)
            out.append(UInt8(n)); out.append(contentsOf: block[offset..<(offset + n)]); offset += n
        }
        out.append(0); out.append(0x3B)
        return out
    }
}
