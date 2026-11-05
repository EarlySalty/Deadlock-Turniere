use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::watch;

use crate::broadcast::{BroadcastClient, BroadcastError, BroadcastSync, DemoWindow, FragmentKind};
use crate::hud::{HudAccumulator, HudSnapshot, TICKS_PER_SECOND};
use crate::hud_decode::{decode_window, DecodedWindow};

const WINDOW_DELTAS: usize = 10;
const SAMPLE_TICKS: i64 = 16;
const NEWEST_PROBE_LIMIT: u64 = 30;

#[derive(Debug, Clone, Copy)]
pub struct FeedTiming {
    pub poll: Duration,
    pub stall: Duration,
}

impl FeedTiming {
    pub fn from_network(network: &turnier_config::NetworkConfig) -> Self {
        Self {
            poll: Duration::from_millis(network.hud_poll_millis),
            stall: Duration::from_secs(network.hud_stall_seconds),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedPhase {
    Starting,
    Live,
    Ended,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedStatus {
    pub match_id: u64,
    pub phase: FeedPhase,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub fragment: Option<u64>,
    pub wire_bytes: u64,
    pub bytes_per_second: Option<f64>,
    pub relay_delay_seconds: Option<f64>,
    pub hud_delay_seconds: Option<f64>,
    pub decode_millis: Option<u64>,
    pub window_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedState {
    pub status: FeedStatus,
    pub snapshot: Option<HudSnapshot>,
}

pub type SharedFeed = Arc<RwLock<FeedState>>;

pub fn new_shared_feed(match_id: u64) -> SharedFeed {
    Arc::new(RwLock::new(FeedState {
        status: FeedStatus {
            match_id,
            phase: FeedPhase::Starting,
            error: None,
            started_at: Utc::now(),
            updated_at: None,
            fragment: None,
            wire_bytes: 0,
            bytes_per_second: None,
            relay_delay_seconds: None,
            hud_delay_seconds: None,
            decode_millis: None,
            window_bytes: 0,
        },
        snapshot: None,
    }))
}

#[derive(Debug, thiserror::Error)]
enum FeedError {
    #[error(transparent)]
    Broadcast(#[from] BroadcastError),
    #[error("{0}")]
    Decode(String),
    #[error("Broadcast liefert keine neuen Fragmente mehr")]
    Stalled,
    #[error("Kein vollständiges Fragment gefunden")]
    NoKeyframe,
}

struct FeedRun {
    client: BroadcastClient,
    shared: SharedFeed,
    stop: watch::Receiver<bool>,
    timing: FeedTiming,
    accumulator: HudAccumulator,
    started: Instant,
    wire_bytes: u64,
}

pub async fn run_feed(
    client: BroadcastClient,
    shared: SharedFeed,
    stop: watch::Receiver<bool>,
    timing: FeedTiming,
) {
    let mut run = FeedRun {
        client,
        shared,
        stop,
        timing,
        accumulator: HudAccumulator::default(),
        started: Instant::now(),
        wire_bytes: 0,
    };
    let outcome = run.run().await;
    let phase = match &outcome {
        Ok(phase) => *phase,
        Err(_) => FeedPhase::Failed,
    };
    if let Err(err) = &outcome {
        tracing::warn!(error = %err, "HUD-Feed beendet mit Fehler");
    }
    run.update(|status| {
        status.phase = phase;
        status.error = outcome.err().map(|err| err.to_string());
    });
}

impl FeedRun {
    async fn run(&mut self) -> Result<FeedPhase, FeedError> {
        let sync = self.client.sync().await?;
        let start = self
            .fetch(sync.signup_fragment, FragmentKind::Start)
            .await?
            .ok_or(FeedError::NoKeyframe)?;
        let mut window = DemoWindow::new(&start, WINDOW_DELTAS)?;
        let mut next = self.newest_full(&sync, &mut window).await?;
        let mut last_progress = Instant::now();
        while !*self.stop.borrow() {
            if window.is_full() {
                match self.fetch_live(next, FragmentKind::Full).await? {
                    Some(full) => window.rebase(&full)?,
                    None => {
                        if self.wait_or_stop(&mut last_progress).await? {
                            return Ok(FeedPhase::Stopped);
                        }
                        continue;
                    }
                }
            }
            let Some(delta) = self.fetch_live(next, FragmentKind::Delta).await? else {
                if self.wait_or_stop(&mut last_progress).await? {
                    return Ok(FeedPhase::Stopped);
                }
                continue;
            };
            window.push_delta(&delta)?;
            last_progress = Instant::now();
            self.decode(&window, next).await?;
            if self.accumulator.is_finished() {
                return Ok(FeedPhase::Ended);
            }
            next += 1;
        }
        Ok(FeedPhase::Stopped)
    }

    async fn newest_full(
        &mut self,
        sync: &BroadcastSync,
        window: &mut DemoWindow,
    ) -> Result<u64, FeedError> {
        for number in (sync.fragment..=sync.fragment + NEWEST_PROBE_LIMIT).rev() {
            if let Some(full) = self.fetch(number, FragmentKind::Full).await? {
                window.rebase(&full)?;
                return Ok(number);
            }
        }
        Err(FeedError::NoKeyframe)
    }

    async fn fetch(
        &mut self,
        number: u64,
        kind: FragmentKind,
    ) -> Result<Option<Vec<u8>>, FeedError> {
        let fragment = self.client.fragment(number, kind).await?;
        Ok(fragment.map(|fragment| {
            self.wire_bytes += fragment.wire_bytes;
            fragment.bytes
        }))
    }

    async fn fetch_live(
        &mut self,
        number: u64,
        kind: FragmentKind,
    ) -> Result<Option<Vec<u8>>, FeedError> {
        match self.fetch(number, kind).await {
            Err(FeedError::Broadcast(err)) if transient(&err) => {
                tracing::warn!(error = %err, fragment = number, "HUD-Feed: Relay-Fehler, neuer Versuch");
                let message = err.to_string();
                self.update(|status| status.error = Some(message));
                Ok(None)
            }
            other => other,
        }
    }

    async fn wait_or_stop(&mut self, last_progress: &mut Instant) -> Result<bool, FeedError> {
        if last_progress.elapsed() > self.timing.stall {
            return Err(FeedError::Stalled);
        }
        let _ = tokio::time::timeout(self.timing.poll, self.stop.changed()).await;
        Ok(*self.stop.borrow())
    }

    async fn decode(&mut self, window: &DemoWindow, fragment: u64) -> Result<(), FeedError> {
        let demo = window.demo_bytes();
        let after = self.accumulator.last_tick();
        let begun = Instant::now();
        let decoded: DecodedWindow =
            tokio::task::spawn_blocking(move || decode_window(demo, after, SAMPLE_TICKS))
                .await
                .map_err(|err| FeedError::Decode(err.to_string()))?
                .map_err(|err| FeedError::Decode(err.to_string()))?;
        let decode_millis = begun.elapsed().as_millis() as u64;
        for tick in &decoded.ticks {
            self.accumulator.apply_tick(tick);
        }
        for kill in &decoded.boss_kills {
            self.accumulator.apply_boss_kill(kill);
        }
        let sync = self.client.sync().await.ok();
        let snapshot = self.accumulator.snapshot();
        let window_bytes = window.byte_len();
        let elapsed = self.started.elapsed().as_secs_f64();
        let wire_bytes = self.wire_bytes;
        let hud_tick = snapshot.tick;
        self.update(|status| {
            status.phase = FeedPhase::Live;
            status.updated_at = Some(Utc::now());
            status.fragment = Some(fragment);
            status.wire_bytes = wire_bytes;
            status.bytes_per_second = (elapsed > 0.0).then(|| wire_bytes as f64 / elapsed);
            status.relay_delay_seconds = sync.as_ref().and_then(|sync| sync.rtdelay);
            status.hud_delay_seconds = sync.as_ref().and_then(|sync| hud_delay(sync, hud_tick));
            status.decode_millis = Some(decode_millis);
            status.window_bytes = window_bytes;
            status.error = None;
        });
        if let Ok(mut state) = self.shared.write() {
            state.snapshot = Some(snapshot);
        }
        Ok(())
    }

    fn update(&self, apply: impl FnOnce(&mut FeedStatus)) {
        if let Ok(mut state) = self.shared.write() {
            apply(&mut state.status);
        }
    }
}

fn transient(err: &BroadcastError) -> bool {
    match err {
        BroadcastError::Http(_) => true,
        BroadcastError::Status(code) => {
            code.is_server_error() || *code == reqwest::StatusCode::TOO_MANY_REQUESTS
        }
        _ => false,
    }
}

fn hud_delay(sync: &BroadcastSync, hud_tick: Option<i64>) -> Option<f64> {
    let relay_tick = sync.endtick.unwrap_or(sync.tick);
    let tps = if sync.tps > 0.0 {
        sync.tps
    } else {
        TICKS_PER_SECOND
    };
    let behind_relay = (relay_tick - hud_tick?) as f64 / tps;
    Some(sync.rtdelay.unwrap_or(0.0) + sync.rcvage.unwrap_or(0.0) + behind_relay.max(0.0))
}
