pub mod results_panel;
pub mod scan_panel;
pub mod settings;
pub mod status_panel;

/// German thousands grouping: 3627854 → "3.627.854".
pub fn format_thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands_with_dots() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1000), "1.000");
        assert_eq!(format_thousands(3_627_854), "3.627.854");
    }
}
