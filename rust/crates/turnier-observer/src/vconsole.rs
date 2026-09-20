use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex};

use crate::protocol::CameraAction;

const HEADER_SIZE: usize = 12;
const CMND_VERSION: u32 = 0x00D4_0000;
const MAX_PACKET: usize = u16::MAX as usize;
const COMMAND_ACK_TIMEOUT: Duration =
    Duration::from_millis(turnier_config::VCONSOLE_ACK_MILLISECONDS);
static COMMAND_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, thiserror::Error)]
pub enum VConsoleError {
    #[error("VConsole transport: {0}")]
    Io(#[from] io::Error),
    #[error("VConsole packet ungueltig: {0}")]
    Protocol(String),
    #[error("VConsole command abgelehnt: {0}")]
    CommandRejected(String),
    #[error("VConsole command acknowledgement timeout")]
    AckTimeout,
}

#[derive(Clone)]
pub struct VConsoleClient {
    writer: Arc<Mutex<OwnedWriteHalf>>,
    prints: broadcast::Sender<String>,
    ack_timeout: Duration,
}

impl VConsoleClient {
    pub async fn connect(addr: &str) -> Result<Self, VConsoleError> {
        Self::with_ack_timeout(addr, COMMAND_ACK_TIMEOUT).await
    }

    pub async fn with_ack_timeout(
        addr: &str,
        ack_timeout: Duration,
    ) -> Result<Self, VConsoleError> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        let (reader, writer) = stream.into_split();
        let (prints, _) = broadcast::channel(512);
        let reader_prints = prints.clone();
        tokio::spawn(async move {
            if let Err(err) = drain_packets(reader, reader_prints).await {
                tracing::warn!(error = %err, "VConsole Leseschleife beendet");
            }
        });
        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            prints,
            ack_timeout,
        })
    }

    async fn send_command(&self, command: &str) -> Result<(), VConsoleError> {
        let payload = build_command_packet(command)?;
        self.writer.lock().await.write_all(&payload).await?;
        Ok(())
    }

    /// Sendet einen Befehl zwischen zwei eindeutigen `echo`-Markern und sammelt
    /// die dazwischen liegenden PRNT-Zeilen. Das ist kein echtes RPC-Ack, aber
    /// es beweist, dass der aktuelle Deadlock-Prozess den Befehlstrom verarbeitet
    /// und erlaubt, sichtbare "unknown/development only"-Ablehnungen fail-closed
    /// zu behandeln.
    pub async fn send_checked(&self, command: &str) -> Result<Vec<String>, VConsoleError> {
        let seq = COMMAND_SEQ.fetch_add(1, Ordering::Relaxed);
        let begin = format!("__ddl_observer_begin_{seq}__");
        let end = format!("__ddl_observer_end_{seq}__");
        let mut rx = self.prints.subscribe();

        self.send_command(&format!("echo {begin}")).await?;
        self.send_command(command).await?;
        self.send_command(&format!("echo {end}")).await?;

        let collect = async {
            let mut in_window = false;
            let mut lines = Vec::new();
            loop {
                match rx.recv().await {
                    Ok(line) => {
                        if line.contains(&begin) {
                            in_window = true;
                            continue;
                        }
                        if line.contains(&end) {
                            return Ok::<_, VConsoleError>(lines);
                        }
                        if in_window {
                            lines.push(line);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        return Err(VConsoleError::Protocol(
                            "PRNT buffer lagged while waiting for command acknowledgement".into(),
                        ));
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(VConsoleError::Protocol(
                            "PRNT stream closed while waiting for command acknowledgement".into(),
                        ));
                    }
                }
            }
        };
        let lines = tokio::time::timeout(self.ack_timeout, collect)
            .await
            .map_err(|_| VConsoleError::AckTimeout)??;
        reject_visible_command_error(command, &lines)?;
        Ok(lines)
    }

    /// Führt ausschließlich die Observer-Allowlist aus. Es gibt bewusst keinen
    /// API-Pfad, der beliebige Console-Kommandos vom Server entgegennimmt.
    pub async fn apply_camera_action(&self, action: &CameraAction) -> Result<(), VConsoleError> {
        match action {
            CameraAction::SpectateLobby { lobby_id } => {
                self.send_checked(&format!("citadel_spectate_lobby_id {lobby_id}"))
                    .await?;
            }
            CameraAction::Directed => {
                self.send_checked("citadel_spec_lock_to_accountid 0")
                    .await?;
                self.send_checked("citadel_spectator_mode 0").await?;
                self.send_checked("spec_autodirector 1").await?;
            }
            CameraAction::HeroChase { account_id } => {
                self.send_checked("spec_autodirector 0").await?;
                self.send_checked("citadel_spectator_mode 2").await?;
                self.send_checked(&format!("citadel_spec_lock_to_accountid {account_id}"))
                    .await?;
            }
            CameraAction::PlayerView { account_id } => {
                self.send_checked("spec_autodirector 0").await?;
                self.send_checked("citadel_spectator_mode 3").await?;
                self.send_checked(&format!("citadel_spec_lock_to_accountid {account_id}"))
                    .await?;
            }
        }
        Ok(())
    }

    /// Best-effort Nachweis, ob der lokale Client tatsächlich in einer Session
    /// hängt. `status` ist ein normaler Client-Befehl; wenn Valve dessen Ausgabe
    /// verändert, liefert diese Probe false statt Auto voreilig freizuschalten.
    pub async fn probe_game_connected(&self) -> Result<bool, VConsoleError> {
        let lines = self.send_checked("status").await?;
        Ok(status_looks_connected(&lines))
    }
}

pub fn build_command_packet(command: &str) -> Result<Vec<u8>, VConsoleError> {
    let bytes = command.as_bytes();
    let total = HEADER_SIZE + bytes.len() + 1;
    if total > MAX_PACKET {
        return Err(VConsoleError::Protocol("command too long".to_string()));
    }
    let mut packet = Vec::with_capacity(total);
    packet.extend_from_slice(b"CMND");
    packet.extend_from_slice(&CMND_VERSION.to_be_bytes());
    packet.extend_from_slice(&(total as u16).to_be_bytes());
    packet.extend_from_slice(&0_u16.to_be_bytes());
    packet.extend_from_slice(bytes);
    packet.push(0);
    Ok(packet)
}

fn reject_visible_command_error(command: &str, lines: &[String]) -> Result<(), VConsoleError> {
    let joined = lines.join("\n").to_ascii_lowercase();
    let rejected = [
        "unknown command",
        "unrecognized command",
        "development only",
        "not allowed",
        "cannot execute",
        "command is restricted",
    ]
    .iter()
    .any(|needle| joined.contains(needle));
    if rejected {
        let detail = lines
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(VConsoleError::CommandRejected(format!(
            "{command}: {detail}"
        )));
    }
    Ok(())
}

fn status_looks_connected(lines: &[String]) -> bool {
    let joined = lines.join("\n").to_ascii_lowercase();
    if joined.contains("not connected")
        || joined.contains("disconnected")
        || joined.contains("no server")
    {
        return false;
    }
    let has_map = joined.contains("map") || joined.contains("level");
    let has_server = joined.contains("server") || joined.contains("hostname");
    let has_player_context = joined.contains("players") || joined.contains("userid");
    has_map && (has_server || has_player_context)
}

async fn drain_packets(
    mut reader: OwnedReadHalf,
    prints: broadcast::Sender<String>,
) -> Result<(), VConsoleError> {
    loop {
        let mut header = [0_u8; HEADER_SIZE];
        reader.read_exact(&mut header).await?;
        let length = u16::from_be_bytes([header[8], header[9]]) as usize;
        if !(HEADER_SIZE..=MAX_PACKET).contains(&length) {
            return Err(VConsoleError::Protocol(format!(
                "invalid inbound packet length {length}"
            )));
        }
        let body_len = length - HEADER_SIZE;
        let mut body = vec![0_u8; body_len];
        if body_len > 0 {
            reader.read_exact(&mut body).await?;
        }
        if &header[..4] == b"PRNT" {
            if let Some(message) = parse_prnt_message(&body) {
                let _ = prints.send(message);
            }
        }
    }
}

/// Aktuelle VConsole2-PRNT-Struktur: Channel-ID am Anfang, Nachricht ab Body
/// Offset 28 als NUL-terminierter C-String. Nicht-ASCII-Steuerbytes werden wie
/// in der aktuellen Referenz verworfen.
fn parse_prnt_message(body: &[u8]) -> Option<String> {
    if body.len() < 28 {
        return None;
    }
    let message = &body[28..];
    let end = message
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(message.len());
    let filtered = message[..end]
        .iter()
        .copied()
        .filter(|byte| *byte <= 0x7f)
        .collect::<Vec<_>>();
    Some(String::from_utf8_lossy(&filtered).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmnd_packet_uses_modern_deadlock_version_and_nul() {
        let packet = build_command_packet("echo observer").unwrap();
        assert_eq!(&packet[..4], b"CMND");
        assert_eq!(&packet[4..8], &[0x00, 0xD4, 0x00, 0x00]);
        assert_eq!(
            u16::from_be_bytes([packet[8], packet[9]]) as usize,
            packet.len()
        );
        assert_eq!(&packet[10..12], &[0, 0]);
        assert_eq!(packet.last(), Some(&0));
    }

    #[test]
    fn prnt_message_starts_at_body_offset_28() {
        let mut body = vec![0_u8; 28];
        body.extend_from_slice(b"hello observer\0trailing");
        assert_eq!(parse_prnt_message(&body).as_deref(), Some("hello observer"));
    }

    #[test]
    fn visible_unknown_command_is_rejected() {
        let lines = vec!["Unknown command: citadel_old_thing".to_string()];
        assert!(reject_visible_command_error("citadel_old_thing", &lines).is_err());
    }

    #[test]
    fn status_probe_is_fail_closed() {
        assert!(!status_looks_connected(&["Not connected to server".into()]));
        assert!(!status_looks_connected(&["something changed".into()]));
        assert!(status_looks_connected(&[
            "hostname: Deadlock".into(),
            "map: citadel".into(),
            "players: 13 humans".into(),
        ]));
    }
}
