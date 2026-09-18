/// Constructs a [`Value`](crate::Value) using JSON-like syntax.
///
/// The macro supports `null`, booleans, all Rust integer primitives, `f32`,
/// `f64`, UTF-8 strings, byte strings, nested arrays, and maps with string
/// literal keys. A value position may also contain an expression yielding a
/// supported Rust value or an existing [`Value`](crate::Value).
///
/// Every invocation returns [`Result<Value>`](crate::Result). In particular,
/// integers outside the range supported by [`Value::int`](crate::Value::int)
/// produce [`ErrorKind::InvalidData`](crate::ErrorKind::InvalidData) instead of
/// panicking.
///
/// Expressions are evaluated exactly once. Passing an owned expression moves
/// it; pass a reference or clone it when the original value must be retained.
/// Container syntax takes precedence over Rust block expressions, so an
/// ambiguous block expression should be wrapped in parentheses.
///
/// # Examples
///
/// ```
/// # fn main() -> doxsync::Result<()> {
/// use doxsync::value;
///
/// let enabled = true;
/// let existing = value!("cached")?;
/// let value = value!({
///     "name": "doxsync",
///     "enabled": enabled,
///     "retries": 3,
///     "ratio": 0.5,
///     "payload": b"hello",
///     "items": [1, null, false],
///     "existing": &existing,
/// })?;
///
/// let map = value.as_map().unwrap();
/// assert_eq!(map.get(&"name".to_owned()).unwrap().as_tstr(), Some("doxsync"));
/// assert_eq!(
///     map.get(&"payload".to_owned()).unwrap().as_bstr(),
///     Some(&b"hello"[..]),
/// );
/// # Ok(())
/// # }
/// ```
///
/// Map keys must be string literals in this first version. Trailing commas are
/// accepted in both arrays and maps.
#[macro_export]
macro_rules! value {
    ($($value:tt)+) => {
        $crate::__doxsync_value_internal!($($value)+)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __doxsync_value_literal {
    (-$literal:literal) => {{
        use $crate::__private::NegativeLiteralIntoValue as _;
        (&(-$literal)).negative_literal_into_value()
    }};
    ($literal:literal) => {{
        use $crate::__private::PositiveLiteralIntoValue as _;
        (&$literal).positive_literal_into_value()
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __doxsync_value_internal {
    // Finish an array. A labelled block preserves the caller's `await` and `?`
    // expression context while still returning conversion failures as the
    // macro's Result.
    (@array [$($elements:expr,)*]) => {{
        '__doxsync_value: {
            let mut values = $crate::__private::Vec::new();
            $(
                let value = match $elements {
                    $crate::__private::Ok(value) => value,
                    $crate::__private::Err(error) => {
                        break '__doxsync_value $crate::__private::Err(error);
                    }
                };
                values.push(value);
            )*
            break '__doxsync_value $crate::Value::array(values);
        }
    }};

    // Parse null and nested containers before the expression fallback.
    (@array [$($elements:expr,)*] null, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_internal!(null),] $($rest)*
        )
    };
    (@array [$($elements:expr,)*] null) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_internal!(null),]
        )
    };
    (@array [$($elements:expr,)*] [$($array:tt)*], $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_internal!([$($array)*]),] $($rest)*
        )
    };
    (@array [$($elements:expr,)*] [$($array:tt)*]) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_internal!([$($array)*]),]
        )
    };
    (@array [$($elements:expr,)*] {$($map:tt)*}, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_internal!({$($map)*}),] $($rest)*
        )
    };
    (@array [$($elements:expr,)*] {$($map:tt)*}) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_internal!({$($map)*}),]
        )
    };

    // Preserve literal tokens until conversion so unsuffixed integers receive
    // a type that covers Value's complete integer range.
    (@array [$($elements:expr,)*] -$literal:literal, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_literal!(-$literal),] $($rest)*
        )
    };
    (@array [$($elements:expr,)*] -$literal:literal) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_literal!(-$literal),]
        )
    };
    (@array [$($elements:expr,)*] $literal:literal, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_literal!($literal),] $($rest)*
        )
    };
    (@array [$($elements:expr,)*] $literal:literal) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_literal!($literal),]
        )
    };

    // Parse ordinary expressions.
    (@array [$($elements:expr,)*] $next:expr, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_internal!($next),] $($rest)*
        )
    };
    (@array [$($elements:expr,)*] $last:expr) => {
        $crate::__doxsync_value_internal!(
            @array [$($elements,)* $crate::__doxsync_value_internal!($last),]
        )
    };

    // Finish a map after parsing its entries.
    (@object [$($key:literal => $value:expr,)*]) => {{
        '__doxsync_value: {
            let mut object = $crate::__private::BTreeMap::new();
            $(
                let value = match $value {
                    $crate::__private::Ok(value) => value,
                    $crate::__private::Err(error) => {
                        break '__doxsync_value $crate::__private::Err(error);
                    }
                };
                let _ = object.insert(
                    $crate::__private::Arc::new($crate::__private::String::from($key)),
                    value,
                );
            )*
            break '__doxsync_value $crate::Value::map(object);
        }
    }};

    // Parse null and nested containers before the expression fallback.
    (@object [$($entries:tt)*] $key:literal : null, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_internal!(null),] $($rest)*
        )
    };
    (@object [$($entries:tt)*] $key:literal : null) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_internal!(null),]
        )
    };
    (@object [$($entries:tt)*] $key:literal : [$($array:tt)*], $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_internal!([$($array)*]),]
            $($rest)*
        )
    };
    (@object [$($entries:tt)*] $key:literal : [$($array:tt)*]) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_internal!([$($array)*]),]
        )
    };
    (@object [$($entries:tt)*] $key:literal : {$($map:tt)*}, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_internal!({$($map)*}),]
            $($rest)*
        )
    };
    (@object [$($entries:tt)*] $key:literal : {$($map:tt)*}) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_internal!({$($map)*}),]
        )
    };

    // Preserve literal tokens until conversion.
    (@object [$($entries:tt)*] $key:literal : -$literal:literal, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_literal!(-$literal),]
            $($rest)*
        )
    };
    (@object [$($entries:tt)*] $key:literal : -$literal:literal) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_literal!(-$literal),]
        )
    };
    (@object [$($entries:tt)*] $key:literal : $literal:literal, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_literal!($literal),]
            $($rest)*
        )
    };
    (@object [$($entries:tt)*] $key:literal : $literal:literal) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_literal!($literal),]
        )
    };

    // Parse ordinary expressions.
    (@object [$($entries:tt)*] $key:literal : $value:expr, $($rest:tt)*) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_internal!($value),]
            $($rest)*
        )
    };
    (@object [$($entries:tt)*] $key:literal : $value:expr) => {
        $crate::__doxsync_value_internal!(
            @object [$($entries)* $key => $crate::__doxsync_value_internal!($value),]
        )
    };

    // Main entry points. Container and literal forms must precede the
    // expression fallback.
    (null) => {
        $crate::Value::null()
    };
    ([]) => {
        $crate::Value::array($crate::__private::Vec::new())
    };
    ([$($tokens:tt)+]) => {
        $crate::__doxsync_value_internal!(@array [] $($tokens)+)
    };
    ({}) => {
        $crate::Value::map($crate::__private::BTreeMap::new())
    };
    ({$($tokens:tt)+}) => {
        $crate::__doxsync_value_internal!(@object [] $($tokens)+)
    };
    (-$literal:literal) => {
        $crate::__doxsync_value_literal!(-$literal)
    };
    ($literal:literal) => {
        $crate::__doxsync_value_literal!($literal)
    };
    ($other:expr) => {
        $crate::__private::into_value($other)
    };
}
