//! Zeit-Konventionen fuer PG-`TIMESTAMPTZ`.

use chrono::{DateTime, Utc};

/// Einheitlicher Clock-Zugriff fuer neue PG-Persistenzpfade.
pub fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_utc_returns_utc_timestamp() {
        let now = now_utc();
        assert_eq!(now.timezone(), Utc);
    }
}
