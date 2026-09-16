/// Render a byte count using binary (1024-based) units.
///
/// ```
/// assert_eq!(mac2wingo::core::format::human_size(0), "0 B");
/// assert_eq!(mac2wingo::core::format::human_size(1024), "1.0 KiB");
/// ```
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 B".to_owned();
    }

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_bytes() {
        assert_eq!(human_size(0), "0 B");
    }

    #[test]
    fn bytes_are_not_scaled() {
        assert_eq!(human_size(512), "512 B");
    }

    #[test]
    fn binary_units() {
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1024 * 1024), "1.0 MiB");
        assert_eq!(human_size(32_017_047_552), "29.8 GiB");
    }
}
