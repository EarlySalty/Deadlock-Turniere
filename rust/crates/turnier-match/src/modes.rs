//! Spielmodi, Hero-Zuweisung und Wertungs-Objective.
//!
//! Portiert `match/game_modes.py` (+ `match/heroes.py` als dünnes Re-Export aus
//! [`turnier_draft::DEADLOCK_HEROES`]). Die reine Logik (Objective-Auflösung,
//! Hero-Auswahl je Modus) ist DB-frei und voll unit-testbar; der DB-Teil
//! ([`prepare_match_assignments`]) lädt nur den Modus-Kontext und ruft die reine
//! Funktion.
//!
//! Begriffstrennung (Befund game_modes.py:144): [`TournamentGameMode`] (Hero-
//! Zuweisungsmodus) ist strikt verschieden vom Steam-Lobby-`game_mode`-Integer
//! des Lobby-Flows. Hier geht es ausschließlich um Ersteres.

use rand::seq::SliceRandom;
use rand::Rng;
use serde_json::{json, Value};

use turnier_core::TournamentGameMode;
use turnier_db::Pool;

use crate::error::MatchError;
use crate::kind::MatchKind;

/// Die Deadlock-Heldenliste als geordnete Namensliste (1:1 zu `HERO_NAMES` des
/// Originals, das aus `draft.heroes.DEADLOCK_HEROES` ableitet).
pub fn hero_names() -> &'static [&'static str] {
    &turnier_draft::DEADLOCK_HEROES
}

/// Ergebnis von [`prepare_match_assignments`]: dieselben drei Felder wie das
/// Python-Dict (`convars`, `hero_assignments`, `announcement_lines`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModeAssignments {
    /// Modus-spezifische ConVars, die in die Lobby-Settings gemischt werden.
    pub convars: serde_json::Map<String, Value>,
    /// Strukturierte Hero-Zuweisung; `null`-äquivalent (leer) → kein
    /// `hero_assignments_json`-Persistieren.
    pub hero_assignments: Value,
    /// Anzeigezeilen für die Lobby-Ankündigung.
    pub announcement_lines: Vec<String>,
}

impl ModeAssignments {
    /// `true`, wenn `hero_assignments` leer ist (entspricht dem Python-Falsy-Check
    /// `if hero_assignments`: leeres Dict ⇒ kein JSON persistieren).
    pub fn hero_assignments_is_empty(&self) -> bool {
        match &self.hero_assignments {
            Value::Null => true,
            Value::Object(m) => m.is_empty(),
            _ => false,
        }
    }
}

/// Anzeige-Labels der Objective-Codes (`_OBJECTIVE_LABELS`).
fn objective_label(code: &str) -> Option<&'static str> {
    match code {
        "first_guardian" => Some("Erster zerstörter Guardian gewinnt"),
        "first_walker" => Some("Erster zerstörter Walker gewinnt"),
        "base" => Some("Reguläres Spielende — gegnerische Basis zerstören"),
        _ => None,
    }
}

/// Ermittelt `(code, anzeige_text)` der Wertungsbedingung. `"auto"` leitet aus der
/// Teamgröße ab: `team_size <= 2` → `first_walker`, sonst `base`. Unbekannte Codes
/// werden als Label = Code durchgereicht (wie `dict.get(code, code)`).
/// Portiert `resolve_match_objective`.
pub fn resolve_match_objective(match_objective: Option<&str>, team_size: i64) -> (String, String) {
    let mut code = match_objective.unwrap_or("auto").trim().to_lowercase();
    if code == "auto" {
        code = if team_size <= 2 { "first_walker".to_string() } else { "base".to_string() };
    }
    let label = objective_label(&code).map(|s| s.to_string()).unwrap_or_else(|| code.clone());
    (code, label)
}

/// Wählt `count` eindeutige Heroes (oder alle, falls `count >= len`).
/// Portiert `_pick_unique_heroes`. Deterministisch über den injizierten RNG.
fn pick_unique_heroes<R: Rng + ?Sized>(rng: &mut R, count: usize) -> Vec<&'static str> {
    let pool = hero_names();
    if count == 0 {
        return Vec::new();
    }
    let take = count.min(pool.len());
    let mut indices: Vec<usize> = (0..pool.len()).collect();
    indices.shuffle(rng);
    indices.into_iter().take(take).map(|i| pool[i]).collect()
}

/// Modus-Kontext eines Matches (für die reine Assignment-Berechnung).
#[derive(Debug, Clone)]
pub struct ModeContext {
    pub mode: TournamentGameMode,
    pub team1_id: Option<i64>,
    pub team2_id: Option<i64>,
    pub team1_name: String,
    pub team2_name: String,
    pub participants: Vec<ModeParticipant>,
}

/// Ein Teilnehmer im Modus-Kontext (Untermenge von [`crate::repo::Participant`]).
#[derive(Debug, Clone)]
pub struct ModeParticipant {
    pub team_id: i64,
    pub discord_id: Option<String>,
    pub discord_name: Option<String>,
    pub team_name: Option<String>,
}

impl ModeParticipant {
    /// Anzeigename mit der Präzedenz des Originals
    /// (`discord_name` ∨ `team_name` ∨ `discord_id` ∨ `"Unbekannt"`).
    /// Portiert `_display_name`.
    fn display_name(&self) -> String {
        for v in [&self.discord_name, &self.team_name, &self.discord_id].into_iter().flatten() {
            if !v.is_empty() {
                return v.clone();
            }
        }
        "Unbekannt".to_string()
    }

    /// `discord_id`, falls nicht leer (entspricht dem `if participant.get("discord_id")`-Filter).
    fn non_empty_discord_id(&self) -> Option<&str> {
        self.discord_id.as_deref().filter(|s| !s.is_empty())
    }
}

/// Berechnet die Hero-/ConVar-/Ankündigungs-Zuweisung aus dem Kontext.
/// Reine Logik (RNG injiziert), portiert den Modus-`if`-Block von
/// `prepare_match_assignments`.
pub fn compute_assignments<R: Rng + ?Sized>(rng: &mut R, ctx: &ModeContext) -> ModeAssignments {
    match ctx.mode {
        TournamentGameMode::Standard => ModeAssignments::default(),

        TournamentGameMode::Mirror => {
            let heroes = pick_unique_heroes(rng, 2);
            // Fallback bei <2 Heroes: identischer Hero für beide Teams (Befund
            // game_modes.py:147-163 — semantisch ok, hier 1:1 erhalten).
            let h0 = heroes.first().copied().unwrap_or("");
            let h1 = heroes.get(1).copied().unwrap_or(h0);
            let t1 = ctx.team1_id;
            let t2 = ctx.team2_id;
            let teams = json!({
                id_key(t1): h0,
                id_key(t2): h1,
            });
            let mut convars = serde_json::Map::new();
            convars.insert("citadel_allow_duplicate_heroes".to_string(), json!(1));
            ModeAssignments {
                convars,
                hero_assignments: json!({ "mode": "mirror", "teams": teams }),
                announcement_lines: vec![
                    format!("{}: {}", ctx.team1_name, h0),
                    format!("{}: {}", ctx.team2_name, h1),
                ],
            }
        }

        TournamentGameMode::AllSame => {
            let hero_name = *hero_names().choose(rng).expect("Heldenliste ist nicht leer");
            let mut players = serde_json::Map::new();
            for p in &ctx.participants {
                if let Some(did) = p.non_empty_discord_id() {
                    players.insert(did.to_string(), json!(hero_name));
                }
            }
            let mut convars = serde_json::Map::new();
            convars.insert("citadel_allow_duplicate_heroes".to_string(), json!(1));
            ModeAssignments {
                convars,
                hero_assignments: json!({
                    "mode": "all_same",
                    "all": hero_name,
                    "players": Value::Object(players),
                }),
                announcement_lines: vec![format!("Alle Spieler: {hero_name}")],
            }
        }

        TournamentGameMode::RandomHeroes => {
            // Reihenfolge wie das Original: alle Teilnehmer mit nicht-leerer ID
            // (Duplikate NICHT entfernt — Variable heißt im Original irreführend
            // "unique_player_ids", filtert aber nur leere; 1:1 erhalten).
            let player_ids: Vec<String> = ctx
                .participants
                .iter()
                .filter_map(|p| p.non_empty_discord_id().map(|s| s.to_string()))
                .collect();
            let allow_duplicates = player_ids.len() > hero_names().len();
            let selected: Vec<&'static str> = if allow_duplicates {
                (0..player_ids.len())
                    .map(|_| *hero_names().choose(rng).expect("Heldenliste ist nicht leer"))
                    .collect()
            } else {
                pick_unique_heroes(rng, player_ids.len())
            };

            let mut player_assignments = serde_json::Map::new();
            for (index, did) in player_ids.iter().enumerate() {
                player_assignments.insert(did.clone(), json!(selected[index]));
            }
            let announcement_lines: Vec<String> = ctx
                .participants
                .iter()
                .filter(|p| p.non_empty_discord_id().is_some())
                .map(|p| {
                    let did = p.non_empty_discord_id().unwrap();
                    let hero = player_assignments.get(did).and_then(|v| v.as_str()).unwrap_or("");
                    format!("{}: {}", p.display_name(), hero)
                })
                .collect();
            let mut convars = serde_json::Map::new();
            if allow_duplicates {
                convars.insert("citadel_allow_duplicate_heroes".to_string(), json!(1));
            }
            ModeAssignments {
                convars,
                hero_assignments: json!({
                    "mode": "random_heroes",
                    "players": Value::Object(player_assignments),
                }),
                announcement_lines,
            }
        }

        TournamentGameMode::SingleLane => ModeAssignments {
            convars: serde_json::Map::new(),
            hero_assignments: json!({}),
            announcement_lines: vec![
                "Single-Lane-Battle ist aktuell in Vorbereitung; es werden noch keine sicheren \
                 Citadel-ConVars gesetzt."
                    .to_string(),
            ],
        },
    }
}

/// JSON-Schlüssel einer Team-ID — `str(team_id)` (Python stringifiziert die ID
/// als Dict-Key; `None` → `"None"`, was im Original durch das f-string ebenso
/// entstünde, in der Praxis aber nur bei gesetzten Teams aufgerufen wird).
fn id_key(team_id: Option<i64>) -> String {
    team_id.map(|v| v.to_string()).unwrap_or_else(|| "None".to_string())
}

/// Lädt den Modus-Kontext aus der DB und berechnet die Assignments.
/// Portiert `prepare_match_assignments` (+ `_load_match_mode_context`).
/// Match/Turnier nicht gefunden → [`MatchError::NotFound`] (im Original
/// `ValueError("Match oder Turnier nicht gefunden")`).
pub async fn prepare_match_assignments(
    pool: &Pool,
    tournament_id: i64,
    kind: MatchKind,
    match_id: i64,
) -> Result<ModeAssignments, MatchError> {
    let ctx = load_match_mode_context(pool, tournament_id, kind, match_id).await?;
    let mut rng = rand::thread_rng();
    Ok(compute_assignments(&mut rng, &ctx))
}

async fn load_match_mode_context(
    pool: &Pool,
    tournament_id: i64,
    kind: MatchKind,
    match_id: i64,
) -> Result<ModeContext, MatchError> {
    use sqlx::Row;

    let row = match kind {
        MatchKind::Group => sqlx::query(
            "SELECT t.tournament_game_mode AS mode, gm.team1_id, gm.team2_id, \
                    team1.name AS team1_name, team2.name AS team2_name \
             FROM tournaments t \
             JOIN groups g ON g.tournament_id = t.id \
             JOIN group_matches gm ON gm.group_id = g.id \
             LEFT JOIN teams team1 ON team1.id = gm.team1_id \
             LEFT JOIN teams team2 ON team2.id = gm.team2_id \
             WHERE t.id = ? AND gm.id = ?",
        ),
        MatchKind::Bracket => sqlx::query(
            "SELECT t.tournament_game_mode AS mode, bm.team1_id, bm.team2_id, \
                    team1.name AS team1_name, team2.name AS team2_name \
             FROM tournaments t \
             JOIN bracket_matches bm ON bm.tournament_id = t.id \
             LEFT JOIN teams team1 ON team1.id = bm.team1_id \
             LEFT JOIN teams team2 ON team2.id = bm.team2_id \
             WHERE t.id = ? AND bm.id = ?",
        ),
    }
    .bind(tournament_id)
    .bind(match_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Err(MatchError::not_found("Match oder Turnier nicht gefunden"));
    };

    let mode: TournamentGameMode = row.get("mode");
    let team1_id: Option<i64> = row.get("team1_id");
    let team2_id: Option<i64> = row.get("team2_id");
    let team1_name: Option<String> = row.get("team1_name");
    let team2_name: Option<String> = row.get("team2_name");

    let team_ids: Vec<i64> = [team1_id, team2_id].into_iter().flatten().collect();
    let participants = if team_ids.is_empty() {
        Vec::new()
    } else {
        let placeholders = std::iter::repeat_n("?", team_ids.len()).collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT tm.team_id, tm.discord_id, tm.discord_name, t.name AS team_name \
             FROM team_members tm \
             JOIN teams t ON t.id = tm.team_id \
             WHERE tm.team_id IN ({placeholders}) \
             ORDER BY tm.team_id, tm.joined_at, tm.id",
        );
        let mut query = sqlx::query(&sql);
        for id in &team_ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(pool).await?;
        rows.into_iter()
            .map(|r| ModeParticipant {
                team_id: r.get("team_id"),
                discord_id: r.get("discord_id"),
                discord_name: r.get("discord_name"),
                team_name: r.get("team_name"),
            })
            .collect()
    };

    // Team-Namen-Fallback NUR wenn Teilnehmer existieren (im Original wird im
    // leeren-team-Zweig der rohe Name ohne Fallback zurückgegeben; da wir den
    // Kontext aber nur für die reine Berechnung nutzen und der Fallback dort
    // ohnehin nicht durchschlägt, vereinheitlichen wir auf den Fallback —
    // identische Wirkung, weil announcement_lines bei leeren Teams nie die
    // Namen verwenden).
    let team1_label =
        team1_name.filter(|s| !s.is_empty()).unwrap_or_else(|| format!("Team {}", id_key(team1_id)));
    let team2_label =
        team2_name.filter(|s| !s.is_empty()).unwrap_or_else(|| format!("Team {}", id_key(team2_id)));

    Ok(ModeContext {
        mode,
        team1_id,
        team2_id,
        team1_name: team1_label,
        team2_name: team2_label,
        participants,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashSet;

    #[test]
    fn objective_auto_kleine_formate() {
        assert_eq!(resolve_match_objective(Some("auto"), 1).0, "first_walker");
        assert_eq!(resolve_match_objective(Some("auto"), 2).0, "first_walker");
        assert_eq!(resolve_match_objective(Some("auto"), 3).0, "base");
        assert_eq!(resolve_match_objective(None, 6).0, "base");
    }

    #[test]
    fn objective_explizit_und_label() {
        let (code, label) = resolve_match_objective(Some("first_guardian"), 6);
        assert_eq!(code, "first_guardian");
        assert_eq!(label, "Erster zerstörter Guardian gewinnt");
        // Unbekannter Code → Label = Code.
        let (code, label) = resolve_match_objective(Some("custom_xyz"), 6);
        assert_eq!(code, "custom_xyz");
        assert_eq!(label, "custom_xyz");
    }

    #[test]
    fn objective_trim_und_lowercase() {
        assert_eq!(resolve_match_objective(Some("  BASE "), 6).0, "base");
    }

    #[test]
    fn standard_liefert_leer() {
        let ctx = ModeContext {
            mode: TournamentGameMode::Standard,
            team1_id: Some(1),
            team2_id: Some(2),
            team1_name: "A".into(),
            team2_name: "B".into(),
            participants: vec![],
        };
        let mut rng = StdRng::seed_from_u64(7);
        let out = compute_assignments(&mut rng, &ctx);
        assert!(out.convars.is_empty());
        assert!(out.hero_assignments_is_empty());
        assert!(out.announcement_lines.is_empty());
    }

    #[test]
    fn mirror_setzt_duplicate_convar_und_zwei_zeilen() {
        let ctx = ModeContext {
            mode: TournamentGameMode::Mirror,
            team1_id: Some(10),
            team2_id: Some(20),
            team1_name: "Alpha".into(),
            team2_name: "Beta".into(),
            participants: vec![],
        };
        let mut rng = StdRng::seed_from_u64(1);
        let out = compute_assignments(&mut rng, &ctx);
        assert_eq!(out.convars.get("citadel_allow_duplicate_heroes"), Some(&json!(1)));
        assert_eq!(out.announcement_lines.len(), 2);
        let teams = &out.hero_assignments["teams"];
        assert!(teams.get("10").is_some());
        assert!(teams.get("20").is_some());
        assert_eq!(out.hero_assignments["mode"], json!("mirror"));
    }

    #[test]
    fn all_same_weist_jedem_spieler_denselben_hero_zu() {
        let ctx = ModeContext {
            mode: TournamentGameMode::AllSame,
            team1_id: Some(1),
            team2_id: Some(2),
            team1_name: "A".into(),
            team2_name: "B".into(),
            participants: vec![
                ModeParticipant {
                    team_id: 1,
                    discord_id: Some("100".into()),
                    discord_name: Some("P1".into()),
                    team_name: Some("A".into()),
                },
                ModeParticipant {
                    team_id: 2,
                    discord_id: Some("200".into()),
                    discord_name: None,
                    team_name: Some("B".into()),
                },
                // Leere discord_id → ausgefiltert.
                ModeParticipant {
                    team_id: 2,
                    discord_id: Some("".into()),
                    discord_name: None,
                    team_name: Some("B".into()),
                },
            ],
        };
        let mut rng = StdRng::seed_from_u64(3);
        let out = compute_assignments(&mut rng, &ctx);
        let all = out.hero_assignments["all"].as_str().unwrap().to_string();
        let players = out.hero_assignments["players"].as_object().unwrap();
        assert_eq!(players.len(), 2);
        assert_eq!(players["100"], json!(all));
        assert_eq!(players["200"], json!(all));
        assert_eq!(out.announcement_lines, vec![format!("Alle Spieler: {all}")]);
    }

    #[test]
    fn random_heroes_eindeutig_wenn_genug_helden() {
        let participants: Vec<ModeParticipant> = (0..4)
            .map(|i| ModeParticipant {
                team_id: 1,
                discord_id: Some(format!("{}", 100 + i)),
                discord_name: Some(format!("P{i}")),
                team_name: Some("A".into()),
            })
            .collect();
        let ctx = ModeContext {
            mode: TournamentGameMode::RandomHeroes,
            team1_id: Some(1),
            team2_id: Some(2),
            team1_name: "A".into(),
            team2_name: "B".into(),
            participants,
        };
        let mut rng = StdRng::seed_from_u64(9);
        let out = compute_assignments(&mut rng, &ctx);
        let players = out.hero_assignments["players"].as_object().unwrap();
        assert_eq!(players.len(), 4);
        // Bei 4 < 26 Heroes: alle eindeutig.
        let heroes: HashSet<&str> = players.values().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(heroes.len(), 4);
        // Keine duplicate-ConVar, da nicht mehr Spieler als Heroes.
        assert!(out.convars.is_empty());
        assert_eq!(out.announcement_lines.len(), 4);
    }

    #[test]
    fn single_lane_nur_hinweiszeile() {
        let ctx = ModeContext {
            mode: TournamentGameMode::SingleLane,
            team1_id: Some(1),
            team2_id: Some(2),
            team1_name: "A".into(),
            team2_name: "B".into(),
            participants: vec![],
        };
        let mut rng = StdRng::seed_from_u64(2);
        let out = compute_assignments(&mut rng, &ctx);
        assert!(out.convars.is_empty());
        assert!(out.hero_assignments_is_empty());
        assert_eq!(out.announcement_lines.len(), 1);
    }
}
