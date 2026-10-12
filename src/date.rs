use anyhow::{bail, Result};
use chrono::NaiveDate;

pub fn parse_date(input: &str) -> Result<NaiveDate> {
    if input.len() != 10
        || input.as_bytes()[4] != b'-'
        || input.as_bytes()[7] != b'-'
        || !input
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        bail!("Введите дату в формате ГГГГ-ММ-ДД.");
    }

    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("Укажите существующую календарную дату."))?;
    if date.format("%Y-%m-%d").to_string() != input {
        bail!("Введите дату в формате ГГГГ-ММ-ДД.");
    }
    Ok(date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_dates_and_leap_days() {
        assert_eq!(
            parse_date("2024-02-29").unwrap(),
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
        );
        assert!(parse_date("2025-02-28").is_ok());
    }

    #[test]
    fn rejects_impossible_or_malformed_dates() {
        for input in [
            "",
            "2025-02-31",
            "2025-13-01",
            "2025-2-01",
            "25-02-01",
            "2025/02/01",
            "2025-02-01 ",
            "２０２５-02-01",
        ] {
            assert!(parse_date(input).is_err(), "accepted {input:?}");
        }
    }
}
