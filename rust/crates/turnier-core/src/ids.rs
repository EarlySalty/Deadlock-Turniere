//! ID-Konventionen an der HTTP/DB-Grenze.
//!
//! Discord-Snowflakes bleiben in DTOs Strings, werden fuer Postgres-`BIGINT`
//! aber einmal zentral und strikt nach `i64` geparst.

use std::error::Error;
use std::fmt;

/// Fehler beim Parsen einer Discord-ID fuer eine PG-`BIGINT`-Spalte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscordIdParseError {
    Empty,
    NonNumeric,
    OutOfRange,
}

impl fmt::Display for DiscordIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("Discord-ID ist leer"),
            Self::NonNumeric => f.write_str("Discord-ID muss rein numerisch sein"),
            Self::OutOfRange => f.write_str("Discord-ID passt nicht in i64"),
        }
    }
}

impl Error for DiscordIdParseError {}

/// Parst eine Discord-ID aus der Wire-Form in den DB-Typ `BIGINT`/`i64`.
pub fn parse_discord_id(value: &str) -> Result<i64, DiscordIdParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DiscordIdParseError::Empty);
    }
    if !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DiscordIdParseError::NonNumeric);
    }

    let parsed = trimmed
        .parse::<u64>()
        .map_err(|_| DiscordIdParseError::OutOfRange)?;
    i64::try_from(parsed).map_err(|_| DiscordIdParseError::OutOfRange)
}

pub fn parse_participant_id(value: &str) -> Result<i64, DiscordIdParseError> {
    let trimmed = value.trim();
    let Some(digits) = trimmed.strip_prefix('-') else {
        return parse_discord_id(trimmed);
    };
    if !digits.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(DiscordIdParseError::NonNumeric);
    }
    match parse_discord_id(digits)? {
        0 => Err(DiscordIdParseError::NonNumeric),
        magnitude => Ok(-magnitude),
    }
}

pub fn is_guest_participant(id: i64) -> bool {
    id < 0
}

pub fn is_guest_participant_id(value: &str) -> bool {
    parse_participant_id(value).is_ok_and(is_guest_participant)
}

/// Formatiert eine aus PG gelesene Discord-ID zur HTTP-/DTO-kompatiblen Form.
pub fn discord_id_to_string(value: i64) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participant_ids_keep_guests_separate_from_discord() {
        assert_eq!(parse_participant_id("-123"), Ok(-123));
        assert_eq!(parse_participant_id("123"), Ok(123));
        assert!(parse_participant_id("-0").is_err());
        assert!(parse_participant_id("--1").is_err());
        assert!(parse_participant_id("- 5").is_err());
        assert!(parse_participant_id("-+5").is_err());
        assert!(parse_discord_id("-123").is_err());
        assert!(is_guest_participant_id(" -7 "));
        assert!(!is_guest_participant_id("7"));
        assert!(!is_guest_participant_id("-0"));
    }

    #[test]
    fn parse_discord_id_accepts_trimmed_numeric_values() {
        assert_eq!(
            parse_discord_id(" 1495154811799077067 "),
            Ok(1_495_154_811_799_077_067)
        );
        assert_eq!(parse_discord_id("00042"), Ok(42));
    }

    #[test]
    fn parse_discord_id_rejects_invalid_wire_values() {
        assert_eq!(parse_discord_id(""), Err(DiscordIdParseError::Empty));
        assert_eq!(
            parse_discord_id("12.3"),
            Err(DiscordIdParseError::NonNumeric)
        );
        assert_eq!(parse_discord_id("-5"), Err(DiscordIdParseError::NonNumeric));
        assert_eq!(
            parse_discord_id("9223372036854775808"),
            Err(DiscordIdParseError::OutOfRange)
        );
    }

    #[test]
    fn discord_id_to_string_preserves_db_value() {
        assert_eq!(
            discord_id_to_string(1_495_154_811_799_077_067),
            "1495154811799077067"
        );
    }
}
