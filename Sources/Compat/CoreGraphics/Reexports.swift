// On Apple platforms `import CoreGraphics` brings CGFloat/CGPoint/CGRect/CGSize (and Foundation's value
// types) into scope. On Linux those live in Foundation, so the compat module re-exports it.
@_exported import Foundation
