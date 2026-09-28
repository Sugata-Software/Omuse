// Swift Testing cross-import overlay for AppKit. The real overlay adds Attachable conformances for Apple types and is
// not built on Linux; an empty module satisfies the compiler when a test imports both Testing and the AppKit compat.
