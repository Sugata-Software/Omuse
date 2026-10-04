# Omuse macro compatibility

This is an independently authored implementation of the four macro interfaces
used by Omuse's locked `html5ever 0.27.0`, `xml5ever 0.18.1`, `tendril 0.4.3` and
`futf 0.1.5` dependencies. It replaces the registry `mac 0.1.1` package through
Cargo's patch mechanism. No source from `reem/rust-mac` is included or relicensed.

The implemented contract comes from those consumers' call sites:

- `unwrap_or_return!(option, fallback)` extracts `Some` or returns from the
  caller. It evaluates the option once and the fallback only on `None`.
- `format_if!(condition, fallback, format, arguments...)` returns a borrowed
  fallback or owned formatted diagnostic. The unselected branch is not evaluated.
- `_tt_as_expr_hack!(expression)` forwards an expression from another macro.
- `test_eq!(name, actual, expected)` declares the HTML helper equality tests.

This is deliberately not advertised as a general replacement for every macro in
the original crate. Adding or changing parser dependencies requires rechecking
their macro calls. The local package version identifies this compatibility
implementation separately from the registry source. `tests/contract.rs` covers
the used call forms, ownership, lazy evaluation and generated tests; the
dependency acceptance harness additionally runs real HTML, XML and UTF-8 paths.

New code is distributed under Omuse's MIT license in `LICENSE`.
