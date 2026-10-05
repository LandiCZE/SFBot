use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::actions::Action;
use crate::game::StateSummary;

#[derive(Serialize, Clone)]
pub struct DecisionLog {
    pub ts: DateTime<Utc>,
    pub snapshot: Snapshot,
    pub action: Action,
    pub reason: String,
    pub executed: bool,
    pub result: String,
    pub invalid_reason: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct Snapshot {
    pub level: u16,
    pub silver: u64,
    pub mushrooms: u32,
    pub thirst_for_adventure_sec: u32,
    pub current_action: String,
    pub tavern_mode: &'static str,
}

impl Snapshot {
    pub fn from_state(state: &StateSummary) -> Self {
        Self {
            level: state.character.level,
            silver: state.character.silver,
            mushrooms: state.character.mushrooms,
            thirst_for_adventure_sec: state.tavern.thirst_for_adventure_sec,
            current_action: state.tavern.current_action.clone(),
            tavern_mode: state.tavern.mode,
        }
    }
}

pub fn append(path: &Path, entry: &DecisionLog) -> Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(entry)?;
    writeln!(f, "{line}")?;
    Ok(())
}
