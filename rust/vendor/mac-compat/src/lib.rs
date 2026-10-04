//! Small, independently implemented adapters for the macro call forms used by
//! Omuse's locked HTML and UTF-8 dependencies. See README.md for scope and origin.

#![forbid(unsafe_code)]

/// Extract an optional value, returning the supplied fallback from the caller
/// when the value is absent. Both expressions are evaluated only when needed.
#[macro_export]
macro_rules! unwrap_or_return {
    ($optional:expr, $fallback:expr $(,)?) => {
        match $optional {
            ::std::option::Option::Some(value) => value,
            ::std::option::Option::None => return $fallback,
        }
    };
}

/// Produce a detailed, allocated diagnostic only when requested. The common
/// fallback remains borrowed and does not evaluate the formatting arguments.
#[macro_export]
macro_rules! format_if {
    ($detailed:expr, $fallback:expr, $($format:tt)+) => {
        if $detailed {
            ::std::borrow::Cow::Owned(::std::format!($($format)+))
        } else {
            ::std::borrow::Cow::Borrowed($fallback)
        }
    };
}

/// Preserve an expression passed through another macro's token-tree argument.
#[macro_export]
macro_rules! _tt_as_expr_hack {
    ($expression:expr) => {
        $expression
    };
}

/// Declare the equality tests used by html5ever's ASCII helper test module.
#[macro_export]
macro_rules! test_eq {
    ($name:ident, $actual:expr, $expected:expr $(,)?) => {
        #[test]
        fn $name() {
            ::std::assert_eq!($actual, $expected);
        }
    };
}
