//! Notification-Event-Domäne für [`notify_users`](crate::notifier::notify_users).
//!
//! Ersetzt die stringly-typed `event_type`-Map des Python-Originals
//! (`_NOTIFICATION_EVENT_COLUMNS` + `_NOTIFICATION_DEFAULTS`) durch ein echtes
//! Enum. Jede Variante kennt ihre `user_profiles`-Spalte UND ihr Default-Flag,
//! sodass die Defaults nur EINMAL definiert sind (behebt die doppelte/driftende
//! Default-Quelle, Befund discord_notifier.py:28-34/270-275 — "safe").

/// Default des DM-Master-Schalters (`notify_discord_dm`). Im Schema
/// `NOT NULL DEFAULT 1`; für profil-lose User gilt im Original das EVENT-Default
/// (siehe [`NotificationEvent::default_flag`]) — dieses Verhalten wird in
/// [`notify_users`](crate::notifier::notify_users) bewusst 1:1 erhalten.
pub const NOTIFY_DM_DEFAULT: bool = true;

/// Ein benachrichtigungsauslösendes Ereignis. Der `event_type`-String des
/// Aufrufers wird über [`NotificationEvent::from_str`] aufgelöst.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationEvent {
    MatchStart,
    Checkin,
    TeamInvite,
    TournamentNews,
    RegistrationReminder,
}

impl NotificationEvent {
    /// Löst den Wire-`event_type`-String auf. Unbekannte Werte → `None`
    /// (im Original `ValueError`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "match_start" => Some(Self::MatchStart),
            "checkin" => Some(Self::Checkin),
            "team_invite" => Some(Self::TeamInvite),
            "tournament_news" => Some(Self::TournamentNews),
            "registration_reminder" => Some(Self::RegistrationReminder),
            _ => None,
        }
    }

    /// Der Wire-`event_type`-String (für das `discord_tasks`-Payload und DMs).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MatchStart => "match_start",
            Self::Checkin => "checkin",
            Self::TeamInvite => "team_invite",
            Self::TournamentNews => "tournament_news",
            Self::RegistrationReminder => "registration_reminder",
        }
    }

    /// Die zugehörige `user_profiles`-Spalte (whitelisted — fließt in keine
    /// dynamische SQL-Konkatenation auf Werteebene ein).
    pub fn column(self) -> &'static str {
        match self {
            Self::MatchStart => "notify_match_start",
            Self::Checkin => "notify_checkin",
            Self::TeamInvite => "notify_team_invite",
            Self::TournamentNews => "notify_tournament_news",
            Self::RegistrationReminder => "notify_registration_reminder",
        }
    }

    /// Das Default-Flag, falls für eine `discord_id` kein Profil existiert.
    /// Entspricht `_NOTIFICATION_DEFAULTS` (tournament_news = `false`, Rest
    /// `true`).
    pub fn default_flag(self) -> bool {
        !matches!(self, Self::TournamentNews)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alle_event_typen_aufloesbar() {
        for (s, ev) in [
            ("match_start", NotificationEvent::MatchStart),
            ("checkin", NotificationEvent::Checkin),
            ("team_invite", NotificationEvent::TeamInvite),
            ("tournament_news", NotificationEvent::TournamentNews),
            (
                "registration_reminder",
                NotificationEvent::RegistrationReminder,
            ),
        ] {
            assert_eq!(NotificationEvent::parse(s), Some(ev));
            assert_eq!(ev.as_str(), s);
        }
        assert_eq!(NotificationEvent::parse("unbekannt"), None);
    }

    #[test]
    fn nur_tournament_news_default_false() {
        assert!(NotificationEvent::MatchStart.default_flag());
        assert!(NotificationEvent::Checkin.default_flag());
        assert!(NotificationEvent::TeamInvite.default_flag());
        assert!(!NotificationEvent::TournamentNews.default_flag());
        assert!(NotificationEvent::RegistrationReminder.default_flag());
    }

    #[test]
    fn spalten_mapping_stabil() {
        assert_eq!(NotificationEvent::MatchStart.column(), "notify_match_start");
        assert_eq!(
            NotificationEvent::TournamentNews.column(),
            "notify_tournament_news"
        );
    }
}
