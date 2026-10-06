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

    // 1a. Witch: drop a junk item matching required_slot (progresses enchant unlock).
    if let Some(w) = state.witch.as_ref() {
        if let Some(required_slot) = w.required_slot {
            if !w.cauldron_bubbling {
                if let Some(b) = state.backpack.iter().find(|b| {
                    b.is_junk && b.target_equipment_slot == Some(required_slot)
                }) {
                    return Some(HeuristicPick {
                        action: Action::WitchDropItem { backpack_slot: b.slot },
                        reason: "heuristic: drop junk into witch cauldron (matches required slot)",
                    });
                }
            }
        }
    }

    // 1b. Toilet: flush when ready, feed junk when hungry.
    if let Some(t) = state.toilet.as_ref() {
        if t.ready_to_flush {
            return Some(HeuristicPick {
                action: Action::ToiletFlush,
                reason: "heuristic: toilet mana full — flush for aura",
            });
        }
        // Feed a junk item (slow lane; dismantle/sell preferred first).
        if state.blacksmith.as_ref().map(|b| b.dismantle_left == 0).unwrap_or(true) {
            if let Some(b) = state.backpack.iter().find(|b| b.is_junk) {
                return Some(HeuristicPick {
                    action: Action::ToiletDropItem { backpack_slot: b.slot },
                    reason: "heuristic: feed junk to toilet (dismantle exhausted / blacksmith missing)",
                });
            }
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

    // 3c. Claim task chests (free rewards: silver, mushrooms via chest, items).
    for &pos in &state.tasks.daily_claimable_chests {
        return Some(HeuristicPick {
            action: Action::ClaimTaskChest {
                track: "daily".into(),
                pos,
            },
            reason: "heuristic: unclaimed daily chest reward",
        });
    }
    for &pos in &state.tasks.event_claimable_chests {
        return Some(HeuristicPick {
            action: Action::ClaimTaskChest {
                track: "event".into(),
                pos,
            },
            reason: "heuristic: unclaimed event chest reward",
        });
    }

    // 3d. Claim pending mail attachments before they expire.
    if let Some(&msg_id) = state.mail.claimables_pending.first() {
        return Some(HeuristicPick {
            action: Action::ClaimPendingMail { msg_id },
            reason: "heuristic: pending mail attachment to claim",
        });
    }

    // 3e. Open unread mail (first unread; delete-all later as housekeeping).
    if state.mail.inbox_unread > 0 && state.mail.inbox_total > 0 {
        // Open the first entry — unread entries tend to be at the top; a
        // single pos=0 Open suffices and the inbox position shifts semantics
        // after each read anyway.
        return Some(HeuristicPick {
            action: Action::OpenMail { pos: 0 },
            reason: "heuristic: open unread mail",
        });
    }

    // 3f. Shop upgrade: buy clear upgrade with silver, enough headroom.
    if state.character.backpack_free_slots > 0 {
        if let Some((shop_name, pos)) = state
            .shops
            .iter()
            .flat_map(|s| s.items.iter().map(move |it| (s.shop, it)))
            .filter(|(_, it)| {
                it.can_equip
                    && it.main_stat_delta_vs_equipped.unwrap_or(0) > 0
                    && (it.price_silver as u64) * 4 <= state.character.silver
            })
            .max_by_key(|(_, it)| it.main_stat_delta_vs_equipped.unwrap_or(0))
            .map(|(s, it)| (s, it.pos))
        {
            return Some(HeuristicPick {
                action: Action::BuyShopItem {
                    shop: shop_name.into(),
                    pos,
                },
                reason: "heuristic: shop item is a main-stat upgrade and <= silver/4",
            });
        }
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

    // 5a. Fortress: train missing units when the training building exists,
    //     slot is free, and we can afford at least a small batch.
    if let Some(f) = state.fortress.as_ref() {
        for u in f.units.iter() {
            if !u.has_training_building {
                continue;
            }
            if u.training_finishes_in_sec.map(|s| s > 0).unwrap_or(false) {
                continue;
            }
            // Target a modest standing army; retrain in batches of 5.
            let target = match u.name {
                "soldier" => 25u16,
                _ => 10u16,
            };
            if u.count >= target {
                continue;
            }
            let wanted = (target - u.count).min(5) as u64;
            let wood_need = u.training_cost_wood.saturating_mul(wanted);
            let stone_need = u.training_cost_stone.saturating_mul(wanted);
            let silver_need = u.training_cost_silver.saturating_mul(wanted);
            if f.wood_current < wood_need
                || f.stone_current < stone_need
                || state.character.silver < silver_need
            {
                continue;
            }
            return Some(HeuristicPick {
                action: Action::FortressTrainUnit {
                    unit: u.name.into(),
                    count: wanted as u32,
                },
                reason: "heuristic: top up fortress unit roster",
            });
        }
    }

    // 5b. Fortress: attack when we have a target and are strong enough.
    if let Some(f) = state.fortress.as_ref() {
        if f.attack_target_present
            && f.attack_target_soldier_advice.is_some()
            && !matches!(
                (
                    f.units.iter().find(|u| u.name == "soldier").map(|u| u.count),
                    f.attack_target_soldier_advice,
                ),
                (Some(count), Some(advice)) if (count as u16) < advice.saturating_add(2)
            )
        {
            let archers_ok = f
                .units
                .iter()
                .find(|u| u.name == "archer")
                .map(|u| u.count > 0)
                .unwrap_or(false);
            let magicians_ok = f
                .units
                .iter()
                .find(|u| u.name == "magician")
                .map(|u| u.count > 0)
                .unwrap_or(false);
            if archers_ok && magicians_ok {
                return Some(HeuristicPick {
                    action: Action::FortressAttack,
                    reason: "heuristic: fortress target winnable (soldiers ≥ advice+2 + support)",
                });
            }
        }
    }

    // 5b2. Underworld: gather any resource near its cap.
    if let Some(uw) = state.underworld.as_ref() {
        for r in uw.resources.iter() {
            if r.limit > 0 && r.current * 10 >= r.limit * 9 {
                return Some(HeuristicPick {
                    action: Action::UnderworldGatherResource {
                        resource: r.name.into(),
                    },
                    reason: "heuristic: underworld resource ≥ 90% full — gather",
                });
            }
        }
    }

    // 5b3. Underworld: upgrade cheapest buildable when idle.
    if let Some(uw) = state.underworld.as_ref() {
        if uw.upgrade_in_progress.is_none() {
            if let Some(b) = uw
                .buildings
                .iter()
                .filter(|b| b.buildable_now)
                .min_by_key(|b| b.upgrade_cost_silver + b.upgrade_cost_souls * 10)
            {
                return Some(HeuristicPick {
                    action: Action::UnderworldUpgradeBuilding {
                        building: b.name.into(),
                    },
                    reason: "heuristic: underworld idle — upgrade cheapest buildable",
                });
            }
        }
    }

    // 5c. Fortress: reroll a too-strong target when the reroll is free.
    if let Some(f) = state.fortress.as_ref() {
        if f.attack_target_present && f.attack_reroll_free {
            if let (Some(advice), Some(count)) = (
                f.attack_target_soldier_advice,
                f.units.iter().find(|u| u.name == "soldier").map(|u| u.count),
            ) {
                if (count as u16) < advice.saturating_add(2) {
                    return Some(HeuristicPick {
                        action: Action::FortressRerollEnemy,
                        reason: "heuristic: current fortress target too strong, free reroll",
                    });
                }
            }
        }
    }

    // 5d. Pets: feed any hungry pet of a habitat with fruit in the wallet.
    if let Some(pets) = state.pets.as_ref() {
        for h in pets.habitats.iter() {
            if h.fruits_wallet == 0 {
                continue;
            }
            if let Some(&pet_id) = h.hungry_pet_ids.first() {
                return Some(HeuristicPick {
                    action: Action::FeedPet {
                        pet_id,
                        habitat: h.name.into(),
                    },
                    reason: "heuristic: feed hungry pet (fruit wallet > 0, below max level)",
                });
            }
        }
    }

    // 5e. Pets: habitat PvE on free timer, pick a habitat we can still progress.
    if let Some(pets) = state.pets.as_ref() {
        if pets.next_free_exploration_sec_remaining.unwrap_or(0) <= 0 {
            if let Some(h) = pets
                .habitats
                .iter()
                .find(|h| !h.is_finished && h.strongest_pet_id.is_some())
            {
                return Some(HeuristicPick {
                    action: Action::FightPetHabitat {
                        habitat: h.name.into(),
                    },
                    reason: "heuristic: pet habitat exploration free + has pet to send",
                });
            }
        }
    }

    // 5f. Pets: daily PvP if we have a clear advantage (my-habitat total level
    //     > opponent's level_total × 1.2).
    if let Some(pets) = state.pets.as_ref() {
        if let Some(o) = pets.opponent.as_ref() {
            if o.next_free_battle_sec_remaining.unwrap_or(0) <= 0 {
                if let Some(habitat) = o.habitat {
                    if let Some(h) = pets.habitats.iter().find(|h| h.name == habitat) {
                        if !h.battled_opponent_today {
                            let my_strength =
                                (h.strongest_pet_level as u32) * 20u32; // rough proxy
                            if my_strength > (o.level_total as f32 * 1.2) as u32 {
                                return Some(HeuristicPick {
                                    action: Action::FightPetOpponent {
                                        habitat: habitat.into(),
                                    },
                                    reason: "heuristic: pet PvP opponent set, clear advantage",
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 6. Dungeons: free fight on a winnable target (regular dungeons).
    if state.dungeons.off_cooldown && state.dungeons.best_winnable_name.is_some() {
        return Some(HeuristicPick {
            action: Action::FightDungeon,
            reason: "heuristic: free dungeon fight on winnable target",
        });
    }

    // 6a. Tower (separate command, shares the dungeon cooldown).
    if state.dungeons.off_cooldown {
        if let Some(t) = state.dungeons.tower.as_ref() {
            if t.winnable {
                return Some(HeuristicPick {
                    action: Action::FightTower,
                    reason: "heuristic: tower floor winnable + dungeon cooldown free",
                });
            }
        }
    }

    // 6a2. Hellevator event handling (gated on status()).
    match state.hellevator.status {
        "reward_claimable" => {
            return Some(HeuristicPick {
                action: Action::HellevatorClaimFinal,
                reason: "heuristic: hellevator event ended — claim final reward",
            });
        }
        "not_entered" => {
            return Some(HeuristicPick {
                action: Action::HellevatorEnter,
                reason: "heuristic: hellevator event live — enter",
            });
        }
        "active" => {
            if state.hellevator.daily_claimable {
                return Some(HeuristicPick {
                    action: Action::HellevatorClaimDaily,
                    reason: "heuristic: hellevator daily reward claimable",
                });
            }
            if state.hellevator.daily_yesterday_claimable {
                return Some(HeuristicPick {
                    action: Action::HellevatorClaimDailyYesterday,
                    reason: "heuristic: hellevator yesterday reward claimable",
                });
            }
            if state.hellevator.key_cards > 0 {
                return Some(HeuristicPick {
                    action: Action::HellevatorFight,
                    reason: "heuristic: hellevator key card available — fight",
                });
            }
        }
        _ => {}
    }

    // 6b. Personal demon portal (daily, unlocks at char lvl 99).
    if let Some(p) = state.dungeons.portal.as_ref() {
        if p.can_fight && p.enemy_hp_percentage > 0 {
            return Some(HeuristicPick {
                action: Action::FightPortal,
                reason: "heuristic: daily portal fight available",
            });
        }
    }

    // 7. Arena fight.
    if state.arena.off_cooldown && !state.arena.enemy_ids.is_empty() {
        return Some(HeuristicPick {
            action: Action::FightArena,
            reason: "heuristic: arena off cooldown + opponents visible",
        });
    }

    // 7b. Housekeeping: trim inbox once it crosses 80% of capacity.
    if state.mail.inbox_capacity > 0
        && state.mail.inbox_total * 10 >= state.mail.inbox_capacity as usize * 8
    {
        return Some(HeuristicPick {
            action: Action::DeleteAllMail,
            reason: "heuristic: inbox ≥ 80% full — delete all",
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
