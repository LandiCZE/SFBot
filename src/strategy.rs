//! Pure heuristic action picker. Covers the mechanical cases so Claude only
//! sees genuine tradeoffs. If this returns None, the main loop asks Claude.

use crate::actions::{Action, Attr};
use crate::game::StateSummary;

/// Minimum free backpack slots required before starting an expedition.
/// Expeditions can drop up to ~4 items (encounter targets + boss drops);
/// 3 free slots leaves comfortable headroom.
const MIN_FREE_SLOTS_FOR_EXPEDITION: usize = 3;

pub struct HeuristicPick {
    pub action: Action,
    pub reason: &'static str,
}

pub fn pick(state: &StateSummary) -> Option<HeuristicPick> {
    let idle = state.tavern.current_action == "idle"
        || (state.tavern.current_action == "expedition"
            && state.tavern.active_expedition.is_none());

    // 0. Equip clear upgrade — do this first so the stat boost applies to
    //    the very next fight. (Equipping is a swap, so it doesn't free a
    //    backpack slot — see sell/dismantle below for that.)
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

    // 1. Drink main-attr or Con potion if a slot is free.
    if state.character.active_potion_slot_free {
        let main = state.character.main_attribute;
        let active_kinds: Vec<&str> = state
            .character
            .active_potions
            .iter()
            .map(|p| p.kind)
            .collect();
        let size_rank = |s: &str| match s {
            "large" => 3,
            "medium" => 2,
            "small" => 1,
            _ => 0,
        };
        if let Some(b) = state
            .backpack
            .iter()
            .filter(|b| {
                b.potion
                    .as_ref()
                    .map(|p| {
                        (p.kind == main || p.kind == "constitution")
                            && !active_kinds.contains(&p.kind)
                    })
                    .unwrap_or(false)
            })
            .max_by_key(|b| size_rank(b.potion.as_ref().map(|p| p.size).unwrap_or("")))
        {
            return Some(HeuristicPick {
                action: Action::DrinkPotion { backpack_slot: b.slot },
                reason: "heuristic: drink main-attr or constitution potion while slot is free",
            });
        }
    }

    // 2. Dismantle junk at the blacksmith (preferred over selling if available).
    if let Some(junk) = state
        .backpack
        .iter()
        .filter(|b| b.is_junk)
        .max_by_key(|b| b.sell_price_silver)
    {
        if let Some(bs) = state.blacksmith.as_ref() {
            if bs.dismantle_left > 0 {
                return Some(HeuristicPick {
                    action: Action::DismantleItem { backpack_slot: junk.slot },
                    reason: "heuristic: dismantle junk at blacksmith (metal/arcane > silver)",
                });
            }
        }
        return Some(HeuristicPick {
            action: Action::SellItem { backpack_slot: junk.slot },
            reason: "heuristic: sell junk item",
        });
    }

    // 3. Buy main attribute if affordable.
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

    // 4. Fortress: gather overflowing resource.
    if let Some(f) = state.fortress.as_ref() {
        for (name, cur, lim) in [
            ("wood", f.wood_current, f.wood_limit),
            ("stone", f.stone_current, f.stone_limit),
            ("experience", f.experience_current, f.experience_limit),
        ] {
            if lim > 0 && cur * 10 >= lim * 9 {
                return Some(HeuristicPick {
                    action: Action::FortressGatherResource {
                        resource: name.into(),
                    },
                    reason: "heuristic: fortress resource ≥ 90% full — gather",
                });
            }
        }
    }

    // 5. Fortress: upgrade cheapest buildable if no upgrade in progress.
    if let Some(f) = state.fortress.as_ref() {
        if f.upgrade_in_progress.is_none() {
            if let Some(b) = f
                .buildings
                .iter()
                .filter(|b| b.buildable_now)
                .min_by_key(|b| b.wood_cost + b.stone_cost)
            {
                return Some(HeuristicPick {
                    action: Action::FortressUpgradeBuilding {
                        building: b.name.into(),
                    },
                    reason: "heuristic: fortress idle — upgrade cheapest buildable",
                });
            }
        }
    }

    // 6. Dungeons: free fight on a winnable target.
    if state.dungeons.off_cooldown && state.dungeons.best_winnable_name.is_some() {
        return Some(HeuristicPick {
            action: Action::FightDungeon,
            reason: "heuristic: free dungeon fight on winnable target",
        });
    }

    // 7. Arena fight.
    if state.arena.off_cooldown && !state.arena.enemy_ids.is_empty() {
        return Some(HeuristicPick {
            action: Action::FightArena,
            reason: "heuristic: arena off cooldown + opponents visible",
        });
    }

    if !idle {
        return None;
    }

    // 8. Start expedition — pre-flight: require >=3 free backpack slots so
    //    the ~4 items an expedition can drop don't overflow. Rule 2
    //    (sell/dismantle junk) will fire first if we're low on slots.
    if state.tavern.mode == "expeditions" && state.tavern.active_expedition.is_none() {
        if state.character.backpack_free_slots < MIN_FREE_SLOTS_FOR_EXPEDITION {
            tracing::debug!(
                free = state.character.backpack_free_slots,
                need = MIN_FREE_SLOTS_FOR_EXPEDITION,
                "backpack too full for expedition — waiting for a clear cycle"
            );
        } else if let Some(exp) = state
            .tavern
            .expeditions
            .iter()
            .filter(|e| state.tavern.thirst_for_adventure_sec >= e.thirst_for_adventure_sec)
            .min_by_key(|e| e.thirst_for_adventure_sec)
        {
            return Some(HeuristicPick {
                action: Action::StartExpedition { expedition_index: exp.index },
                reason: "heuristic: idle, expeditions mode, backpack has headroom",
            });
        }
    }

    // 9. Start quest.
    if state.tavern.mode == "quests" && !state.tavern.quests.is_empty() {
        if let Some(q) = state
            .tavern
            .quests
            .iter()
            .filter(|q| state.tavern.thirst_for_adventure_sec >= q.duration_sec)
            .max_by_key(|q| (q.experience as u64 * 60_000) / (q.duration_sec.max(1) as u64))
        {
            return Some(HeuristicPick {
                action: Action::StartQuest { quest_index: q.index },
                reason: "heuristic: pick highest XP/min quest",
            });
        }
    }

    // 10. Guard work if thirst drained.
    if state.tavern.thirst_for_adventure_sec == 0 && idle {
        return Some(HeuristicPick {
            action: Action::StartGuardWork { hours: 10 },
            reason: "heuristic: thirst drained — long guard shift for passive silver",
        });
    }

    None
}
