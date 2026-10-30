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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_formats_cents() {
        assert_eq!(Money(1250).display(), "S/ 12.50");
        assert_eq!(Money(5).display(), "S/ 0.05");
        assert_eq!(Money(-105).display(), "-S/ 1.05");
    }
}
