// Fixed-capacity formatting helpers built on `arrayvec::ArrayString`.
// Avoids heap allocations.
use arrayvec::ArrayString;

// "snake_case" string to "Title Case" string. E.g.: "hello_world" => "Hello World".
// Truncates the resulting string if capacity is not big enough to hold `s`.
pub fn snake_case_to_title<const CAP: usize>(s: &str) -> ArrayString<CAP> {
    let mut result = ArrayString::<CAP>::new();

    for (i, word) in s.split('_').enumerate() {
        if i > 0 && result.try_push(' ').is_err() {
            break;
        }

        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for c in first.to_uppercase() {
                if result.try_push(c).is_err() {
                    return result;
                }
            }

            for c in chars {
                if result.try_push(c).is_err() {
                    return result;
                }
            }
        }
    }

    result
}

// Fixed string writer that truncates instead of erroring.
pub struct FixedWriter<'a, const CAP: usize> {
    pub buf: &'a mut ArrayString<CAP>,
}

impl<'a, const CAP: usize> FixedWriter<'a, CAP> {
    pub fn new(buf: &'a mut ArrayString<CAP>) -> Self {
        Self { buf }
    }
}

impl<'a, const CAP: usize> core::fmt::Write for FixedWriter<'a, CAP> {
    // Pushes str into self, truncating if capacity is exceeded.
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let remaining = self.buf.capacity() - self.buf.len();
        if s.len() <= remaining {
            self.buf.push_str(s);
        } else {
            self.buf.push_str(&s[..remaining]);
        }
        Ok(())
    }
}

// Creates a new `ArrayString<CAP>` and formats into it.
// Panics if capacity is exceeded.
#[macro_export]
macro_rules! format_fixed_string {
    ($capacity:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        let mut buf = arrayvec::ArrayString::<$capacity>::new();
        match write!(&mut buf, $($arg)*) {
            Ok(()) => buf,
            Err(_) => panic!("format_fixed_string! capacity {} exceeded", $capacity),
        }
    }};
}

// Creates a new `ArrayString<CAP>` and formats into it.
// Silently truncates if capacity is exceeded.
#[macro_export]
macro_rules! format_fixed_string_trunc {
    ($capacity:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        let mut buf = arrayvec::ArrayString::<$capacity>::new();
        let mut writer = $crate::fixed_string::FixedWriter::new(&mut buf);
        let _ = write!(&mut writer, $($arg)*);
        buf
    }};
}

// Clears an existing `ArrayString` and writes formatted data into it.
// Panics if capacity is exceeded.
// Intended for per-frame buffer reuse.
#[macro_export]
macro_rules! write_fixed_string {
    ($buf:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        $buf.clear();
        match write!($buf, $($arg)*) {
            Ok(()) => {},
            Err(_) => panic!("write_fixed_string! capacity {} exceeded", $buf.capacity()),
        }
    }};
}

// Clears an existing `ArrayString` and writes formatted data into it.
// Silently truncates on overflow.
// Intended for per-frame buffer reuse.
#[macro_export]
macro_rules! write_fixed_string_trunc {
    ($buf:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        $buf.clear();
        let mut writer = $crate::fixed_string::FixedWriter::new($buf);
        let _ = write!(&mut writer, $($arg)*);
    }};
}

// Append formatted data to an existing `ArrayString`.
// Panics if capacity is exceeded.
#[macro_export]
macro_rules! append_fixed_string {
    ($buf:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        match write!($buf, $($arg)*) {
            Ok(()) => {},
            Err(_) => panic!("append_fixed_string! capacity {} exceeded", $buf.capacity()),
        }
    }};
}

// Append formatted data to an existing `ArrayString`.
// Silently truncates on overflow.
#[macro_export]
macro_rules! append_fixed_string_trunc {
    ($buf:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        let mut writer = $crate::fixed_string::FixedWriter::new($buf);
        let _ = write!(&mut writer, $($arg)*);
    }};
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use arrayvec::ArrayString;

    use super::*;

    #[test]
    fn test_snake_case_to_title() {
        let s1 = snake_case_to_title::<64>("snake_case_string");
        assert_eq!(&*s1, "Snake Case String");

        // Already in title case, no change.
        let s2 = snake_case_to_title::<64>("Snake Case String");
        assert_eq!(&*s2, "Snake Case String");

        // Truncated.
        let s3 = snake_case_to_title::<20>("a_very_long_string_that_will_be_truncated");
        assert_eq!(&*s3, "A Very Long String T");
    }

    #[test]
    fn test_format_fixed_string_ok() {
        let s = format_fixed_string!(16, "Population: {}", 42);
        assert_eq!(&*s, "Population: 42");
    }

    #[test]
    fn test_format_fixed_string_trunc() {
        let s = format_fixed_string_trunc!(8, "Hello {}", 12345);
        // Should truncate safely.
        assert_eq!(s.len(), 8);
        assert_eq!(&*s, "Hello 12");
    }

    #[test]
    fn test_write_fixed_string_reuse() {
        let mut buf = ArrayString::<16>::new();

        write_fixed_string_trunc!(&mut buf, "{}", 10);
        assert_eq!(&*buf, "10");

        write_fixed_string_trunc!(&mut buf, "Value: {}", 5);
        assert_eq!(&*buf, "Value: 5");

        write_fixed_string_trunc!(&mut buf, "A very long string that will be truncated");
        assert_eq!(buf.len(), 16);
        assert_eq!(&*buf, "A very long stri");
    }

    #[test]
    #[should_panic(expected = "format_fixed_string! capacity 4 exceeded")]
    fn test_format_fixed_string_overflow_panics() {
        // Capacity too small.
        let _ = format_fixed_string!(4, "Hello {}", 12345);
    }

    #[test]
    #[should_panic(expected = "write_fixed_string! capacity 4 exceeded")]
    fn test_write_fixed_string_overflow_panics() {
        // Capacity too small.
        let mut buf = ArrayString::<4>::new();
        write_fixed_string!(&mut buf, "{}", 12345);
    }
}
