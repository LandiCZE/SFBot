use serde::{Deserialize, Serialize};
use sf_api::command::AttributeType;

use crate::game::StateSummary;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    StartQuest { quest_index: u8 },
    StartExpedition { expedition_index: u8 },
    BuyAttribute { attribute: Attr, points: u32 },
    EquipItem { backpack_slot: usize },
    SellItem { backpack_slot: usize },
    DrinkPotion { backpack_slot: usize },
    StartGuardWork { hours: u8 },
    FightArena,
    SetQuestingPreference { prefer_quests: bool },
    Wait,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Attr {
    Strength,
    Dexterity,
    Intelligence,
    Constitution,
    Luck,
}

impl Attr {
    pub fn to_sf(self) -> AttributeType {
        match self {
            Attr::Strength => AttributeType::Strength,
            Attr::Dexterity => AttributeType::Dexterity,
            Attr::Intelligence => AttributeType::Intelligence,
            Attr::Constitution => AttributeType::Constitution,
            Attr::Luck => AttributeType::Luck,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Decision {
    #[serde(flatten)]
    pub action: Action,
    pub reason: String,
}

pub fn validate(action: &Action, state: &StateSummary) -> Result<(), String> {
    // sf-api's Tavern::is_idle() treats current_action="expedition" as idle when
    // there is no active expedition (i.e. a Finished expedition that the server
    // hasn't fully cleared yet). Mirror that so start_* actions aren't blocked.
    let idle = state.tavern.current_action == "idle"
        || (state.tavern.current_action == "expedition"
            && state.tavern.active_expedition.is_none());
    match action {
        Action::StartQuest { quest_index } => {
            if !idle {
                return Err(format!("not idle ({})", state.tavern.current_action));
            }
            if state.tavern.mode != "quests" {
                return Err(format!("quests not available (mode={})", state.tavern.mode));
            }
            if (*quest_index as usize) >= state.tavern.quests.len() {
                return Err(format!(
                    "quest_index {quest_index} out of range (have {} quests)",
                    state.tavern.quests.len()
                ));
            }
            let q = &state.tavern.quests[*quest_index as usize];
            if state.tavern.thirst_for_adventure_sec < q.duration_sec {
                return Err(format!(
                    "need {}s thirst, have {}s",
                    q.duration_sec, state.tavern.thirst_for_adventure_sec
                ));
            }
            Ok(())
        }
        Action::StartExpedition { expedition_index } => {
            if !idle {
                return Err(format!("not idle ({})", state.tavern.current_action));
            }
            if state.tavern.mode != "expeditions" {
                return Err(format!(
                    "expeditions not available (mode={})",
                    state.tavern.mode
                ));
            }
            if state.tavern.active_expedition.is_some() {
                return Err("already on an expedition".into());
            }
            if (*expedition_index as usize) >= state.tavern.expeditions.len() {
                return Err(format!(
                    "expedition_index {expedition_index} out of range (have {})",
                    state.tavern.expeditions.len()
                ));
            }
            let e = &state.tavern.expeditions[*expedition_index as usize];
            if state.tavern.thirst_for_adventure_sec < e.thirst_for_adventure_sec {
                return Err(format!(
                    "need {}s thirst, have {}s",
                    e.thirst_for_adventure_sec, state.tavern.thirst_for_adventure_sec
                ));
            }
            Ok(())
        }
        Action::BuyAttribute { attribute, points } => {
            if *points == 0 {
                return Err("points must be >= 1".into());
            }
            if *points > 50 {
                return Err("refusing to buy more than 50 points in one call".into());
            }
            let stat = match attribute {
                Attr::Strength => &state.character.attributes.strength,
                Attr::Dexterity => &state.character.attributes.dexterity,
                Attr::Intelligence => &state.character.attributes.intelligence,
                Attr::Constitution => &state.character.attributes.constitution,
                Attr::Luck => &state.character.attributes.luck,
            };
            let class_mult = match attribute {
                Attr::Luck => 5u64,
                a if attr_matches_main_or_con(*a, state.character.main_attribute) => 1,
                _ => 2,
            };
            let level = state.character.level as u64;
            let mut total: u64 = 0;
            for i in 0..*points as u64 {
                let n = stat.times_bought as u64 + i;
                total = total.saturating_add((n * n * n * class_mult + 25) * level);
            }
            if state.character.silver < total {
                return Err(format!(
                    "need ~{total} silver for {points}x {:?}, have {}",
                    attribute, state.character.silver
                ));
            }
            Ok(())
        }
        Action::EquipItem { backpack_slot } => {
            let b = backpack_item(*backpack_slot, state)?;
            if b.target_equipment_slot.is_none() {
                return Err(format!(
                    "slot {backpack_slot} item is not equipment ({})",
                    b.item.kind
                ));
            }
            Ok(())
        }
        Action::SellItem { backpack_slot } => {
            backpack_item(*backpack_slot, state)?;
            Ok(())
        }
        Action::DrinkPotion { backpack_slot } => {
            let b = backpack_item(*backpack_slot, state)?;
            if b.potion.is_none() {
                return Err(format!(
                    "slot {backpack_slot} is not a potion ({})",
                    b.item.kind
                ));
            }
            if !state.character.active_potion_slot_free {
                return Err("all 3 active potion slots are full".into());
            }
            Ok(())
        }
        Action::StartGuardWork { hours } => {
            if !(1..=10).contains(hours) {
                return Err("hours must be 1..=10".into());
            }
            if !idle {
                return Err(format!("not idle ({})", state.tavern.current_action));
            }
            Ok(())
        }
        Action::FightArena => {
            if !state.arena.off_cooldown {
                return Err(format!(
                    "arena on cooldown ({}s)",
                    state.arena.next_free_fight_sec_remaining.unwrap_or_default()
                ));
            }
            if state.arena.enemy_ids.is_empty() {
                return Err("no arena opponents visible".into());
            }
            Ok(())
        }
        Action::SetQuestingPreference { .. } => {
            if !state.tavern.can_change_questing_preference {
                return Err(
                    "preference change needs full ALU + 0 beers (server also checks 'alu used today')".into(),
                );
            }
            Ok(())
        }
        Action::Wait => Ok(()),
    }
}

fn attr_matches_main_or_con(a: Attr, main: &str) -> bool {
    if matches!(a, Attr::Constitution) {
        return true;
    }
    matches!(
        (a, main),
        (Attr::Strength, "strength")
            | (Attr::Dexterity, "dexterity")
            | (Attr::Intelligence, "intelligence")
    )
}

fn backpack_item(
    slot: usize,
    state: &StateSummary,
) -> Result<&crate::game::BackpackItemSummary, String> {
    state
        .backpack
        .iter()
        .find(|b| b.slot == slot)
        .ok_or_else(|| format!("backpack slot {slot} is empty"))
}
