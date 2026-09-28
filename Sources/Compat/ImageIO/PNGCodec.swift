// PNGCodec.swift — portable PNG reader/writer (the always-available floor under ImageIO).
// Decodes colour types 0,2,3,4,6 at 8 or 16 bits (non-interlaced), reads pHYs for DPI; encodes 8-bit RGBA or gray
// with stored (uncompressed) deflate blocks. Photographic formats need the host's codecs.

import Foundation
import CoreGraphics

final class PortablePNGCodec: ImageCodecBackend {
    private static let signature: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]

    // MARK: Chunk parsing

    private struct Header { var width = 0, height = 0, depth = 8, colorType = 6, interlace = 0 }
    private struct Parsed { var header = Header(); var palette: [UInt8] = []; var transparency: [UInt8] = []
                            var idat = Data(); var dpi: Double? }

    private func parse(_ data: Data) -> Parsed? {
        let b = [UInt8](data)
        guard b.count > 33, Array(b[0..<8]) == Self.signature else { return nil }
        var p = Parsed(); var i = 8; var sawHeader = false
        func be32(_ o: Int) -> Int { Int(b[o]) << 24 | Int(b[o + 1]) << 16 | Int(b[o + 2]) << 8 | Int(b[o + 3]) }
        while i + 8 <= b.count {
            let length = be32(i); let type = String(bytes: b[(i + 4)..<(i + 8)], encoding: .ascii) ?? ""
            let start = i + 8, end = start + length
            guard end + 4 <= b.count else { return nil }
            let body = b[start..<end]
            switch type {
            case "IHDR":
                guard length == 13 else { return nil }
                p.header = Header(width: be32(start), height: be32(start + 4), depth: Int(b[start + 8]),
                                  colorType: Int(b[start + 9]), interlace: Int(b[start + 12]))
                sawHeader = true
            case "PLTE": p.palette = Array(body)
            case "tRNS": p.transparency = Array(body)
            case "IDAT": p.idat.append(contentsOf: body)
            case "pHYs":
                if length == 9, b[start + 8] == 1 { p.dpi = Double(be32(start)) * 0.0254 }
            case "IEND": return sawHeader ? p : nil
            default: break
            }
            i = end + 4
        }
        return sawHeader ? p : nil
    }

    private static func channels(_ colorType: Int) -> Int? {
        switch colorType { case 0: 1; case 2: 3; case 3: 1; case 4: 2; case 6: 4; default: nil }
    }

    func identify(_ data: Data) -> ImageInfo? {
        guard let p = parse(data), p.header.width > 0, p.header.height > 0 else { return nil }
        return ImageInfo(typeIdentifier: "public.png", width: p.header.width, height: p.header.height, bitDepth: p.header.depth,
                         dpi: p.dpi, orientation: 1, hasAlpha: [4, 6].contains(p.header.colorType) || !p.transparency.isEmpty)
    }

    // MARK: Decode

    func decode(_ data: Data) -> CGImage? {
        guard let p = parse(data), p.header.interlace == 0, let ch = Self.channels(p.header.colorType),
              [8, 16].contains(p.header.depth) || (p.header.colorType == 3 && [1, 2, 4, 8].contains(p.header.depth)) || (p.header.colorType == 0 && [1, 2, 4].contains(p.header.depth)),
              p.header.width > 0, p.header.height > 0, p.header.width * p.header.height <= 100_000_000,
              let raw = Inflate.zlib([UInt8](p.idat)) else { return nil }
        let w = p.header.width, h = p.header.height, depth = p.header.depth
        let bitsPerPixel = ch * depth
        let bpp = max(1, bitsPerPixel / 8)
        let stride = (w * bitsPerPixel + 7) / 8
        guard raw.count >= h * (stride + 1) else { return nil }
        var rows = [UInt8](repeating: 0, count: h * stride)
        for y in 0..<h {
            let filter = raw[y * (stride + 1)]
            let src = y * (stride + 1) + 1, dst = y * stride
            for x in 0..<stride {
                let a = x >= bpp ? Int(rows[dst + x - bpp]) : 0
                let up = y > 0 ? Int(rows[dst - stride + x]) : 0
                let c = (x >= bpp && y > 0) ? Int(rows[dst - stride + x - bpp]) : 0
                var v = Int(raw[src + x])
                switch filter {
                case 1: v += a
                case 2: v += up
                case 3: v += (a + up) / 2
                case 4:
                    let pa = abs(up - c), pb = abs(a - c), pc = abs(a + up - 2 * c)
                    v += (pa <= pb && pa <= pc) ? a : (pb <= pc ? up : c)
                case 0: break
                default: return nil
                }
                rows[dst + x] = UInt8(truncatingIfNeeded: v)
            }
        }
        // Sample reader that expands any depth to 8 bits.
        func sample(_ row: Int, _ index: Int) -> Int {
            let base = row * stride
            switch depth {
            case 8: return Int(rows[base + index])
            case 16: return Int(rows[base + index * 2])
            default:
                let bit = index * depth
                let byte = Int(rows[base + bit / 8])
                let shift = 8 - depth - (bit % 8)
                return (byte >> shift) & ((1 << depth) - 1)
            }
        }
        let maxValue = depth == 16 ? 255 : (1 << depth) - 1

        // A plain gray image without transparency stays a one-channel plane (the mask representation).
        if p.header.colorType == 0 && p.transparency.isEmpty {
            var plane = [UInt8](repeating: 0, count: w * h)
            for y in 0..<h { for x in 0..<w { plane[y * w + x] = UInt8(sample(y, x) * 255 / maxValue) } }
            return CGImage(PortableImage(width: w, height: h, kind: .mask, bytesPerRow: w, bytes: plane))
        }
        var out = [UInt8](repeating: 0, count: w * h * 4)
        for y in 0..<h {
            for x in 0..<w {
                var r = 0, g = 0, b = 0, a = 255
                switch p.header.colorType {
                case 0: r = sample(y, x) * 255 / maxValue; g = r; b = r
                    if p.transparency.count >= 2 { let key = Int(p.transparency[0]) << 8 | Int(p.transparency[1]); if sample(y, x) == (depth == 16 ? key >> 8 : key) { a = 0 } }
                case 2: r = sample(y, x * 3); g = sample(y, x * 3 + 1); b = sample(y, x * 3 + 2)
                case 3:
                    let idx = sample(y, x)
                    guard idx * 3 + 2 < p.palette.count else { return nil }
                    r = Int(p.palette[idx * 3]); g = Int(p.palette[idx * 3 + 1]); b = Int(p.palette[idx * 3 + 2])
                    if idx < p.transparency.count { a = Int(p.transparency[idx]) }
                case 4: r = sample(y, x * 2); g = r; b = r; a = sample(y, x * 2 + 1)
                default: r = sample(y, x * 4); g = sample(y, x * 4 + 1); b = sample(y, x * 4 + 2); a = sample(y, x * 4 + 3)
                }
                let o = (y * w + x) * 4
                // Premultiply (rounded) for the compat's canonical storage.
                out[o] = UInt8((r * a + 127) / 255); out[o + 1] = UInt8((g * a + 127) / 255)
                out[o + 2] = UInt8((b * a + 127) / 255); out[o + 3] = UInt8(a)
            }
        }
        return CGImage(PortableImage(width: w, height: h, kind: .rgba, bytesPerRow: w * 4, bytes: out))
    }

    // MARK: Encode

    func encode(_ image: CGImage, typeIdentifier: String, quality: Double?, dpi: Double?) -> Data? {
        guard typeIdentifier == "public.png" else { return nil }
        let w = image.width, h = image.height
        let mask = image.isGrayPlane
        let ch = mask ? 1 : 4
        var filtered = [UInt8](); filtered.reserveCapacity(h * (w * ch + 1))
        let px = image.portableImage.bytes, rowBytes = image.bytesPerRow
        for y in 0..<h {
            filtered.append(0)
            for x in 0..<w {
                if mask { filtered.append(px[y * rowBytes + x]) }
                else {
                    let o = y * rowBytes + x * 4
                    let a = Int(px[o + 3])
                    // Un-premultiply for PNG's straight alpha.
                    for c in 0..<3 { filtered.append(a == 0 ? 0 : UInt8(min(255, (Int(px[o + c]) * 255 + a / 2) / a))) }
                    filtered.append(px[o + 3])
                }
            }
        }
        var out = Data(Self.signature)
        func chunk(_ type: String, _ body: [UInt8]) {
            var len = UInt32(body.count).bigEndian
            out.append(Data(bytes: &len, count: 4))
            let typeBytes = Array(type.utf8)
            out.append(contentsOf: typeBytes); out.append(contentsOf: body)
            var crc = CRC32.checksum(typeBytes + body).bigEndian
            out.append(Data(bytes: &crc, count: 4))
        }
        func be(_ v: Int) -> [UInt8] { [UInt8(v >> 24 & 255), UInt8(v >> 16 & 255), UInt8(v >> 8 & 255), UInt8(v & 255)] }
        chunk("IHDR", be(w) + be(h) + [8, mask ? 0 : 6, 0, 0, 0])
        if let dpi, dpi > 0 { let ppm = Int((dpi / 0.0254).rounded()); chunk("pHYs", be(ppm) + be(ppm) + [1]) }
        chunk("IDAT", Inflate.zlibStored(filtered))
        chunk("IEND", [])
        return out
    }
}

enum CRC32 {
    private static let table: [UInt32] = (0..<256).map { n in
        var c = UInt32(n)
        for _ in 0..<8 { c = c & 1 == 1 ? 0xEDB88320 ^ (c >> 1) : c >> 1 }
        return c
    }
    static func checksum(_ bytes: [UInt8]) -> UInt32 {
        var c: UInt32 = 0xFFFFFFFF
        for b in bytes { c = table[Int((c ^ UInt32(b)) & 0xFF)] ^ (c >> 8) }
        return c ^ 0xFFFFFFFF
    }
}

/// RFC 1950/1951: a complete inflater (stored, fixed and dynamic Huffman blocks) and a stored-block writer.
enum Inflate {
    static func zlibStored(_ data: [UInt8]) -> [UInt8] {
        var out: [UInt8] = [0x78, 0x01]
        var i = 0
        repeat {
            let n = min(65535, data.count - i)
            let final: UInt8 = i + n >= data.count ? 1 : 0
            out.append(final)
            out.append(UInt8(n & 255)); out.append(UInt8(n >> 8)); out.append(UInt8(~n & 255)); out.append(UInt8((~n >> 8) & 255))
            out.append(contentsOf: data[i..<(i + n)])
            i += n
        } while i < data.count
        var a: UInt32 = 1, b: UInt32 = 0
        for v in data { a = (a + UInt32(v)) % 65521; b = (b + a) % 65521 }
        let adler = b << 16 | a
        out.append(contentsOf: [UInt8(adler >> 24), UInt8(adler >> 16 & 255), UInt8(adler >> 8 & 255), UInt8(adler & 255)])
        return out
    }

    static func zlib(_ input: [UInt8]) -> [UInt8]? {
        guard input.count > 6, input[0] & 0x0F == 8, (Int(input[0]) << 8 | Int(input[1])) % 31 == 0 else { return nil }
        return inflate(Array(input[2...]))
    }

    private struct Huffman { var count = [Int](repeating: 0, count: 16); var symbol: [Int] }
    private static func build(_ lengths: [Int]) -> Huffman {
        var h = Huffman(symbol: [Int](repeating: 0, count: lengths.count))
        for l in lengths { h.count[l] += 1 }
        var offs = [Int](repeating: 0, count: 16)
        for i in 1..<15 { offs[i + 1] = offs[i] + h.count[i] }
        for (s, l) in lengths.enumerated() where l != 0 { h.symbol[offs[l]] = s; offs[l] += 1 }
        return h
    }

    static func inflate(_ input: [UInt8]) -> [UInt8]? {
        var pos = 0, bitBuf = 0, bitCnt = 0
        var out = [UInt8]()
        func bits(_ n: Int) -> Int? {
            var val = bitBuf
            while bitCnt < n {
                guard pos < input.count else { return nil }
                val |= Int(input[pos]) << bitCnt; pos += 1; bitCnt += 8
            }
            bitBuf = val >> n; bitCnt -= n
            return val & ((1 << n) - 1)
        }
        func decode(_ h: Huffman) -> Int? {
            var code = 0, first = 0, index = 0
            for len in 1...15 {
                guard let b = bits(1) else { return nil }
                code |= b
                let count = h.count[len]
                if code - count < first { return h.symbol[index + (code - first)] }
                index += count; first += count; first <<= 1; code <<= 1
            }
            return nil
        }
        let lbase = [3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258]
        let lext = [0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0]
        let dbase = [1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577]
        let dext = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13]
        func codes(_ lc: Huffman, _ dc: Huffman) -> Bool {
            while true {
                guard let sym = decode(lc) else { return false }
                if sym < 256 { out.append(UInt8(sym)) }
                else if sym == 256 { return true }
                else {
                    let s = sym - 257
                    guard s < 29, let e = bits(lext[s]), let ds = decode(dc), ds < 30, let de = bits(dext[ds]) else { return false }
                    let len = lbase[s] + e, dist = dbase[ds] + de
                    guard dist <= out.count else { return false }
                    for _ in 0..<len { out.append(out[out.count - dist]) }
                }
            }
        }
        var last = 0
        repeat {
            guard let l = bits(1), let type = bits(2) else { return nil }
            last = l
            switch type {
            case 0:
                bitBuf = 0; bitCnt = 0
                guard pos + 4 <= input.count else { return nil }
                let len = Int(input[pos]) | Int(input[pos + 1]) << 8
                pos += 4
                guard pos + len <= input.count else { return nil }
                out.append(contentsOf: input[pos..<(pos + len)]); pos += len
            case 1:
                var l = [Int](repeating: 8, count: 288)
                for i in 144..<256 { l[i] = 9 }; for i in 256..<280 { l[i] = 7 }
                guard codes(build(l), build([Int](repeating: 5, count: 30))) else { return nil }
            case 2:
                guard let nlen = bits(5), let ndist = bits(5), let ncode = bits(4) else { return nil }
                let order = [16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15]
                var cl = [Int](repeating: 0, count: 19)
                for i in 0..<(ncode + 4) { guard let v = bits(3) else { return nil }; cl[order[i]] = v }
                let clh = build(cl)
                var lengths = [Int](); let total = nlen + 257 + ndist + 1
                while lengths.count < total {
                    guard let sym = decode(clh) else { return nil }
                    if sym < 16 { lengths.append(sym) }
                    else {
                        var prev = 0, rep = 0
                        if sym == 16 { guard let last = lengths.last, let r = bits(2) else { return nil }; prev = last; rep = 3 + r }
                        else if sym == 17 { guard let r = bits(3) else { return nil }; rep = 3 + r }
                        else { guard let r = bits(7) else { return nil }; rep = 11 + r }
                        guard lengths.count + rep <= total else { return nil }
                        lengths.append(contentsOf: [Int](repeating: prev, count: rep))
                    }
                }
                guard codes(build(Array(lengths[0..<(nlen + 257)])), build(Array(lengths[(nlen + 257)...]))) else { return nil }
            default: return nil
            }
        } while last == 0
        return out
    }
}
