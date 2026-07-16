use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use turnier_draft::{Hero, HeroFetcher, HeroesProvider, DEADLOCK_HEROES};

#[derive(Clone)]
struct FakeFetcher {
    calls: Arc<AtomicUsize>,
    result: Result<Vec<Hero>, String>,
}

impl HeroFetcher for FakeFetcher {
    fn fetch(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Hero>, String>> + Send + '_>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = self.result.clone();
        Box::pin(async move { result })
    }
}

#[tokio::test]
async fn fetch_fehler_faellt_auf_statische_liste_zurueck() {
    let fetcher = FakeFetcher {
        calls: Arc::new(AtomicUsize::new(0)),
        result: Err("nicht erreichbar".to_string()),
    };
    let provider = HeroesProvider::new(fetcher, Duration::from_secs(24 * 60 * 60));

    let heroes = provider.heroes().await;

    assert_eq!(heroes.len(), DEADLOCK_HEROES.len());
    assert_eq!(heroes[0].name, DEADLOCK_HEROES[0]);
}

#[tokio::test]
async fn zweiter_aufruf_kommt_aus_dem_cache() {
    let calls = Arc::new(AtomicUsize::new(0));
    let fetcher = FakeFetcher {
        calls: Arc::clone(&calls),
        result: Ok(vec![Hero {
            id: 7,
            name: "Testheld".to_string(),
            image_url: "https://example.invalid/test.webp".to_string(),
        }]),
    };
    let provider = HeroesProvider::new(fetcher, Duration::from_secs(24 * 60 * 60));

    assert_eq!(provider.heroes().await[0].name, "Testheld");
    assert_eq!(provider.heroes().await[0].name, "Testheld");
    assert!(provider.is_valid_hero("Testheld").await);
    assert!(!provider.is_valid_hero("testheld").await);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
