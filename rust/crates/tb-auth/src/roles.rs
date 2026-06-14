//! RBAC: Ableitung der `is_admin`/`is_mod`-Flags aus den Discord-Rollen.
//!
//! Im Python-Original wurden die Admin-/Mod-Rollen-Mengen bei JEDEM
//! authentifizierten Request neu aus den Config-CSV-Strings geparst
//! (`config.py` baute jedes Mal ein frisches `set`). Hier parsen wir sie EINMAL
//! beim Start in [`RoleSets`] und halten sie im AppState — gleiches Ergebnis,
//! kein Hot-Path-Reparsen (Map-Befund „safe").
//!
//! Modell der Rollen-Ordnung (User < Mod < Admin): Admin impliziert Mod. Das
//! entspricht 1:1 der Original-Logik aus `middleware.py`:
//!   `is_admin = roles ∩ admin_ids ≠ ∅`
//!   `is_mod   = is_admin ∨ (roles ∩ mod_ids ≠ ∅)`

use std::collections::HashSet;

use tb_config::Config;

/// Die einmal beim Start materialisierten Rollen-ID-Mengen.
///
/// `admin_ids` = allgemeine Admin-Rollen ∪ Turnier-Admin-Rollen (siehe
/// [`Config::admin_role_ids`]). `mod_ids` = reine Mod-Rollen.
#[derive(Debug, Clone)]
pub struct RoleSets {
    admin_ids: HashSet<String>,
    mod_ids: HashSet<String>,
}

impl RoleSets {
    /// Baut die Mengen aus der Konfiguration. Einmal beim App-Start aufrufen und
    /// das Ergebnis im AppState halten.
    pub fn from_config(config: &Config) -> Self {
        Self {
            admin_ids: config.admin_role_ids().into_iter().collect(),
            mod_ids: config.mod_role_ids().into_iter().collect(),
        }
    }

    /// Direkter Konstruktor für Tests/Spezialfälle.
    pub fn new(admin_ids: HashSet<String>, mod_ids: HashSet<String>) -> Self {
        Self { admin_ids, mod_ids }
    }

    /// `true`, wenn eine der Rollen eine Admin-Rolle ist.
    pub fn is_admin(&self, roles: &[String]) -> bool {
        roles.iter().any(|r| self.admin_ids.contains(r))
    }

    /// `true`, wenn der Nutzer Mod ODER Admin ist (Admin impliziert Mod).
    pub fn is_mod(&self, roles: &[String]) -> bool {
        self.is_admin(roles) || roles.iter().any(|r| self.mod_ids.contains(r))
    }

    /// Berechnet beide Flags in einem Durchgang.
    pub fn flags(&self, roles: &[String]) -> RoleFlags {
        let is_admin = self.is_admin(roles);
        let is_mod = is_admin || roles.iter().any(|r| self.mod_ids.contains(r));
        RoleFlags { is_admin, is_mod }
    }
}

/// Das berechnete Berechtigungs-Paar für eine Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleFlags {
    pub is_admin: bool,
    pub is_mod: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sets() -> RoleSets {
        RoleSets::new(
            ["admin1".to_string(), "admin2".to_string()].into_iter().collect(),
            ["mod1".to_string()].into_iter().collect(),
        )
    }

    fn roles(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn admin_role_setzt_beide_flags() {
        let f = sets().flags(&roles(&["admin1", "irgendwas"]));
        assert!(f.is_admin);
        assert!(f.is_mod, "Admin impliziert Mod");
    }

    #[test]
    fn mod_role_ohne_admin() {
        let f = sets().flags(&roles(&["mod1"]));
        assert!(!f.is_admin);
        assert!(f.is_mod);
    }

    #[test]
    fn user_ohne_rollen() {
        let f = sets().flags(&roles(&["zufall", "noch_eine"]));
        assert!(!f.is_admin);
        assert!(!f.is_mod);
    }

    #[test]
    fn leere_rollen() {
        let f = sets().flags(&[]);
        assert!(!f.is_admin);
        assert!(!f.is_mod);
    }
}
