use std::collections::VecDeque;
use std::io::Read;
use std::time::Duration;

use serde::Deserialize;

const DEMO_MAGIC: &[u8; 8] = b"PBDEMS2\0";
const FRAME_HEADER: usize = 10;
const CMD_STOP: u8 = 0;
const CMD_SYNC_TICK: u8 = 3;
const CMD_SEND_TABLES: u8 = 4;
const CMD_CLASS_INFO: u8 = 5;
const CMD_PACKET: u8 = 7;
const CMD_SIGNON_PACKET: u8 = 8;
const PACKET_DATA_TAG: u8 = 0x1a;
const MAX_FRAGMENT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum BroadcastError {
    #[error("Broadcast transport: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Broadcast antwortete mit {0}")]
    Status(reqwest::StatusCode),
    #[error("Broadcast-Fragment unlesbar: {0}")]
    Frame(String),
    #[error("Broadcast-Fragment zu groß")]
    TooLarge,
    #[error("Broadcast-Sync unlesbar: {0}")]
    Sync(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct BroadcastSync {
    pub tick: i64,
    #[serde(default)]
    pub endtick: Option<i64>,
    pub fragment: u64,
    pub signup_fragment: u64,
    #[serde(default = "default_tps")]
    pub tps: f64,
    #[serde(default)]
    pub rtdelay: Option<f64>,
    #[serde(default)]
    pub rcvage: Option<f64>,
}

fn default_tps() -> f64 {
    64.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentKind {
    Start,
    Full,
    Delta,
}

impl FragmentKind {
    fn path(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Full => "full",
            Self::Delta => "delta",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Fragment {
    pub bytes: Vec<u8>,
    pub wire_bytes: u64,
}

#[derive(Clone)]
pub struct BroadcastClient {
    http: reqwest::Client,
    base_url: String,
}

impl BroadcastClient {
    pub fn new(
        base_url: &str,
        network: &turnier_config::NetworkConfig,
    ) -> Result<Self, BroadcastError> {
        let http = reqwest::Client::builder()
            .no_proxy()
            .user_agent("DeutscheDeadlockCommunity-Observer/1.0")
            .connect_timeout(Duration::from_secs(network.observer_connect_seconds))
            .timeout(Duration::from_secs(network.observer_request_seconds))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn sync(&self) -> Result<BroadcastSync, BroadcastError> {
        let response = self
            .http
            .get(format!("{}/sync", self.base_url))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(BroadcastError::Status(response.status()));
        }
        let body = read_limited(response).await?;
        serde_json::from_slice(&body).map_err(|err| BroadcastError::Sync(err.to_string()))
    }

    pub async fn fragment(
        &self,
        number: u64,
        kind: FragmentKind,
    ) -> Result<Option<Fragment>, BroadcastError> {
        let response = self
            .http
            .get(format!("{}/{number}/{}", self.base_url, kind.path()))
            .send()
            .await?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(BroadcastError::Status(status));
        }
        let body = read_limited(response).await?;
        let wire_bytes = body.len() as u64;
        Ok(Some(Fragment {
            bytes: inflate(&body)?,
            wire_bytes,
        }))
    }
}

async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, BroadcastError> {
    if response.content_length().unwrap_or(0) > MAX_FRAGMENT_BYTES {
        return Err(BroadcastError::TooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if (body.len() + chunk.len()) as u64 > MAX_FRAGMENT_BYTES {
            return Err(BroadcastError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn inflate(body: &[u8]) -> Result<Vec<u8>, BroadcastError> {
    if !body.starts_with(&[0x1f, 0x8b]) {
        return Ok(body.to_vec());
    }
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(body)
        .take(MAX_FRAGMENT_BYTES * 4)
        .read_to_end(&mut out)
        .map_err(|err| BroadcastError::Frame(err.to_string()))?;
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    pub cmd: u8,
    pub tick: u32,
    pub body: &'a [u8],
}

pub fn frames(data: &[u8]) -> Result<Vec<Frame<'_>>, BroadcastError> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let header = data
            .get(offset..offset + FRAME_HEADER)
            .ok_or_else(|| BroadcastError::Frame(format!("Kopf bei {offset} abgeschnitten")))?;
        let tick = u32::from_le_bytes([header[1], header[2], header[3], header[4]]);
        let size = u32::from_le_bytes([header[6], header[7], header[8], header[9]]) as usize;
        let start = offset + FRAME_HEADER;
        let body = data
            .get(start..start + size)
            .ok_or_else(|| BroadcastError::Frame(format!("Nutzlast bei {offset} abgeschnitten")))?;
        out.push(Frame {
            cmd: header[0],
            tick,
            body,
        });
        offset = start + size;
    }
    Ok(out)
}

fn push_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn push_command(out: &mut Vec<u8>, cmd: u8, tick: u32, body: &[u8]) {
    push_varint(out, u64::from(cmd));
    push_varint(out, u64::from(tick));
    push_varint(out, body.len() as u64);
    out.extend_from_slice(body);
}

fn push_packet(out: &mut Vec<u8>, cmd: u8, tick: u32, data: &[u8]) {
    let mut wrapped = Vec::with_capacity(data.len() + 6);
    wrapped.push(PACKET_DATA_TAG);
    push_varint(&mut wrapped, data.len() as u64);
    wrapped.extend_from_slice(data);
    push_command(out, cmd, tick, &wrapped);
}

fn encode_signon(start: &[u8]) -> Result<Vec<u8>, BroadcastError> {
    let mut out = Vec::new();
    for frame in frames(start)? {
        match frame.cmd {
            CMD_SEND_TABLES | CMD_CLASS_INFO => {
                push_command(&mut out, frame.cmd, frame.tick, frame.body)
            }
            CMD_SIGNON_PACKET => push_packet(&mut out, CMD_SIGNON_PACKET, frame.tick, frame.body),
            _ => {}
        }
    }
    push_command(&mut out, CMD_SYNC_TICK, 0, &[]);
    Ok(out)
}

fn encode_packets(fragment: &[u8]) -> Result<(Vec<u8>, Option<u32>), BroadcastError> {
    let mut out = Vec::new();
    let mut last_tick = None;
    for frame in frames(fragment)? {
        if frame.cmd == CMD_PACKET {
            push_packet(&mut out, CMD_PACKET, frame.tick, frame.body);
            last_tick = Some(frame.tick);
        }
    }
    Ok((out, last_tick))
}

pub struct DemoWindow {
    signon: Vec<u8>,
    base: Vec<u8>,
    deltas: VecDeque<Vec<u8>>,
    max_deltas: usize,
    last_tick: Option<u32>,
}

impl DemoWindow {
    pub fn new(start: &[u8], max_deltas: usize) -> Result<Self, BroadcastError> {
        Ok(Self {
            signon: encode_signon(start)?,
            base: Vec::new(),
            deltas: VecDeque::new(),
            max_deltas: max_deltas.max(1),
            last_tick: None,
        })
    }

    pub fn rebase(&mut self, full: &[u8]) -> Result<(), BroadcastError> {
        let (base, tick) = encode_packets(full)?;
        self.base = base;
        self.deltas.clear();
        if tick.is_some() {
            self.last_tick = tick;
        }
        Ok(())
    }

    pub fn push_delta(&mut self, delta: &[u8]) -> Result<(), BroadcastError> {
        let (packets, tick) = encode_packets(delta)?;
        self.deltas.push_back(packets);
        if tick.is_some() {
            self.last_tick = tick;
        }
        Ok(())
    }

    pub fn is_full(&self) -> bool {
        self.deltas.len() >= self.max_deltas
    }

    pub fn has_base(&self) -> bool {
        !self.base.is_empty()
    }

    pub fn last_tick(&self) -> Option<u32> {
        self.last_tick
    }

    pub fn byte_len(&self) -> usize {
        self.signon.len() + self.base.len() + self.deltas.iter().map(Vec::len).sum::<usize>()
    }

    pub fn demo_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.byte_len() + 32);
        out.extend_from_slice(DEMO_MAGIC);
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&self.signon);
        out.extend_from_slice(&self.base);
        for delta in &self.deltas {
            out.extend_from_slice(delta);
        }
        push_command(&mut out, CMD_STOP, 0, &[]);
        out
    }
}
