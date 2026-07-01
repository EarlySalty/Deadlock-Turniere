//! Whitelist-Anker fuer die wenigen erlaubten dynamischen PG-SQL-Stellen.
//!
//! Dynamisch bleibt nur Struktur, die nicht als Bind-Parameter moeglich ist:
//! variable `IN`-Listen, bekannte Reminder-Dedupe-Tabellen und Patch-Update-
//! Builder mit Spalten-Whitelist. Alle Werte muessen weiter gebunden werden.

use sqlx::{Postgres, QueryBuilder};

/// Erlaubte Kategorien fuer dynamische SQL-Konstruktion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicSqlKind {
    VariableInList,
    ReminderDedupeTable,
    PatchUpdateBuilder,
}

/// Vollstaendige Allowlist dynamischer SQL-Kategorien.
pub const ALLOWED_DYNAMIC_SQL_KINDS: [DynamicSqlKind; 3] = [
    DynamicSqlKind::VariableInList,
    DynamicSqlKind::ReminderDedupeTable,
    DynamicSqlKind::PatchUpdateBuilder,
];

/// Sortierte Whitelist der Reminder-Dedupe-Tabellen.
pub const REMINDER_DEDUPE_TABLES: [&str; 3] = [
    "sent_match_reminders",
    "sent_start_reminders",
    "sent_tournament_reminders",
];

/// Reminder-Dedupe-Tabellen, die als Identifier in SQL auftauchen duerfen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderDedupeTable {
    Match,
    Start,
    Tournament,
}

impl ReminderDedupeTable {
    pub fn from_unqualified_name(value: &str) -> Option<Self> {
        match value {
            "sent_match_reminders" => Some(Self::Match),
            "sent_start_reminders" => Some(Self::Start),
            "sent_tournament_reminders" => Some(Self::Tournament),
            _ => None,
        }
    }

    pub const fn unqualified_name(self) -> &'static str {
        match self {
            Self::Match => "sent_match_reminders",
            Self::Start => "sent_start_reminders",
            Self::Tournament => "sent_tournament_reminders",
        }
    }

    pub const fn qualified_name(self) -> &'static str {
        match self {
            Self::Match => r#"turnier."sent_match_reminders""#,
            Self::Start => r#"turnier."sent_start_reminders""#,
            Self::Tournament => r#"turnier."sent_tournament_reminders""#,
        }
    }
}

/// Fuegt eine variable `IN`-Liste aus gebundenen `i64`-Werten an.
///
/// Gibt die Anzahl der Werte zurueck. Bei leerer Liste wird nichts angehaengt;
/// Aufrufer muessen den Leerfall selbst in eine passende Semantik uebersetzen,
/// z. B. `WHERE false`.
pub fn push_i64_bind_list<'args, I>(builder: &mut QueryBuilder<'args, Postgres>, values: I) -> usize
where
    I: IntoIterator<Item = i64>,
{
    let values: Vec<i64> = values.into_iter().collect();
    if values.is_empty() {
        return 0;
    }

    builder.push("(");
    let mut separated = builder.separated(", ");
    let mut count = 0;
    for value in values {
        separated.push_bind(value);
        count += 1;
    }
    separated.push_unseparated(")");
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_dedupe_tables_are_sorted_and_whitelisted() {
        let mut sorted = REMINDER_DEDUPE_TABLES;
        sorted.sort_unstable();
        assert_eq!(REMINDER_DEDUPE_TABLES, sorted);

        for table in REMINDER_DEDUPE_TABLES {
            assert!(ReminderDedupeTable::from_unqualified_name(table).is_some());
        }
        assert!(ReminderDedupeTable::from_unqualified_name("tournaments").is_none());
    }

    #[test]
    fn reminder_dedupe_table_names_are_schema_qualified() {
        assert_eq!(
            ReminderDedupeTable::Tournament.qualified_name(),
            r#"turnier."sent_tournament_reminders""#
        );
        assert_eq!(
            ReminderDedupeTable::Match.unqualified_name(),
            "sent_match_reminders"
        );
    }

    #[test]
    fn allowed_dynamic_sql_kinds_are_explicit() {
        assert_eq!(
            ALLOWED_DYNAMIC_SQL_KINDS,
            [
                DynamicSqlKind::VariableInList,
                DynamicSqlKind::ReminderDedupeTable,
                DynamicSqlKind::PatchUpdateBuilder,
            ]
        );
    }

    #[test]
    fn push_i64_bind_list_uses_binds_and_skips_empty_lists() {
        let mut builder = QueryBuilder::<Postgres>::new("WHERE id IN ");
        assert_eq!(push_i64_bind_list(&mut builder, [10, 20]), 2);
        assert_eq!(builder.sql(), "WHERE id IN ($1, $2)");

        let mut empty = QueryBuilder::<Postgres>::new("WHERE false");
        assert_eq!(push_i64_bind_list(&mut empty, []), 0);
        assert_eq!(empty.sql(), "WHERE false");
    }
}
