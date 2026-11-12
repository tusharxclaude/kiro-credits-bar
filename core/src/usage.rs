//! Parsing of `kiro-cli chat --v2 --no-interactive "/usage"` output.
//!
//! Only the first two lines are read. Lines after them differ by plan
//! (overage status, admin notice, plan link) and are ignored on purpose.
//! Anything unexpected is an error, never a guess (see ADR 0001).

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// What Kiro reported at one moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageReading {
    /// Plan label as printed, e.g. `KIRO PRO+`.
    pub plan: String,
    /// First day of the next billing cycle (credits renew on this date).
    pub reset_on: NaiveDate,
    /// Credits used so far in the billing cycle.
    pub used: f64,
    /// The allowance for the billing cycle.
    pub total: f64,
}

impl UsageReading {
    /// Allowance minus used, never negative.
    pub fn remaining(&self) -> f64 {
        (self.total - self.used).max(0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// The output did not have the expected shape. Carries a short reason.
    Unexpected(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Unexpected(why) => write!(f, "unexpected /usage output: {why}"),
        }
    }
}

impl std::error::Error for ParseError {}

fn bad<T>(why: &str) -> Result<T, ParseError> {
    Err(ParseError::Unexpected(why.to_string()))
}

fn number(s: &str) -> Result<f64, ParseError> {
    let cleaned: String = s.trim().chars().filter(|c| *c != ',').collect();
    match cleaned.parse::<f64>() {
        Ok(n) if n.is_finite() && n >= 0.0 => Ok(n),
        _ => bad("not a valid credit amount"),
    }
}

/// Parse the two header lines of `/usage` output.
pub fn parse_usage(text: &str) -> Result<UsageReading, ParseError> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());

    // Line 1: `Estimated Usage | resets on 2026-12-01 | KIRO PRO+`
    let first = match lines.next() {
        Some(l) => l,
        None => return bad("empty output"),
    };
    let parts: Vec<&str> = first.split('|').map(str::trim).collect();
    if parts.len() != 3 || parts[0] != "Estimated Usage" {
        return bad("first line is not `Estimated Usage | resets on <date> | <plan>`");
    }
    let date_text = match parts[1].strip_prefix("resets on ") {
        Some(d) => d.trim(),
        None => return bad("first line has no `resets on` part"),
    };
    let reset_on = match NaiveDate::parse_from_str(date_text, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return bad("reset date is not YYYY-MM-DD"),
    };
    if parts[2].is_empty() {
        return bad("plan name is empty");
    }
    let plan = parts[2].to_string();

    // Line 2: `Credits (349.90 of 2000 covered in plan), 17.5%`
    let second = match lines.next() {
        Some(l) => l,
        None => return bad("missing credits line"),
    };
    let inner = match second.strip_prefix("Credits (") {
        Some(rest) => rest,
        None => return bad("second line does not start with `Credits (`"),
    };
    let (amounts, percent) = match inner.split_once("),") {
        Some(pair) => pair,
        None => return bad("credits line has no `), <percent>%` ending"),
    };
    let amounts = match amounts.strip_suffix(" covered in plan") {
        Some(a) => a,
        None => return bad("credits line is missing `covered in plan`"),
    };
    let (used_text, total_text) = match amounts.split_once(" of ") {
        Some(pair) => pair,
        None => return bad("credits line has no `<used> of <total>`"),
    };
    let used = number(used_text)?;
    let total = number(total_text)?;
    if total <= 0.0 {
        return bad("allowance is zero");
    }
    // The percentage is only a cross-check; it must at least be a number.
    if number(percent.trim().trim_end_matches('%')).is_err() {
        return bad("percentage is not a number");
    }

    Ok(UsageReading {
        plan,
        reset_on,
        used,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREE: &str = include_str!("../../tests/fixtures/usage_free.txt");
    const PRO_PLUS: &str = include_str!("../../tests/fixtures/usage_pro_plus_org_managed.txt");

    #[test]
    fn parses_the_free_plan_fixture() {
        let r = parse_usage(FREE).unwrap();
        assert_eq!(r.plan, "KIRO FREE");
        assert_eq!(r.reset_on, NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!(r.used, 0.0);
        assert_eq!(r.total, 50.0);
    }

    #[test]
    fn parses_the_pro_plus_org_managed_fixture() {
        let r = parse_usage(PRO_PLUS).unwrap();
        assert_eq!(r.plan, "KIRO PRO+");
        assert_eq!(r.used, 349.90);
        assert_eq!(r.total, 2000.0);
        assert!((r.remaining() - 1650.10).abs() < 1e-9);
    }

    #[test]
    fn ignores_lines_after_the_first_two() {
        let text = "Estimated Usage | resets on 2026-12-01 | KIRO PRO\n\
                    Credits (1.50 of 1000 covered in plan), 0.1%\n\
                    Overages: Enabled, something we have never seen\n\
                    Another new line\n";
        let r = parse_usage(text).unwrap();
        assert_eq!(r.total, 1000.0);
    }

    #[test]
    fn accepts_thousands_separators() {
        let text = "Estimated Usage | resets on 2026-12-01 | KIRO POWER\n\
                    Credits (1,234.50 of 10,000 covered in plan), 12.3%\n";
        let r = parse_usage(text).unwrap();
        assert_eq!((r.used, r.total), (1234.5, 10000.0));
    }

    #[test]
    fn remaining_never_goes_negative_when_over_allowance() {
        let text = "Estimated Usage | resets on 2026-12-01 | KIRO PRO\n\
                    Credits (1100.00 of 1000 covered in plan), 110.0%\n";
        assert_eq!(parse_usage(text).unwrap().remaining(), 0.0);
    }

    #[test]
    fn rejects_text_that_is_the_model_talking_instead_of_usage() {
        // What a V3 prompt-path reply looks like: prose, not the header.
        let prose = "To see your usage, run Kiro CLI and type /usage.";
        assert!(parse_usage(prose).is_err());
    }

    #[test]
    fn rejects_malformed_outputs() {
        for text in [
            "",
            "   \n\n",
            "Estimated Usage | KIRO FREE\nCredits (0 of 50 covered in plan), 0%",
            "Estimated Usage | resets on 01/12/2026 | KIRO FREE\nCredits (0 of 50 covered in plan), 0%",
            "Estimated Usage | resets on 2026-12-01 | KIRO FREE",
            "Estimated Usage | resets on 2026-12-01 | KIRO FREE\nCredits (abc of 50 covered in plan), 0%",
            "Estimated Usage | resets on 2026-12-01 | KIRO FREE\nCredits (0 of 0 covered in plan), 0%",
            "Estimated Usage | resets on 2026-12-01 | KIRO FREE\nCredits (-5 of 50 covered in plan), 0%",
            "Estimated Usage | resets on 2026-12-01 | \nCredits (0 of 50 covered in plan), 0%",
        ] {
            assert!(parse_usage(text).is_err(), "should reject: {text:?}");
        }
    }
}
