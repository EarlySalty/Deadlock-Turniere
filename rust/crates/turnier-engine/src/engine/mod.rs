//! Reine, DB-freie Bracket-Algorithmen.
//!
//! Diese Untermodule kapseln die gesamte Verdrahtungs-Mathematik der Engine —
//! Slot-Modell, Seed-Reihenfolge, Slot-Verteilung, Snake-Draft, Gruppen-Anzahl,
//! Round-Robin und die Double-Elimination-Rundengrößen/Drop-Ziele. Keine Funktion
//! hier berührt eine Datenbank; alle sind per Unit-Test gegen das Python-Original
//! gepinnt. Die Persistenz (`crate::persist`) ruft sie und vergibt die echten
//! Match-IDs.

pub mod double_elim;
pub mod groups;
pub mod naming;
pub mod seeding;
pub mod slots;

pub use slots::BracketSlot;
