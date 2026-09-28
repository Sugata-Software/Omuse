import Foundation
import UniformTypeIdentifiers

/// Drag-and-drop / paste providers carry loadable representations; Qt hands file URLs and images instead, so this is
/// a plain container of already-resolved data.
open class NSItemProvider {
    /// Representations by type identifier, already resolved (Qt hands over data, not lazy providers).
    public var representations: [String: Data]
    public init(representations: [String: Data] = [:]) { self.representations = representations }
    /// `NSItemProvider(item:typeIdentifier:)`: a URL becomes its absolute string, `Data` is kept as is.
    public convenience init(item: NSSecureCoding?, typeIdentifier: String?) {
        var reps: [String: Data] = [:]
        // `item` is typed as the `NSSecureCoding` existential; only the `NS*` reference types actually conform to it
        // (the `URL`/`Data` value types don't), so those are the only casts that can ever match.
        if let typeIdentifier {
            if let nsurl = item as? NSURL { reps[typeIdentifier] = Data(nsurl.absoluteString.utf8) }
            else if let nsdata = item as? NSData { reps[typeIdentifier] = nsdata as Data }
        }
        self.init(representations: reps)
    }
    public var registeredTypeIdentifiers: [String] { Array(representations.keys) }

    public func hasItemConformingToTypeIdentifier(_ identifier: String) -> Bool {
        guard let wanted = UTType(identifier) else { return representations[identifier] != nil }
        return representations.keys.contains { UTType($0)?.conforms(to: wanted) == true || $0 == identifier }
    }

    /// `public.file-url` resolves to a `URL`; other types hand back their data.
    public func loadItem(forTypeIdentifier identifier: String, options: [AnyHashable: Any]? = nil,
                         completionHandler: (@Sendable (NSSecureCoding?, Error?) -> Void)? = nil) {
        guard let data = representations[identifier] else { completionHandler?(nil, CocoaError(.fileReadNoSuchFile)); return }
        if identifier == UTType.fileURL.identifier, let text = String(data: data, encoding: .utf8), let url = URL(string: text) {
            completionHandler?(url as NSURL, nil)
        } else {
            completionHandler?(data as NSData, nil)
        }
    }

    /// Writes the representation to a temporary file that exists only for the duration of the handler.
    @discardableResult
    public func loadFileRepresentation(forTypeIdentifier identifier: String,
                                       completionHandler: @escaping @Sendable (URL?, Error?) -> Void) -> AnyObject? {
        let wanted = UTType(identifier)
        guard let match = representations.first(where: { key, _ in key == identifier || (wanted != nil && UTType(key)?.conforms(to: wanted!) == true) }) else {
            completionHandler(nil, CocoaError(.fileReadNoSuchFile)); return nil
        }
        let ext = UTType(match.key)?.preferredFilenameExtension ?? "dat"
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("provider-\(UUID().uuidString)").appendingPathExtension(ext)
        do { try match.value.write(to: url); completionHandler(url, nil); try? FileManager.default.removeItem(at: url) }
        catch { completionHandler(nil, error) }
        return nil
    }

    @discardableResult
    public func loadDataRepresentation(forTypeIdentifier identifier: String,
                                       completionHandler: @escaping @Sendable (Data?, Error?) -> Void) -> AnyObject? {
        let wanted = UTType(identifier)
        let match = representations.first { key, _ in key == identifier || (wanted != nil && UTType(key)?.conforms(to: wanted!) == true) }
        completionHandler(match?.value, match == nil ? CocoaError(.fileReadNoSuchFile) : nil)
        return nil
    }
}
