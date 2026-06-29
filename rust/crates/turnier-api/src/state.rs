//! Der geteilte Anwendungszustand (`AppState`), den jede Route über axum erhält.
//!
//! Bündelt die langlebigen Handles auf alle Subsysteme. Alle Felder sind billig
//! klonbar (Pool ist intern ref-gezählt; die Dienste liegen hinter `Arc`), sodass
//! axum den State pro Request klonen kann.

use std::sync::Arc;

use turnier_config::Config;
use turnier_db::Pool;
use turnier_discord::{BrokerClient, DiscordNotifier};
use turnier_match::{MatchManager, SteamBridge};
use turnier_steam::RankResolver;

/// Zentraler, geteilter Anwendungszustand.
#[derive(Clone)]
pub struct AppState {
    /// Haupt-DB-Pool auf die Turnier-SQLite.
    pub pool: Pool,
    /// Aufgelöste Laufzeit-Konfiguration.
    pub config: Arc<Config>,
    /// Beim Start materialisierte Admin-/Mod-Rollen-Mengen (RBAC).
    pub role_sets: turnier_auth::RoleSets,
    /// Client für den delegierten Discord-OAuth-Flow.
    pub oauth: turnier_auth::OAuthClient,
    /// Match-Lebenszyklus-Orchestrator (hält selbst Discord/Steam-Bridge).
    pub match_manager: Arc<MatchManager>,
    /// Rang-Resolver (Cache -> Steam-Bridge -> Discord-Rollen).
    pub rank_resolver: Arc<dyn RankResolver>,
    /// Discord-Notifier für direkte Effekte aus Routen + `advance_tournament_status`.
    pub notifier: Arc<DiscordNotifier>,
}

impl AppState {
    /// Baut den kompletten Anwendungszustand aus Pool und Konfiguration.
    ///
    /// Discord-Notifier und Steam-Bridge degradieren sauber, wenn nicht
    /// konfiguriert (kein Token / keine Bridge-DB), exakt wie in den Crates.
    pub async fn build(pool: Pool, config: Arc<Config>) -> Result<Self, BuildError> {
        let role_sets = turnier_auth::RoleSets::from_config(&config);
        let oauth = turnier_auth::OAuthClient::new(&config);

        let broker = BrokerClient::from_config(&config);
        // Zwei billige Notifier-Instanzen: eine für den MatchManager, eine für den
        // direkten Routen-/Scheduler-Gebrauch. Beide teilen denselben Pool/Broker.
        let notifier_for_match = DiscordNotifier::new(broker.clone(), pool.clone(), &config);
        let notifier = DiscordNotifier::new(broker, pool.clone(), &config);

        let bridge = match SteamBridge::open(&config.steam_bridge_db_path).await {
            Ok(bridge) => bridge,
            Err(err) => {
                tracing::warn!(error = %err, "Steam-Bridge konnte nicht geöffnet werden — Steam-Tasks deaktiviert");
                None
            }
        };

        let match_manager = Arc::new(MatchManager::new(
            pool.clone(),
            Some(notifier_for_match),
            bridge,
            &config,
        ));

        let rank_resolver: Arc<dyn RankResolver> =
            Arc::new(turnier_steam::build_resolver(pool.clone(), &config).await?);

        Ok(Self {
            pool,
            config,
            role_sets,
            oauth,
            match_manager,
            rank_resolver,
            notifier: Arc::new(notifier),
        })
    }
}

/// Fehler beim Aufbau des `AppState` (Start-Zeit).
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// Der Rang-Resolver konnte nicht initialisiert werden.
    #[error(transparent)]
    Steam(#[from] turnier_steam::SteamError),
}
