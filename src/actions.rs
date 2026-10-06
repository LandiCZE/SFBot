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
    DismantleItem { backpack_slot: usize },
    StartGuardWork { hours: u8 },
    FightArena,
    FightDungeon,
    FightTower,
    FightPortal,
    FortressUpgradeBuilding { building: String },
    FortressGatherResource { resource: String },
    FortressTrainUnit { unit: String, count: u32 },
    FortressUpgradeUnit { unit: String },
    FortressAttack,
    FortressRerollEnemy,
    FeedPet { pet_id: u32, habitat: String },
    FightPetHabitat { habitat: String },
    FightPetOpponent { habitat: String },
    BuyShopItem { shop: String, pos: u8 },
    ClaimTaskChest { track: String, pos: u8 },
    OpenMail { pos: u8 },
    DeleteAllMail,
    ClaimPendingMail { msg_id: i64 },
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
                return Err(format!("expeditions not available (mode={})", state.tavern.mode));
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
        Action::DismantleItem { backpack_slot } => {
            backpack_item(*backpack_slot, state)?;
            let bs = state
                .blacksmith
                .as_ref()
                .ok_or_else(|| "blacksmith not unlocked".to_string())?;
            if bs.dismantle_left == 0 {
                return Err("no dismantles left today".into());
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
        Action::FightDungeon => {
            if !state.dungeons.off_cooldown {
                return Err(format!(
                    "dungeons on cooldown ({}s)",
                    state
                        .dungeons
                        .next_free_fight_sec_remaining
                        .unwrap_or_default()
                ));
            }
            if state.dungeons.best_winnable_name.is_none() {
                return Err("no winnable dungeon within safe-margin".into());
            }
            Ok(())
        }
        Action::FightTower => {
            if !state.dungeons.off_cooldown {
                return Err(format!(
                    "dungeons on cooldown ({}s)",
                    state.dungeons.next_free_fight_sec_remaining.unwrap_or_default()
                ));
            }
            let t = state
                .dungeons
                .tower
                .as_ref()
                .ok_or_else(|| "tower not open".to_string())?;
            if !t.winnable {
                return Err(format!(
                    "tower floor {} enemy too strong (level {:?})",
                    t.current_floor + 1,
                    t.enemy_level
                ));
            }
            Ok(())
        }
        Action::FightPortal => {
            let p = state
                .dungeons
                .portal
                .as_ref()
                .ok_or_else(|| "portal not visible (unlocks at char lvl 99)".to_string())?;
            if !p.can_fight {
                return Err("portal already fought today".into());
            }
            if p.enemy_hp_percentage == 0 {
                return Err("portal enemy has 0 HP (nothing to fight)".into());
            }
            Ok(())
        }
        Action::FortressUpgradeBuilding { building } => {
            let f = state
                .fortress
                .as_ref()
                .ok_or_else(|| "fortress not unlocked".to_string())?;
            if f.upgrade_in_progress.is_some() {
                return Err("another fortress upgrade is already in progress".into());
            }
            let b = f
                .buildings
                .iter()
                .find(|b| b.name == building.as_str())
                .ok_or_else(|| format!("unknown building {building:?}"))?;
            if !b.buildable_now {
                return Err(format!(
                    "{building} not buildable now (level={}, cost: wood={} stone={} silver={})",
                    b.level, b.wood_cost, b.stone_cost, b.silver_cost
                ));
            }
            Ok(())
        }
        Action::FeedPet { pet_id, habitat } => {
            let p = state
                .pets
                .as_ref()
                .ok_or_else(|| "pets not unlocked".to_string())?;
            let h = p
                .habitats
                .iter()
                .find(|h| h.name == habitat.as_str())
                .ok_or_else(|| format!("unknown habitat {habitat:?}"))?;
            if h.fruits_wallet == 0 {
                return Err(format!("no {habitat} fruit in wallet"));
            }
            if !h.hungry_pet_ids.contains(pet_id) {
                return Err(format!(
                    "pet {pet_id} is not hungry (or not in {habitat})"
                ));
            }
            Ok(())
        }
        Action::FightPetHabitat { habitat } => {
            let p = state
                .pets
                .as_ref()
                .ok_or_else(|| "pets not unlocked".to_string())?;
            if p.next_free_exploration_sec_remaining
                .map(|s| s > 0)
                .unwrap_or(false)
            {
                return Err("pet exploration is on cooldown".into());
            }
            let h = p
                .habitats
                .iter()
                .find(|h| h.name == habitat.as_str())
                .ok_or_else(|| format!("unknown habitat {habitat:?}"))?;
            if h.is_finished {
                return Err(format!("{habitat} habitat already finished"));
            }
            if h.strongest_pet_id.is_none() {
                return Err(format!("no unlocked pet in {habitat} to send"));
            }
            Ok(())
        }
        Action::FightPetOpponent { habitat } => {
            let p = state
                .pets
                .as_ref()
                .ok_or_else(|| "pets not unlocked".to_string())?;
            let o = p
                .opponent
                .as_ref()
                .ok_or_else(|| "no pet opponent set".to_string())?;
            if o.habitat != Some(habitat.as_str())
                && o.habitat.map(|s| s.to_string()) != Some(habitat.clone())
            {
                return Err(format!(
                    "opponent is in habitat {:?}, not {habitat}",
                    o.habitat
                ));
            }
            if o.next_free_battle_sec_remaining.map(|s| s > 0).unwrap_or(false) {
                return Err("pet PvP is on cooldown".into());
            }
            let h = p
                .habitats
                .iter()
                .find(|h| h.name == habitat.as_str())
                .ok_or_else(|| format!("unknown habitat {habitat:?}"))?;
            if h.battled_opponent_today {
                return Err(format!("already battled {habitat} opponent today"));
            }
            Ok(())
        }
        Action::BuyShopItem { shop, pos } => {
            let s = state
                .shops
                .iter()
                .find(|s| s.shop == shop.as_str())
                .ok_or_else(|| format!("unknown shop {shop:?}"))?;
            let item = s
                .items
                .iter()
                .find(|i| i.pos == *pos)
                .ok_or_else(|| format!("shop {shop} slot {pos} is empty"))?;
            if state.character.silver < item.price_silver as u64 {
                return Err(format!(
                    "need {} silver, have {}",
                    item.price_silver, state.character.silver
                ));
            }
            if state.character.backpack_free_slots == 0 {
                return Err("backpack is full".into());
            }
            Ok(())
        }
        Action::ClaimTaskChest { track, pos } => {
            if *pos >= 3 {
                return Err("pos must be 0..=2".into());
            }
            let claimable = match track.as_str() {
                "daily" => &state.tasks.daily_claimable_chests,
                "event" => &state.tasks.event_claimable_chests,
                other => return Err(format!("unknown track {other:?}")),
            };
            if !claimable.contains(pos) {
                return Err(format!("chest {pos} is not currently claimable on {track}"));
            }
            Ok(())
        }
        Action::OpenMail { pos } => {
            if (*pos as usize) >= state.mail.inbox_total {
                return Err(format!(
                    "mail pos {pos} out of range (inbox has {})",
                    state.mail.inbox_total
                ));
            }
            Ok(())
        }
        Action::DeleteAllMail => {
            if state.mail.inbox_total == 0 {
                return Err("inbox is empty".into());
            }
            Ok(())
        }
        Action::ClaimPendingMail { msg_id } => {
            if !state.mail.claimables_pending.contains(msg_id) {
                return Err(format!("no pending claimable with msg_id {msg_id}"));
            }
            Ok(())
        }
        Action::FortressTrainUnit { unit, count } => {
            let f = state
                .fortress
                .as_ref()
                .ok_or_else(|| "fortress not unlocked".to_string())?;
            let u = f
                .units
                .iter()
                .find(|u| u.name == unit.as_str())
                .ok_or_else(|| format!("unknown unit {unit:?}"))?;
            if !u.has_training_building {
                return Err(format!("{unit}: training building not built"));
            }
            if u.training_finishes_in_sec.map(|s| s > 0).unwrap_or(false) {
                return Err(format!("{unit}: training already in progress"));
            }
            if *count == 0 {
                return Err("count must be >= 1".into());
            }
            let n = *count as u64;
            let wood_need = u.training_cost_wood.saturating_mul(n);
            let stone_need = u.training_cost_stone.saturating_mul(n);
            let silver_need = u.training_cost_silver.saturating_mul(n);
            if f.wood_current < wood_need
                || f.stone_current < stone_need
                || state.character.silver < silver_need
            {
                return Err(format!(
                    "need wood {wood_need}, stone {stone_need}, silver {silver_need} — have wood {}, stone {}, silver {}",
                    f.wood_current, f.stone_current, state.character.silver
                ));
            }
            Ok(())
        }
        Action::FortressUpgradeUnit { unit } => {
            let f = state
                .fortress
                .as_ref()
                .ok_or_else(|| "fortress not unlocked".to_string())?;
            let u = f
                .units
                .iter()
                .find(|u| u.name == unit.as_str())
                .ok_or_else(|| format!("unknown unit {unit:?}"))?;
            if !u.has_training_building {
                return Err(format!("{unit}: training building not built"));
            }
            if f.wood_current < u.upgrade_cost_wood || f.stone_current < u.upgrade_cost_stone {
                return Err(format!(
                    "unit upgrade needs wood {} stone {} — have wood {} stone {}",
                    u.upgrade_cost_wood, u.upgrade_cost_stone, f.wood_current, f.stone_current
                ));
            }
            Ok(())
        }
        Action::FortressAttack => {
            let f = state
                .fortress
                .as_ref()
                .ok_or_else(|| "fortress not unlocked".to_string())?;
            if !f.attack_target_present {
                return Err("no attack target assigned".into());
            }
            let advice = f
                .attack_target_soldier_advice
                .ok_or_else(|| "target soldier_advice not loaded yet".to_string())?;
            let soldier = f
                .units
                .iter()
                .find(|u| u.name == "soldier")
                .ok_or_else(|| "no soldier unit".to_string())?;
            if (soldier.count as u16) < advice.saturating_add(2) {
                return Err(format!(
                    "need {} soldiers (advice {} + 2 buffer), have {}",
                    advice + 2,
                    advice,
                    soldier.count
                ));
            }
            let archer = f.units.iter().find(|u| u.name == "archer");
            let magician = f.units.iter().find(|u| u.name == "magician");
            if archer.map(|u| u.count).unwrap_or(0) == 0
                || magician.map(|u| u.count).unwrap_or(0) == 0
            {
                return Err("need at least 1 archer and 1 magician for support".into());
            }
            Ok(())
        }
        Action::FortressRerollEnemy => {
            let f = state
                .fortress
                .as_ref()
                .ok_or_else(|| "fortress not unlocked".to_string())?;
            if !f.attack_reroll_free {
                return Err("reroll is not free yet".into());
            }
            Ok(())
        }
        Action::FortressGatherResource { resource } => {
            let f = state
                .fortress
                .as_ref()
                .ok_or_else(|| "fortress not unlocked".to_string())?;
            match resource.as_str() {
                "wood" => {
                    if f.wood_current == 0 {
                        return Err("no wood to gather".into());
                    }
                }
                "stone" => {
                    if f.stone_current == 0 {
                        return Err("no stone to gather".into());
                    }
                }
                "experience" => {
                    if f.experience_current == 0 {
                        return Err("no experience to gather".into());
                    }
                }
                other => return Err(format!("unknown resource {other:?}")),
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
