//! Erzeugung des Discord-Match-Channel-Namens aus Team-Namen.
//!
//! Repliziert `_slugify` + `_build_match_channel_name` aus dem Python-Original.
//! Der Python-Slug nutzt `unicodedata.normalize("NFKD", …)` + ASCII-Ignore.
//! Da Rust ohne externe Crate keine NFKD-Zerlegung kennt, werden die für
//! Team-Namen praktisch relevanten lateinischen Akzente/Umlaute deterministisch
//! transliteriert; der danach verbleibende Regelpfad (nicht-alphanumerisch → `-`,
//! Mehrfach-`-` kollabieren, trimmen, lowercase, Fallback `team`) ist identisch.

/// Discord-Channel-Namen sind auf 100 Zeichen begrenzt.
pub const CHANNEL_NAME_MAX: usize = 100;

/// Wandelt einen Team-Namen in einen URL-tauglichen Slug. Leerer Rest → `"team"`.
pub fn slugify(value: &str) -> String {
    let ascii = transliterate_to_ascii(value);

    // Nicht-alphanumerische Folgen werden zu einem einzelnen '-'.
    let mut cleaned = String::with_capacity(ascii.len());
    let mut last_was_dash = false;
    for ch in ascii.chars() {
        if ch.is_ascii_alphanumeric() {
            cleaned.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            cleaned.push('-');
            last_was_dash = true;
        }
    }

    // Führende/abschließende '-' entfernen, dann lowercase.
    let trimmed = cleaned.trim_matches('-').to_lowercase();
    if trimmed.is_empty() {
        "team".to_string()
    } else {
        trimmed
    }
}

/// Baut den Match-Channel-Namen `match-<slug1>-vs-<slug2>`, gekappt auf 100
/// Zeichen.
pub fn build_match_channel_name(team1_name: &str, team2_name: &str) -> String {
    let name = format!("match-{}-vs-{}", slugify(team1_name), slugify(team2_name));
    name.chars().take(CHANNEL_NAME_MAX).collect()
}

/// Deterministische Transliteration der praktisch vorkommenden lateinischen
/// Akzente/Umlaute auf ASCII. Alles, was kein bekanntes Mapping hat und nicht
/// ASCII ist, wird verworfen — genau wie der `encode("ascii", "ignore")`-Schritt
/// des Originals nach NFKD-Zerlegung (kombinierende Akzente fallen weg).
fn transliterate_to_ascii(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii() {
            out.push(ch);
            continue;
        }
        match ch {
            'ä' | 'à' | 'á' | 'â' | 'ã' | 'å' => out.push('a'),
            'Ä' | 'À' | 'Á' | 'Â' | 'Ã' | 'Å' => out.push('A'),
            'ö' | 'ò' | 'ó' | 'ô' | 'õ' | 'ø' => out.push('o'),
            'Ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ø' => out.push('O'),
            'ü' | 'ù' | 'ú' | 'û' => out.push('u'),
            'Ü' | 'Ù' | 'Ú' | 'Û' => out.push('U'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'É' | 'È' | 'Ê' | 'Ë' => out.push('E'),
            'í' | 'ì' | 'î' | 'ï' => out.push('i'),
            'Í' | 'Ì' | 'Î' | 'Ï' => out.push('I'),
            'ç' => out.push('c'),
            'Ç' => out.push('C'),
            'ñ' => out.push('n'),
            'Ñ' => out.push('N'),
            // Kein ASCII-Äquivalent (z. B. ß → NFKD lässt 'ß' bestehen und
            // ascii-ignore verwirft es) → verwerfen, wie das Original.
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_grundfaelle() {
        assert_eq!(slugify("Team Alpha"), "team-alpha");
        assert_eq!(slugify("  Hello  World  "), "hello-world");
        assert_eq!(slugify("A!!!B"), "a-b");
        assert_eq!(slugify("---"), "team");
        assert_eq!(slugify(""), "team");
    }

    #[test]
    fn slug_akzente_und_umlaute() {
        assert_eq!(slugify("Café"), "cafe");
        assert_eq!(slugify("Müller"), "muller");
        // 'ß' hat kein ASCII-Äquivalent und wird (wie im Original) verworfen.
        assert_eq!(slugify("Straße"), "strae");
    }

    #[test]
    fn channel_name_aufbau_und_kappung() {
        assert_eq!(
            build_match_channel_name("Alpha", "Beta"),
            "match-alpha-vs-beta"
        );
        let long = "x".repeat(200);
        let name = build_match_channel_name(&long, &long);
        assert_eq!(name.chars().count(), CHANNEL_NAME_MAX);
        assert!(name.starts_with("match-"));
    }
}
