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
const READY_BACKOFF_FACTOR: u32 = 8;

#[derive(Debug, Clone, Copy)]
pub struct FeedTiming {
    pub poll: Duration,
    pub stall: Duration,
    pub ready: Duration,
}

impl FeedTiming {
    pub fn from_network(network: &turnier_config::NetworkConfig) -> Self {
        Self {
            poll: Duration::from_millis(network.hud_poll_millis),
            stall: Duration::from_secs(network.hud_stall_seconds),
            ready: Duration::from_secs(network.hud_ready_seconds),
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
        let Some(sync) = self.ready_sync().await? else {
            return Ok(FeedPhase::Stopped);
        };
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

    async fn ready_sync(&mut self) -> Result<Option<BroadcastSync>, FeedError> {
        let deadline = Instant::now() + self.timing.ready;
        let longest = self.timing.poll * READY_BACKOFF_FACTOR;
        let mut wait = self.timing.poll;
        loop {
            match self.client.sync().await {
                Err(BroadcastError::Status(code)) if not_ready(code) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(BroadcastError::Status(code).into());
                    }
                    tracing::info!(status = %code, "HUD-Feed: Broadcast noch nicht bereit");
                    self.update(|status| {
                        status.error = Some(format!("Broadcast noch nicht bereit ({code})"))
                    });
                    let _ = tokio::time::timeout(wait.min(remaining), self.stop.changed()).await;
                    if *self.stop.borrow() {
                        return Ok(None);
                    }
                    wait = (wait * 2).min(longest);
                }
                other => return Ok(Some(other?)),
            }
        }
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

fn not_ready(code: reqwest::StatusCode) -> bool {
    code == reqwest::StatusCode::METHOD_NOT_ALLOWED || code == reqwest::StatusCode::NOT_FOUND
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{TcpListener, TcpStream};

    const NOT_READY: &str = "HTTP/1.1 405 Method Not Allowed\r\ncontent-type: application/json\r\ncontent-length: 50\r\nconnection: close\r\n\r\n{\"statusCode\":405,\"message\":\"Broadcast not ready\"}";
    const FORBIDDEN: &str =
        "HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
    const READY: &str = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 46\r\nconnection: close\r\n\r\n{\"tick\":100,\"fragment\":12,\"signup_fragment\":3}";

    async fn answer(stream: TcpStream, response: &'static str) {
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            if stream.readable().await.is_err() {
                return;
            }
            match stream.try_read(&mut buf) {
                Ok(0) => return,
                Ok(n) => request.extend_from_slice(&buf[..n]),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => return,
            }
        }
        let mut sent = 0;
        let bytes = response.as_bytes();
        while sent < bytes.len() {
            if stream.writable().await.is_err() {
                return;
            }
            match stream.try_write(&bytes[sent..]) {
                Ok(n) => sent += n,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => return,
            }
        }
    }

    async fn relay(responses: Vec<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            for response in responses {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                answer(stream, response).await;
            }
        });
        base
    }

    fn feed_run(base: &str, ready: Duration) -> (FeedRun, watch::Sender<bool>) {
        let (stop_tx, stop) = watch::channel(false);
        let client = BroadcastClient::new(base, &turnier_config::NetworkConfig::default()).unwrap();
        let run = FeedRun {
            client,
            shared: new_shared_feed(1),
            stop,
            timing: FeedTiming {
                poll: Duration::from_millis(10),
                stall: Duration::from_secs(5),
                ready,
            },
            accumulator: HudAccumulator::default(),
            started: Instant::now(),
            wire_bytes: 0,
        };
        (run, stop_tx)
    }

    #[tokio::test]
    async fn sync_wartet_bis_broadcast_bereit_ist() {
        let base = relay(vec![NOT_READY, NOT_READY, READY]).await;
        let (mut run, _stop) = feed_run(&base, Duration::from_secs(5));
        let sync = run.ready_sync().await.unwrap().unwrap();
        assert_eq!(sync.fragment, 12);
        assert_eq!(run.shared.read().unwrap().status.phase, FeedPhase::Starting);
    }

    #[tokio::test]
    async fn sync_gibt_nach_frist_auf() {
        let base = relay(vec![NOT_READY; 20]).await;
        let (mut run, _stop) = feed_run(&base, Duration::from_millis(50));
        let err = run.ready_sync().await.unwrap_err();
        assert!(matches!(
            err,
            FeedError::Broadcast(BroadcastError::Status(code)) if code == reqwest::StatusCode::METHOD_NOT_ALLOWED
        ));
    }

    #[tokio::test]
    async fn anderer_sync_fehler_bleibt_fatal() {
        let base = relay(vec![FORBIDDEN, READY]).await;
        let (mut run, _stop) = feed_run(&base, Duration::from_secs(5));
        let err = run.ready_sync().await.unwrap_err();
        assert!(matches!(
            err,
            FeedError::Broadcast(BroadcastError::Status(code)) if code == reqwest::StatusCode::FORBIDDEN
        ));
    }
}
