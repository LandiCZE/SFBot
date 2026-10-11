//! Deterministic expedition decision-making. Pure scoring functions over the
//! state surfaced in `game::ActiveExpeditionSummary` / `TavernSummary`. No
//! Claude calls, no I/O. Weights are compile-time constants — easy to tune,
//! but there is no runtime config file (yet). If weights become a user
//! concern, lift these into an `expedition_policy.toml` loaded at startup.

use crate::game::{
    ActiveExpeditionSummary, ExpeditionRewardBrief, ExpeditionSummary, StateSummary,
};

/// Weighting used by all pickers below. Deliberately a module-level constant;
/// the user explicitly prefers deterministic code over configuration surface
/// area for subsystems whose math is clear.
#[derive(Clone, Copy)]
pub struct Weights {
    pub heroism: f32,
    /// Bonus added when the encounter closes a slot of the target pickup
    /// (the item we are nominally collecting on this run).
    pub closes_target: f32,
    /// Bonus added when the encounter token is itself a bounty item —
    /// picking it unlocks the +10 heroism bonus for the matching collectibles
    /// later in the run.
    pub picks_bounty: f32,
    /// Multiplier applied to the "special" tag on an AvailableExpedition.
    pub egg_bonus: f32,
    pub daily_task_bonus: f32,
    /// Scoring factors for post-boss reward picks. All amounts are in the
    /// corresponding native unit (silver, xp, mushrooms, count), so these
    /// multipliers are "value per unit".
    pub reward_silver_per_unit: f32,
    pub reward_xp_per_unit: f32,
    pub reward_mushroom_per_unit: f32,
    pub reward_fruit_per_unit: f32,
    pub reward_resource_per_unit: f32,
    pub reward_other_fallback: f32,
}

pub const DEFAULT_WEIGHTS: Weights = Weights {
    heroism: 1.0,
    closes_target: 15.0,
    picks_bounty: 10.0,
    egg_bonus: 50.0,
    daily_task_bonus: 80.0,
    reward_silver_per_unit: 1.0,
    reward_xp_per_unit: 2.0,
    reward_mushroom_per_unit: 500.0, // mushrooms are rare; weight accordingly
    reward_fruit_per_unit: 20.0,
    reward_resource_per_unit: 3.0,
    reward_other_fallback: 1.0,
};

/// Choose which of the up-to-two available expeditions to start. Falls back
/// to the shortest-duration option when scores tie or no options have specials.
pub fn pick_start(available: &[ExpeditionSummary], w: &Weights) -> Option<u8> {
    if available.is_empty() {
        return None;
    }
    let scored: Vec<(u8, f32)> = available
        .iter()
        .map(|e| {
            let special_score = match e.special.as_deref() {
                Some("DailyTask") => w.daily_task_bonus,
                Some("Egg") => w.egg_bonus,
                _ => 0.0,
            };
            // Shorter ALU cost is a tiebreak — small negative weight on duration.
            let duration_penalty = (e.thirst_for_adventure_sec as f32) * 0.001;
            (e.index, special_score - duration_penalty)
        })
        .collect();
    scored
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx)
}

pub fn pick_encounter(exp: &ActiveExpeditionSummary, w: &Weights) -> Option<u8> {
    if exp.encounters.is_empty() {
        return None;
    }
    exp.encounters
        .iter()
        .map(|enc| {
            let mut score = (enc.heroism as f32) * w.heroism;
            if enc.closes_target {
                score += w.closes_target;
            }
            if enc.is_bounty {
                score += w.picks_bounty;
            }
            (enc.pos, score)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(pos, _)| pos)
}

pub fn pick_reward(exp: &ActiveExpeditionSummary, w: &Weights) -> Option<u8> {
    if exp.rewards.is_empty() {
        return None;
    }
    exp.rewards
        .iter()
        .map(|r| (r.pos, reward_score(r, w)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(pos, _)| pos)
}

fn reward_score(r: &ExpeditionRewardBrief, w: &Weights) -> f32 {
    match r.kind {
        "silver" => (r.silver as f32) * w.reward_silver_per_unit,
        "experience" => (r.experience as f32) * w.reward_xp_per_unit,
        "mushroom" => (r.mushrooms as f32) * w.reward_mushroom_per_unit,
        "fruit" => w.reward_fruit_per_unit,
        "resource" => w.reward_resource_per_unit,
        "honor" => (r.silver as f32) * w.reward_silver_per_unit * 0.5,
        _ => w.reward_other_fallback,
    }
}

/// Convenience: given the top-level state, decide if we should emit a
/// StartExpedition and which index. Returns None if conditions aren't met.
pub fn should_start_expedition(state: &StateSummary, w: &Weights) -> Option<u8> {
    if state.tavern.mode != "expeditions" {
        return None;
    }
    if state.tavern.active_expedition.is_some() {
        return None;
    }
    let affordable: Vec<&ExpeditionSummary> = state
        .tavern
        .expeditions
        .iter()
        .filter(|e| state.tavern.thirst_for_adventure_sec >= e.thirst_for_adventure_sec)
        .collect();
    if affordable.is_empty() {
        return None;
    }
    // Pick over affordable only so we never recommend an unaffordable one.
    let owned: Vec<ExpeditionSummary> = affordable
        .iter()
        .map(|&e| ExpeditionSummary {
            index: e.index,
            target: e.target.clone(),
            thirst_for_adventure_sec: e.thirst_for_adventure_sec,
            special: e.special.clone(),
        })
        .collect();
    pick_start(&owned, w)
}
