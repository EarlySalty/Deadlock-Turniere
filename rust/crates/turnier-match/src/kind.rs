//! [`MatchKind`] — der typsichere Ersatz für den stringly-typed `match_type`.
//!
//! Das Python-Original reicht `match_type: str` (`"bracket"` | `"group"`) durch
//! die gesamte Manager-Kette und entscheidet damit über Tabellennamen
//! (`_match_table`), Scope-Spalte (`_match_scope_column`) und Scope-Wert
//! (`_match_scope_value`). Hier kapselt das Enum diese drei Helfer; das konkrete
//! SQL liegt in [`crate::repo`], das die Tabellennamen NICHT als String einsetzt,
//! sondern feste Query-Zweige hat.

use std::fmt;

/// Art eines Matches: Bracket-Match oder Gruppen-Match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchKind {
    /// Match im K.-o.-Bracket (`bracket_matches`).
    Bracket,
    /// Match in der Gruppenphase (`group_matches`).
    Group,
}

impl MatchKind {
    /// Wire-/DB-Repräsentation, identisch zum Python-`match_type`-String.
    /// Wird für `match_casters`/`has_active_task`-Payloads und die
    /// `match_results`/`source`-Felder gebraucht.
    pub fn as_str(self) -> &'static str {
        match self {
            MatchKind::Bracket => "bracket",
            MatchKind::Group => "group",
        }
    }
}

impl fmt::Display for MatchKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_python_match_type() {
        assert_eq!(MatchKind::Bracket.as_str(), "bracket");
        assert_eq!(MatchKind::Group.as_str(), "group");
        assert_eq!(MatchKind::Bracket.to_string(), "bracket");
    }
}
