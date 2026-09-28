import Foundation
import CoreGraphics

/// Core Animation layer placeholder: views may set layer properties; nothing composites them here.
public final class CALayer {
    public var masksToBounds = false, cornerRadius: CGFloat = 0, opacity: Float = 1, isHidden = false
    public var anchorPoint = CGPoint(x: 0.5, y: 0.5), position = CGPoint.zero, bounds = CGRect.zero
    private var transform = CGAffineTransform.identity
    public func affineTransform() -> CGAffineTransform { transform }
    public func setAffineTransform(_ t: CGAffineTransform) { transform = t }
    public var backgroundColor: CGColor?
    public var borderWidth: CGFloat = 0, borderColor: CGColor?
    public init() {}
}

@MainActor public protocol NSTextViewDelegate: AnyObject {
    func textDidChange(_ notification: Notification)
    func textView(_ textView: NSTextView, shouldChangeTextIn affectedCharRange: NSRange, replacementString: String?) -> Bool
}
extension NSTextViewDelegate {
    public func textDidChange(_ notification: Notification) {}
    public func textView(_ textView: NSTextView, shouldChangeTextIn affectedCharRange: NSRange, replacementString: String?) -> Bool { true }
}

@MainActor open class NSText: NSView {
    open var string = ""
    open var font: NSFont?
    open var textColor: NSColor?
    open var alignment: NSTextAlignment = .natural
    open var isEditable = true, isSelectable = true, isRichText = true
    open var isFieldEditor = false
    var selection = NSRange(location: 0, length: 0)
    open func selectedRange() -> NSRange { selection }
    open func setSelectedRange(_ range: NSRange) { selection = range }
    open func selectAll(_ sender: Any?) { selection = NSRange(location: 0, length: (string as NSString).length) }
    open func paste(_ sender: Any?) {}
    open func copy(_ sender: Any?) {}
    open func cut(_ sender: Any?) {}
}

@MainActor open class NSTextView: NSText {
    public weak var delegate: NSTextViewDelegate?
    public var textStorage: NSTextStorage? = NSTextStorage(attributedString: NSAttributedString(string: ""))
    public var layoutManager: NSLayoutManager? = NSLayoutManager()
    public var textContainer: NSTextContainer? = NSTextContainer(size: .zero)
    public var textContainerInset = CGSize.zero
    public var drawsBackground = true
    public var backgroundColor: NSColor = .white
    public var insertionPointColor: NSColor = .black
    public var importsGraphics = true, allowsUndo = false
    public var isVerticallyResizable = false, isHorizontallyResizable = false
    public var isAutomaticQuoteSubstitutionEnabled = true, isAutomaticDashSubstitutionEnabled = true
    public var isAutomaticSpellingCorrectionEnabled = true, isAutomaticTextReplacementEnabled = true
    public var typingAttributes: [NSAttributedString.Key: Any] = [:]
    open func hasMarkedText() -> Bool { false }
    open func pasteAsPlainText(_ sender: Any?) {}
    open func insertText(_ string: Any, replacementRange: NSRange) {
        guard let text = string as? String else { return }
        let ns = self.string as NSString
        let range = replacementRange.location == NSNotFound ? selection : replacementRange
        self.string = ns.replacingCharacters(in: range, with: text)
        selection = NSRange(location: range.location + (text as NSString).length, length: 0)
        delegate?.textDidChange(Notification(name: Notification.Name("NSTextDidChangeNotification")))
    }
    open func scrollRangeToVisible(_ range: NSRange) {}
}
