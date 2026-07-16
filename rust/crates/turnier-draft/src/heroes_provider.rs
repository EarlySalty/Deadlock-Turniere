//! Live-Heldenliste mit 24-Stunden-Prozesscache und statischem Fallback.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::heroes::DEADLOCK_HEROES;

const HEROES_URL: &str = "https://api.deadlock-api.com/v1/assets/heroes";
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Für Drafts benötigte Helden-Stammdaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hero {
    pub id: u32,
    pub name: String,
    pub image_url: String,
}

/// Injizierbare Quelle für Live-Helden. Tests liefern hier keinen HTTP-Client.
pub trait HeroFetcher: Send + Sync {
    fn fetch(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Hero>, String>> + Send + '_>>;
}

/// Prozesslokaler TTL-Cache um eine injizierte Heldenquelle.
pub struct HeroesProvider<F> {
    fetcher: F,
    ttl: Duration,
    cache: Mutex<Option<(Instant, Vec<Hero>)>>,
}

impl<F: HeroFetcher> HeroesProvider<F> {
    pub fn new(fetcher: F, ttl: Duration) -> Self {
        Self {
            fetcher,
            ttl,
            cache: Mutex::new(None),
        }
    }

    /// Lädt spielbare Live-Helden oder liefert bei jedem Fehler die statische Liste.
    pub async fn heroes(&self) -> Vec<Hero> {
        if let Some(heroes) = self.cached() {
            return heroes;
        }

        let heroes = match self.fetcher.fetch().await {
            Ok(heroes) if !heroes.is_empty() => heroes,
            Ok(_) | Err(_) => static_heroes(),
        };
        *self.cache_guard() = Some((Instant::now(), heroes.clone()));
        heroes
    }

    /// Gibt nur einen bereits geladenen, noch frischen Cache zurück.
    pub fn cached(&self) -> Option<Vec<Hero>> {
        self.cache_guard()
            .as_ref()
            .filter(|(loaded_at, _)| loaded_at.elapsed() < self.ttl)
            .map(|(_, heroes)| heroes.clone())
    }

    /// Prüft exakt gegen die von diesem Provider geladene Liste.
    pub async fn is_valid_hero(&self, name: &str) -> bool {
        self.heroes().await.iter().any(|hero| hero.name == name)
    }

    fn cache_guard(&self) -> MutexGuard<'_, Option<(Instant, Vec<Hero>)>> {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Produktive HTTP-Quelle der Deadlock Assets API.
pub struct ReqwestHeroFetcher {
    client: reqwest::Client,
}

impl Default for ReqwestHeroFetcher {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl HeroFetcher for ReqwestHeroFetcher {
    fn fetch(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Hero>, String>> + Send + '_>> {
        Box::pin(async move {
            let response = self
                .client
                .get(HEROES_URL)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|error| error.to_string())?;
            let heroes = response
                .json::<Vec<ApiHero>>()
                .await
                .map_err(|error| error.to_string())?;
            Ok(heroes
                .into_iter()
                .filter(|hero| hero.player_selectable && !hero.disabled)
                .map(Hero::from)
                .collect())
        })
    }
}

#[derive(Deserialize)]
struct ApiHero {
    id: u32,
    name: String,
    player_selectable: bool,
    disabled: bool,
    #[serde(default)]
    images: ApiHeroImages,
}

#[derive(Default, Deserialize)]
struct ApiHeroImages {
    icon_image_small_webp: Option<String>,
    icon_image_small: Option<String>,
    icon_hero_card_webp: Option<String>,
}

impl From<ApiHero> for Hero {
    fn from(hero: ApiHero) -> Self {
        Self {
            id: hero.id,
            name: hero.name,
            image_url: hero
                .images
                .icon_image_small_webp
                .or(hero.images.icon_image_small)
                .or(hero.images.icon_hero_card_webp)
                .unwrap_or_default(),
        }
    }
}

fn static_heroes() -> Vec<Hero> {
    DEADLOCK_HEROES
        .iter()
        .enumerate()
        .map(|(index, name)| Hero {
            id: index as u32 + 1,
            name: (*name).to_string(),
            image_url: String::new(),
        })
        .collect()
}

pub(crate) static DEFAULT_PROVIDER: Lazy<HeroesProvider<ReqwestHeroFetcher>> =
    Lazy::new(|| HeroesProvider::new(ReqwestHeroFetcher::default(), CACHE_TTL));

/// Liefert die gecachte Live-Liste beziehungsweise den statischen Fallback.
pub async fn load_heroes() -> Vec<Hero> {
    DEFAULT_PROVIDER.heroes().await
}
