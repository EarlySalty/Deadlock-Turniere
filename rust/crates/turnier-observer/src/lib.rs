//! Automatischer Ingame-Observer fuer Deadlock-Scrims.
//!
//! Die Domain ist bewusst vom Transport getrennt: Live-Daten werden zu stabilen
//! [`PlayerSnapshot`]s normalisiert, der [`Director`] entscheidet deterministisch,
//! und der lokale Agent kennt nur drei erlaubte Kameraaktionen.

pub mod director;
pub mod live;
pub mod protocol;
pub mod vconsole;

pub use director::{Director, DirectorConfig, DirectorDecision, MatchFrame, PlayerSnapshot, ScoreBreakdown};
pub use protocol::{AgentAck, AgentHeartbeat, CameraAction, CameraCommand, ObserverMode};
