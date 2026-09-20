//! Live-Heldenliste mit 24-Stunden-Prozesscache und kurz gecachtem statischem Fallback.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::heroes::DEADLOCK_HEROES;

const HEROES_URL: &str = "https://api.deadlock-api.com/v1/assets/heroes";
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const FALLBACK_CACHE_TTL: Duration = Duration::from_secs(60);

/// Für Drafts benötigte Helden-Stammdaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hero {
    pub id: u32,
    pub name: String,
    pub image_url: String,
    pub card_image_url: String,
}

/// Injizierbare Quelle für Live-Helden. Tests liefern hier keinen HTTP-Client.
pub trait HeroFetcher: Send + Sync {
    fn fetch(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Hero>, String>> + Send + '_>>;
}

/// Prozesslokaler TTL-Cache um eine injizierte Heldenquelle.
pub struct HeroesProvider<F> {
    fetcher: F,
    ttl: Duration,
    fallback_ttl: Duration,
    cache: Mutex<Option<(Instant, Duration, Vec<Hero>)>>,
}

impl<F: HeroFetcher> HeroesProvider<F> {
    pub fn new(fetcher: F, ttl: Duration, fallback_ttl: Duration) -> Self {
        Self {
            fetcher,
            ttl,
            fallback_ttl,
            cache: Mutex::new(None),
        }
    }

    /// Lädt spielbare Live-Helden oder liefert bei jedem Fehler die statische Liste.
    pub async fn heroes(&self) -> Vec<Hero> {
        if let Some(heroes) = self.cached() {
            return heroes;
        }

        let (heroes, ttl) = match self.fetcher.fetch().await {
            Ok(heroes) if !heroes.is_empty() => (heroes, self.ttl),
            Ok(_) | Err(_) => (static_heroes(), self.fallback_ttl),
        };
        *self.cache_guard() = Some((Instant::now(), ttl, heroes.clone()));
        heroes
    }

    /// Gibt nur einen bereits geladenen, noch frischen Cache zurück.
    pub fn cached(&self) -> Option<Vec<Hero>> {
        self.cache_guard()
            .as_ref()
            .filter(|(loaded_at, ttl, _)| loaded_at.elapsed() < *ttl)
            .map(|(_, _, heroes)| heroes.clone())
    }

    /// Prüft exakt gegen die von diesem Provider geladene Liste.
    pub async fn is_valid_hero(&self, name: &str) -> bool {
        self.heroes().await.iter().any(|hero| hero.name == name)
    }

    fn cache_guard(&self) -> MutexGuard<'_, Option<(Instant, Duration, Vec<Hero>)>> {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Produktive HTTP-Quelle der Deadlock Assets API.
pub struct ReqwestHeroFetcher {
    client: reqwest::Client,
    url: String,
}

impl ReqwestHeroFetcher {
    pub fn new(url: String, timeout_seconds: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .expect("HTTP-Client für geprüfte Heldenquelle");
        Self { client, url }
    }
}
impl Default for ReqwestHeroFetcher {
    fn default() -> Self {
        Self::new(HEROES_URL.to_owned(), 5)
    }
}

impl HeroFetcher for ReqwestHeroFetcher {
    fn fetch(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Hero>, String>> + Send + '_>> {
        Box::pin(async move {
            let response = self
                .client
                .get(&self.url)
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
        let card_image_url = hero.images.icon_hero_card_webp.clone().unwrap_or_default();
        let image_url = hero
            .images
            .icon_image_small_webp
            .or(hero.images.icon_image_small)
            .or(hero.images.icon_hero_card_webp)
            .unwrap_or_default();
        Self {
            id: hero.id,
            name: hero.name,
            image_url,
            card_image_url,
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
            card_image_url: String::new(),
        })
        .collect()
}

pub(crate) static DEFAULT_PROVIDER: Lazy<HeroesProvider<ReqwestHeroFetcher>> =
    Lazy::new(|| HeroesProvider::new(ReqwestHeroFetcher::default(), CACHE_TTL, FALLBACK_CACHE_TTL));

/// Liefert die gecachte Live-Liste beziehungsweise den statischen Fallback.
pub async fn load_heroes() -> Vec<Hero> {
    DEFAULT_PROVIDER.heroes().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_held_liefert_portrait_und_splash() {
        let api_hero = ApiHero {
            id: 12,
            name: "Beispielsheld".to_string(),
            player_selectable: true,
            disabled: false,
            images: ApiHeroImages {
                icon_image_small_webp: Some("https://example.invalid/sm.webp".to_string()),
                icon_image_small: None,
                icon_hero_card_webp: Some("https://example.invalid/card.webp".to_string()),
            },
        };
        let hero = Hero::from(api_hero);
        assert_eq!(hero.image_url, "https://example.invalid/sm.webp");
        assert_eq!(hero.card_image_url, "https://example.invalid/card.webp");
    }

    #[test]
    fn api_held_ohne_splash_liefert_leere_card_url() {
        let api_hero = ApiHero {
            id: 13,
            name: "Schlichtheld".to_string(),
            player_selectable: true,
            disabled: false,
            images: ApiHeroImages {
                icon_image_small_webp: Some("https://example.invalid/sm.webp".to_string()),
                icon_image_small: None,
                icon_hero_card_webp: None,
            },
        };
        let hero = Hero::from(api_hero);
        assert_eq!(hero.card_image_url, "");
    }
}
