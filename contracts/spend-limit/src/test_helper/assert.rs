#[macro_export]
macro_rules! assert_substring {
    ($haystack:expr, $needle:expr) => {
        let Some(start) = $haystack.rfind($needle.as_str()) else {
            panic!(
                "Expected string:\n    {}\nnot found in:\n    `{}`",
                $needle, $haystack
            );
        };

        assert_eq!($haystack[start..start + $needle.len()], $needle);
    };
}
