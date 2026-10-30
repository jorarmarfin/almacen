use serde::{Deserialize, Serialize};

/// Monto en céntimos. Nunca float (spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(pub i64);

impl Money {
    pub fn cents(self) -> i64 {
        self.0
    }

    /// Formato "S/ 12.50" para tickets y logs.
    pub fn display(self) -> String {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.abs();
        format!("{sign}S/ {}.{:02}", abs / 100, abs % 100)
    }
}

/// Parsea "12", "12.5", "12,50" a céntimos sin pasar por float. Rechaza negativos y >2 decimales.
pub fn parse_cents(input: &str) -> Option<i64> {
    let t = input.trim().replace(',', ".");
    if t.is_empty() || t.starts_with('-') {
        return None;
    }
    let (int, frac) = match t.split_once('.') {
        Some((i, f)) => (i, f),
        None => (t.as_str(), ""),
    };
    if int.is_empty() && frac.is_empty() || frac.len() > 2 {
        return None;
    }
    if !int.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let i: i64 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let f: i64 = format!("{frac:0<2}").parse().ok()?;
    i.checked_mul(100)?.checked_add(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cents_cases() {
        assert_eq!(parse_cents("12"), Some(1200));
        assert_eq!(parse_cents("12.5"), Some(1250));
        assert_eq!(parse_cents("12,50"), Some(1250));
        assert_eq!(parse_cents(".5"), Some(50));
        assert_eq!(parse_cents("0.05"), Some(5));
        assert_eq!(parse_cents("1.234"), None);
        assert_eq!(parse_cents("-1"), None);
        assert_eq!(parse_cents("abc"), None);
        assert_eq!(parse_cents(""), None);
    }

    #[test]
    fn display_formats_cents() {
        assert_eq!(Money(1250).display(), "S/ 12.50");
        assert_eq!(Money(5).display(), "S/ 0.05");
        assert_eq!(Money(-105).display(), "-S/ 1.05");
    }
}
