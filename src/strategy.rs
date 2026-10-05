//! Pure heuristic action picker. Covers the mechanical cases so Claude only
//! sees genuine tradeoffs. If this returns None, the main loop asks Claude.
//!
//! Rules (first match wins):
//! 1. Equip clear upgrade — backpack item with largest positive main_stat_delta.
//! 2. Sell junk — backpack item with no slot or strictly worse than equipped.
//! 3. Buy attribute — main-attr next point when silver > 2 × its cost.
//! 4. Fight arena — off cooldown + visible opponents.
//! 5. Start expedition — idle, expeditions mode, enough thirst for cheapest.
//! 6. Start quest — idle, quests mode, pick highest xp/min quest we can afford.
//! 7. Guard work 10h — idle, thirst is 0 and no gear/attr/arena to do.
//!
//! Hard rule inherited: no action variant below can spend mushrooms.

use crate::actions::{Action, Attr};
use crate::game::StateSummary;

pub struct HeuristicPick {
    pub action: Action,
    pub reason: &'static str,
}

pub fn pick(state: &StateSummary) -> Option<HeuristicPick> {
    let idle = state.tavern.current_action == "idle"
        || (state.tavern.current_action == "expedition"
            && state.tavern.active_expedition.is_none());

    // 1. Equip clear upgrade.
    if let Some(upgrade) = state
        .backpack
        .iter()
        .filter(|b| b.main_stat_delta_vs_equipped.unwrap_or(0) > 0)
        .max_by_key(|b| b.main_stat_delta_vs_equipped.unwrap_or(0))
    {
        return Some(HeuristicPick {
            action: Action::EquipItem { backpack_slot: upgrade.slot },
            reason: "heuristic: clear main-stat upgrade in backpack",
        });
    }

    // 2. Sell junk (clearly worse or non-equippable).
    if let Some(junk) = state
        .backpack
        .iter()
        .filter(|b| b.is_junk)
        .max_by_key(|b| b.sell_price_silver)
    {
        return Some(HeuristicPick {
            action: Action::SellItem { backpack_slot: junk.slot },
            reason: "heuristic: sell junk item (no slot or strictly worse than equipped)",
        });
    }

    // 3. Buy a main-attribute point if silver is comfortably above cost.
    let main = state.character.main_attribute;
    let (main_stat, main_attr_variant) = match main {
        "strength" => (&state.character.attributes.strength, Attr::Strength),
        "dexterity" => (&state.character.attributes.dexterity, Attr::Dexterity),
        "intelligence" => (&state.character.attributes.intelligence, Attr::Intelligence),
        _ => (&state.character.attributes.dexterity, Attr::Dexterity),
    };
    if state.character.silver >= main_stat.next_point_cost_silver.saturating_mul(2)
        && main_stat.next_point_cost_silver > 0
    {
        return Some(HeuristicPick {
            action: Action::BuyAttribute {
                attribute: main_attr_variant,
                points: 1,
            },
            reason: "heuristic: silver >= 2× main-attr next-point cost — buy 1 point",
        });
    }

    // 3b. Also buy a Constitution point under the same rule.
    let con_stat = &state.character.attributes.constitution;
    if state.character.silver >= con_stat.next_point_cost_silver.saturating_mul(2)
        && con_stat.next_point_cost_silver > 0
    {
        return Some(HeuristicPick {
            action: Action::BuyAttribute {
                attribute: Attr::Constitution,
                points: 1,
            },
            reason: "heuristic: silver >= 2× constitution next-point cost — buy 1 point",
        });
    }

    // 4. Arena fight.
    if state.arena.off_cooldown && !state.arena.enemy_ids.is_empty() {
        return Some(HeuristicPick {
            action: Action::FightArena,
            reason: "heuristic: arena off cooldown + opponents visible",
        });
    }

    if !idle {
        // Non-idle without a timer branch (shouldn't happen) — hand off.
        return None;
    }

    // 5. Start expedition.
    if state.tavern.mode == "expeditions" && state.tavern.active_expedition.is_none() {
        if let Some(exp) = state
            .tavern
            .expeditions
            .iter()
            .filter(|e| state.tavern.thirst_for_adventure_sec >= e.thirst_for_adventure_sec)
            .min_by_key(|e| e.thirst_for_adventure_sec)
        {
            return Some(HeuristicPick {
                action: Action::StartExpedition { expedition_index: exp.index },
                reason: "heuristic: idle, expeditions mode, enough thirst for cheapest",
            });
        }
    }

    // 6. Start quest (highest XP/min we can afford).
    if state.tavern.mode == "quests" && !state.tavern.quests.is_empty() {
        if let Some(q) = state
            .tavern
            .quests
            .iter()
            .filter(|q| state.tavern.thirst_for_adventure_sec >= q.duration_sec)
            .max_by_key(|q| {
                // XP per minute, scaled by 1000 to stay integer.
                let per_min = (q.experience as u64 * 60_000) / (q.duration_sec.max(1) as u64);
                per_min
            })
        {
            return Some(HeuristicPick {
                action: Action::StartQuest { quest_index: q.index },
                reason: "heuristic: pick highest XP/min quest",
            });
        }
    }

    // 7. Guard work if thirst is exhausted.
    if state.tavern.thirst_for_adventure_sec == 0 && idle {
        return Some(HeuristicPick {
            action: Action::StartGuardWork { hours: 10 },
            reason: "heuristic: thirst drained — long guard shift for passive silver",
        });
    }

    // No clear choice — defer to Claude for nuanced tie-breaking.
    None
}
