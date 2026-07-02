//! Discord-Embed-Modelle als serde-Strukturen (statt ad-hoc `serde_json::json!`).
//!
//! Dadurch lassen sich die Discord-Feldlängengrenzen typgeprüft kappen:
//! Embed-Titel ≤ 256, Beschreibung ≤ 4096, Feld-Name ≤ 256, Feld-Wert ≤ 1024.
//! Das Python-Original kappte gar nicht — über die Längengrenze hinausgehende
//! Inhalte hätte der Broker/Discord abgelehnt. Hier wird defensiv gekürzt
//! (zeichenweise, nicht byteweise, damit Mehrbyte-UTF-8 nicht zerschnitten
//! wird), ohne die fachliche Bedeutung zu ändern.

use serde::Serialize;

/// Discord-Grenze für Embed-Titel und Feld-Namen.
pub const TITLE_MAX: usize = 256;
/// Discord-Grenze für die Embed-Beschreibung.
pub const DESCRIPTION_MAX: usize = 4096;
/// Discord-Grenze für einen Feld-Wert.
pub const FIELD_VALUE_MAX: usize = 1024;

/// Ein Embed-Feld (`{name, value, inline}`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

impl Field {
    /// Erzeugt ein Feld und kappt `name`/`value` auf die Discord-Grenzen.
    pub fn new(name: impl Into<String>, value: impl Into<String>, inline: bool) -> Self {
        Self {
            name: truncate(name.into(), TITLE_MAX),
            value: truncate(value.into(), FIELD_VALUE_MAX),
            inline,
        }
    }
}

/// Ein Discord-Embed (`{title?, description?, fields}`). `title`/`description`
/// werden ausgelassen, wenn nicht gesetzt — exakt wie im Original, das die
/// Schlüssel je nach Aufruf weglässt.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Embed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<Field>,
}

impl Embed {
    /// Leeres Embed (nur `fields`).
    pub fn new() -> Self {
        Self {
            title: None,
            description: None,
            fields: Vec::new(),
        }
    }

    /// Setzt den Titel (gekappt auf [`TITLE_MAX`]).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(truncate(title.into(), TITLE_MAX));
        self
    }

    /// Setzt die Beschreibung (gekappt auf [`DESCRIPTION_MAX`]).
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(truncate(description.into(), DESCRIPTION_MAX));
        self
    }

    /// Hängt ein Feld an.
    pub fn field(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        inline: bool,
    ) -> Self {
        self.fields.push(Field::new(name, value, inline));
        self
    }
}

impl Default for Embed {
    fn default() -> Self {
        Self::new()
    }
}

/// Kürzt `value` auf höchstens `max` Zeichen (nicht Bytes), damit Mehrbyte-
/// UTF-8 (Emojis, Umlaute) nicht in der Mitte zerschnitten wird.
fn truncate(value: String, max: usize) -> String {
    if value.chars().count() <= max {
        value
    } else {
        value.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feld_wert_wird_auf_1024_gekappt() {
        let long = "x".repeat(2000);
        let f = Field::new("Name", long, false);
        assert_eq!(f.value.chars().count(), FIELD_VALUE_MAX);
    }

    #[test]
    fn beschreibung_wird_auf_4096_gekappt() {
        let long = "y".repeat(5000);
        let e = Embed::new().description(long);
        assert_eq!(e.description.unwrap().chars().count(), DESCRIPTION_MAX);
    }

    #[test]
    fn kurze_werte_unveraendert() {
        let f = Field::new("N", "kurz", true);
        assert_eq!(f.value, "kurz");
        assert!(f.inline);
    }

    #[test]
    fn mehrbyte_wird_nicht_zerschnitten() {
        // 300 Umlaute -> auf 256 Zeichen gekappt, gültiges UTF-8.
        let f = Field::new("ä".repeat(300), "v", false);
        assert_eq!(f.name.chars().count(), TITLE_MAX);
    }

    #[test]
    fn fehlende_felder_werden_nicht_serialisiert() {
        let e = Embed::new().field("a", "b", false);
        let json = serde_json::to_value(&e).unwrap();
        assert!(json.get("title").is_none());
        assert!(json.get("description").is_none());
        assert!(json.get("fields").is_some());
    }
}
