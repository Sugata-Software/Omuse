// CoreFoundationTypes.swift — the toll-free-bridged CoreFoundation names upstream code writes explicitly
// (`colors as CFArray`, `[kCGImageSourceShouldCache: true] as CFDictionary`). On Linux they resolve to the
// Swift value types they bridge to on Apple platforms, so the same `as` casts compile unchanged.

import Foundation

public typealias CFArray = [Any]
public typealias CFDictionary = [AnyHashable: Any]
public typealias CFString = String
public typealias CFURL = URL
public typealias CFData = Data
public typealias CFMutableData = NSMutableData
public typealias CFTypeRef = AnyObject
