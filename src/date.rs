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
