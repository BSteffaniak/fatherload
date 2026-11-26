//! Credits as players read them (Wave 14 finish): big prices with thousands separators
//! ("1,200,000 cr", not "1200000 cr") on every counter, page, toast and HUD line.

/// `value` with a comma between each group of three digits ("12,500", "-3,000", "940").
#[must_use]
pub fn grouped(value: impl Into<i128>) -> String {
    let value = value.into();
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if value < 0 {
        out.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A figure written by [`grouped`] (or plainly) read back: "1,200" is 1200. `None` for
/// anything else.
#[must_use]
pub fn parse_grouped<T: std::str::FromStr>(text: &str) -> Option<T> {
    let digits: String = text.chars().filter(|c| *c != ',').collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{grouped, parse_grouped};

    #[test]
    fn a_grouped_figure_reads_back() {
        assert_eq!(
            parse_grouped::<u32>(&grouped(1_200_000_u32)),
            Some(1_200_000)
        );
        assert_eq!(parse_grouped::<u32>("940"), Some(940));
        assert_eq!(parse_grouped::<u32>("free"), None);
    }

    #[test]
    fn big_numbers_read_in_groups_of_three() {
        assert_eq!(grouped(0_u32), "0");
        assert_eq!(grouped(940_u32), "940");
        assert_eq!(grouped(1_000_u32), "1,000");
        assert_eq!(grouped(12_500_u64), "12,500");
        assert_eq!(grouped(1_200_000_u32), "1,200,000");
        assert_eq!(grouped(-3_000_i64), "-3,000");
        assert_eq!(grouped(u64::MAX), "18,446,744,073,709,551,615");
    }
}
