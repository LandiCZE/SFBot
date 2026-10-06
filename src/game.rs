use chrono::Local;
use serde::Serialize;
use sf_api::command::{AttributeType, ExpeditionSetting, ShopType};
use sf_api::gamestate::GameState;
use sf_api::gamestate::character::{Character, Class};
use sf_api::gamestate::dungeons::{Dungeon, DungeonProgress, LightDungeon, ShadowDungeon};
use sf_api::gamestate::fortress::{FortressBuildingType, FortressResourceType, FortressUnitType};
use sf_api::gamestate::items::{EquipmentSlot, Item, ItemType, PotionSize, PotionType};
use sf_api::gamestate::social::ClaimableStatus;
use sf_api::gamestate::tavern::{AvailableTasks, CurrentAction, ExpeditionStage};
use sf_api::gamestate::underworld::{UnderworldBuildingType, UnderworldResourceType, UnderworldUnitType};
use sf_api::gamestate::unlockables::{HabitatExploration, HabitatType, HellevatorStatus};
use sf_api::gamestate::legendary_dungeon::LegendaryDungeonStatus;
use sf_api::misc::EnumMapGet;

/// Daily per-pet fruit-feed cap in the official client (approximate).
pub const PET_DAILY_FEED_CAP: u16 = 5;

#[derive(Serialize)]
pub struct StateSummary {
    pub character: CharacterSummary,
    pub tavern: TavernSummary,
    pub arena: ArenaSummary,
    pub dungeons: DungeonsSummary,
    pub blacksmith: Option<BlacksmithSummary>,
    pub fortress: Option<FortressSummary>,
    pub shops: Vec<ShopSummary>,
    pub tasks: TasksSummary,
    pub mail: MailSummary,
    pub pets: Option<PetsSummary>,
    pub underworld: Option<UnderworldSummary>,
    pub hellevator: HellevatorSummary,
    pub legendary_dungeon: LegendaryDungeonSummary,
    pub equipment: Vec<EquippedSummary>,
    pub backpack: Vec<BackpackItemSummary>,
}

#[derive(Serialize)]
pub struct HellevatorSummary {
    pub status: &'static str, // "not_available" | "not_entered" | "active" | "reward_claimable"
    pub key_cards: u32,
    pub current_floor: u32,
    pub points: u32,
    pub daily_claimable: bool,
    pub daily_yesterday_claimable: bool,
}

#[derive(Serialize)]
pub struct LegendaryDungeonSummary {
    pub status: &'static str,
}

#[derive(Serialize)]
pub struct UnderworldSummary {
    pub souls_current: u64,
    pub souls_limit: u64,
    pub resources: Vec<UnderworldResourceBrief>,
    pub buildings: Vec<UnderworldBuildingBrief>,
    pub units: Vec<UnderworldUnitBrief>,
    pub upgrade_in_progress: Option<UnderworldUpgradeInfo>,
    pub lure_level: u16,
    pub lured_today: u16,
}

#[derive(Serialize)]
pub struct UnderworldResourceBrief {
    pub name: &'static str,
    pub current: u64,
    pub limit: u64,
    pub per_hour: u64,
}

#[derive(Serialize)]
pub struct UnderworldBuildingBrief {
    pub name: &'static str,
    pub level: u8,
    pub upgrade_cost_silver: u64,
    pub upgrade_cost_souls: u64,
    pub upgrade_time_sec: u64,
    pub buildable_now: bool,
}

#[derive(Serialize)]
pub struct UnderworldUnitBrief {
    pub name: &'static str,
    pub level: u16,
    pub count: u16,
    pub upgrade_cost_silver: u64,
    pub upgrade_cost_souls: u64,
    pub upgrade_next_level: u16,
}

#[derive(Serialize)]
pub struct UnderworldUpgradeInfo {
    pub target: String,
    pub finishes_in_sec: i64,
}

#[derive(Serialize)]
pub struct PetsSummary {
    pub rank: u32,
    pub honor: u32,
    pub total_collected: u16,
    pub max_pet_level: u16,
    pub next_free_exploration_sec_remaining: Option<i64>,
    pub habitats: Vec<PetHabitatSummary>,
    pub opponent: Option<PetOpponentSummary>,
}

#[derive(Serialize)]
pub struct PetHabitatSummary {
    pub name: &'static str,
    pub fruits_wallet: u16,
    pub fights_won: u32,
    pub is_finished: bool,
    pub next_fight_lvl: Option<u16>,
    pub battled_opponent_today: bool,
    pub hungry_pet_ids: Vec<u32>,
    pub strongest_pet_id: Option<u32>,
    pub strongest_pet_level: u16,
}

#[derive(Serialize)]
pub struct PetOpponentSummary {
    pub pid: u32,
    pub habitat: Option<&'static str>,
    pub next_free_battle_sec_remaining: Option<i64>,
    pub pet_count: u32,
    pub level_total: u32,
}

#[derive(Serialize)]
pub struct CharacterSummary {
    pub name: String,
    pub level: u16,
    pub class: String,
    pub main_attribute: &'static str,
    pub experience: u64,
    pub next_level_xp: u64,
    pub gold: f64,
    pub silver: u64,
    pub mushrooms: u32,
    pub attributes: AttributesSummary,
    pub active_potions: Vec<ActivePotionSummary>,
    pub active_potion_slot_free: bool,
    pub backpack_free_slots: usize,
    pub backpack_total_slots: usize,
}

#[derive(Serialize)]
pub struct AttributesSummary {
    pub strength: AttributeStat,
    pub dexterity: AttributeStat,
    pub intelligence: AttributeStat,
    pub constitution: AttributeStat,
    pub luck: AttributeStat,
}

#[derive(Serialize)]
pub struct AttributeStat {
    pub base: u32,
    pub additions: u32,
    pub times_bought: u32,
    pub next_point_cost_silver: u64,
}

#[derive(Serialize)]
pub struct ActivePotionSummary {
    pub kind: &'static str,
    pub size: &'static str,
    pub expires_in_sec: Option<i64>,
}

#[derive(Serialize)]
pub struct TavernSummary {
    pub thirst_for_adventure_sec: u32,
    pub beer_available: u8,
    pub beer_drunk: u8,
    pub beer_max: u8,
    pub quicksand_glasses: u32,
    pub current_action: String,
    pub busy_until_sec_remaining: Option<i64>,
    pub mode: &'static str,
    pub questing_preference: &'static str,
    pub can_change_questing_preference: bool,
    pub quests: Vec<QuestSummary>,
    pub expeditions: Vec<ExpeditionSummary>,
    pub active_expedition: Option<ActiveExpeditionSummary>,
}

#[derive(Serialize)]
pub struct QuestSummary {
    pub index: u8,
    pub duration_sec: u32,
    pub silver: u32,
    pub experience: u32,
    pub item_reward: Option<ItemBrief>,
}

#[derive(Serialize)]
pub struct ExpeditionSummary {
    pub index: u8,
    pub target: String,
    pub thirst_for_adventure_sec: u32,
    pub special: Option<String>,
}

#[derive(Serialize)]
pub struct ActiveExpeditionSummary {
    pub target: String,
    pub target_current: u8,
    pub target_amount: u8,
    pub current_floor: u8,
    pub heroism: i32,
    pub stage: &'static str,
    pub stage_remaining_sec: Option<i64>,
}

#[derive(Serialize)]
pub struct ArenaSummary {
    pub off_cooldown: bool,
    pub next_free_fight_sec_remaining: Option<i64>,
    pub fights_for_xp_today: u8,
    pub enemy_ids: Vec<u32>,
}

#[derive(Serialize)]
pub struct DungeonsSummary {
    pub off_cooldown: bool,
    pub next_free_fight_sec_remaining: Option<i64>,
    pub available: Vec<DungeonBrief>,
    /// Best winnable target with (enemy_level + SAFE_MARGIN) <= my_level.
    pub best_winnable_name: Option<String>,
    /// Tower is tracked separately — uses Command::FightTower, not FightDungeon.
    pub tower: Option<TowerBrief>,
    /// Personal demon portal (unlocks at char level 99).
    pub portal: Option<PortalBrief>,
}

#[derive(Serialize)]
pub struct TowerBrief {
    pub current_floor: u16,
    pub enemy_level: Option<u16>,
    pub enemy_class: Option<String>,
    pub winnable: bool,
}

#[derive(Serialize)]
pub struct PortalBrief {
    pub can_fight: bool,
    pub finished: u16,
    pub enemy_level: u32,
    pub enemy_hp_percentage: u8,
}

#[derive(Serialize)]
pub struct DungeonBrief {
    pub name: String,
    pub kind: &'static str, // "light" or "shadow"
    pub current_floor: u16,
    pub enemy_level: Option<u16>,
    pub enemy_class: Option<String>,
}

#[derive(Serialize)]
pub struct BlacksmithSummary {
    pub metal: u64,
    pub arcane: u64,
    pub dismantle_left: u8,
}

#[derive(Serialize)]
pub struct ShopSummary {
    pub shop: &'static str, // "weapon" | "magic"
    pub items: Vec<ShopItemBrief>,
}

#[derive(Serialize)]
pub struct ShopItemBrief {
    pub pos: u8,
    pub item: ItemBrief,
    pub price_silver: u32,
    pub target_equipment_slot: Option<&'static str>,
    pub main_stat_delta_vs_equipped: Option<i32>,
    pub can_equip: bool,
}

#[derive(Serialize)]
pub struct TasksSummary {
    pub daily_earned_points: u32,
    pub daily_total_points: u32,
    pub daily_claimable_chests: Vec<u8>, // 0-based indices of chests that can be opened
    pub event_earned_points: u32,
    pub event_total_points: u32,
    pub event_claimable_chests: Vec<u8>,
}

#[derive(Serialize)]
pub struct MailSummary {
    pub inbox_total: usize,
    pub inbox_unread: usize,
    pub inbox_capacity: u16,
    pub claimables_pending: Vec<i64>, // msg_id of claimable mail not yet claimed and not expired
}

#[derive(Serialize)]
pub struct FortressSummary {
    pub honor: u32,
    pub wood_current: u64,
    pub wood_limit: u64,
    pub stone_current: u64,
    pub stone_limit: u64,
    pub experience_current: u64,
    pub experience_limit: u64,
    pub buildings: Vec<FortressBuildingBrief>,
    pub units: Vec<FortressUnitBrief>,
    pub upgrade_in_progress: Option<FortressUpgradeInfo>,
    pub attack_target_pid: Option<u32>,
    pub attack_target_present: bool,
    pub attack_target_soldier_advice: Option<u16>,
    pub attack_reroll_free: bool,
    pub attack_reroll_silver_cost: u64,
}

#[derive(Serialize)]
pub struct FortressUnitBrief {
    pub name: &'static str, // "soldier" | "archer" | "magician"
    pub level: u16,
    pub count: u16,
    pub in_training: u16,
    pub training_finishes_in_sec: Option<i64>,
    pub training_cost_wood: u64,
    pub training_cost_stone: u64,
    pub training_cost_silver: u64,
    pub training_time_sec: u64,
    pub upgrade_cost_wood: u64,
    pub upgrade_cost_stone: u64,
    pub upgrade_next_level: u64,
    pub has_training_building: bool,
}

#[derive(Serialize)]
pub struct FortressBuildingBrief {
    pub name: &'static str,
    pub level: u16,
    pub wood_cost: u64,
    pub stone_cost: u64,
    pub silver_cost: u64,
    pub buildable_now: bool,
}

#[derive(Serialize)]
pub struct FortressUpgradeInfo {
    pub target: String,
    pub finishes_in_sec: i64,
}

#[derive(Serialize)]
pub struct EquippedSummary {
    pub slot: &'static str,
    pub item: ItemBrief,
    pub main_stat_score: u32,
}

#[derive(Serialize)]
pub struct BackpackItemSummary {
    pub slot: usize,
    pub item: ItemBrief,
    pub sell_price_silver: u32,
    pub target_equipment_slot: Option<&'static str>,
    pub main_stat_delta_vs_equipped: Option<i32>,
    pub is_junk: bool,
    pub potion: Option<PotionBrief>,
}

#[derive(Serialize)]
pub struct PotionBrief {
    pub kind: &'static str,
    pub size: &'static str,
}

#[derive(Serialize)]
pub struct ItemBrief {
    pub kind: String,
    pub strength: u32,
    pub dexterity: u32,
    pub intelligence: u32,
    pub constitution: u32,
    pub luck: u32,
    pub armor_or_weapon_val: u32,
}

/// How many levels below us a dungeon enemy must be before we attempt it.
pub const DUNGEON_SAFE_MARGIN: u16 = 5;

impl StateSummary {
    pub fn from_game_state(gs: &GameState) -> Self {
        let ch = &gs.character;
        let main = ch.class.main_attribute();

        let active_potions: Vec<ActivePotionSummary> = ch
            .active_potions
            .iter()
            .filter_map(|slot| slot.as_ref())
            .map(|p| ActivePotionSummary {
                kind: potion_kind_name(p.typ),
                size: potion_size_name(p.size),
                expires_in_sec: p.expires.map(|t| (t - Local::now()).num_seconds()),
            })
            .collect();
        let active_potion_slot_free = ch.active_potions.iter().any(|s| s.is_none());

        let backpack_total_slots = ch.inventory.backpack.len();
        let backpack_filled = ch.inventory.backpack.iter().filter(|s| s.is_some()).count();
        let backpack_free_slots = backpack_total_slots.saturating_sub(backpack_filled);

        let character = CharacterSummary {
            name: ch.name.clone(),
            level: ch.level,
            class: format!("{:?}", ch.class),
            main_attribute: attr_name(main),
            experience: ch.experience,
            next_level_xp: ch.next_level_xp,
            gold: (ch.silver as f64) / 100.0,
            silver: ch.silver,
            mushrooms: ch.mushrooms,
            attributes: AttributesSummary {
                strength: attr_stat(ch, AttributeType::Strength),
                dexterity: attr_stat(ch, AttributeType::Dexterity),
                intelligence: attr_stat(ch, AttributeType::Intelligence),
                constitution: attr_stat(ch, AttributeType::Constitution),
                luck: attr_stat(ch, AttributeType::Luck),
            },
            active_potions,
            active_potion_slot_free,
            backpack_free_slots,
            backpack_total_slots,
        };

        let (current_action, busy_until_sec_remaining) = match &gs.tavern.current_action {
            CurrentAction::Idle => ("idle".to_string(), None),
            CurrentAction::Quest {
                quest_idx,
                busy_until,
            } => (
                format!("quest #{quest_idx}"),
                Some((*busy_until - Local::now()).num_seconds()),
            ),
            CurrentAction::CityGuard { hours, busy_until } => (
                format!("city_guard ({hours}h)"),
                Some((*busy_until - Local::now()).num_seconds()),
            ),
            CurrentAction::Expedition => ("expedition".to_string(), None),
            CurrentAction::Unknown(t) => (
                "unknown".to_string(),
                t.map(|ts| (ts - Local::now()).num_seconds()),
            ),
        };

        let (mode, quests, expeditions) = match gs.tavern.available_tasks() {
            AvailableTasks::Quests(qs) => (
                "quests",
                qs.iter()
                    .enumerate()
                    .map(|(i, q)| QuestSummary {
                        index: i as u8,
                        duration_sec: q.base_length,
                        silver: q.base_silver,
                        experience: q.base_experience,
                        item_reward: q.item.as_ref().map(item_brief),
                    })
                    .collect(),
                Vec::new(),
            ),
            AvailableTasks::Expeditions(es) => (
                "expeditions",
                Vec::new(),
                es.iter()
                    .enumerate()
                    .map(|(i, e)| ExpeditionSummary {
                        index: i as u8,
                        target: format!("{:?}", e.target),
                        thirst_for_adventure_sec: e.thirst_for_adventure_sec,
                        special: e.special.map(|s| format!("{:?}", s)),
                    })
                    .collect(),
            ),
        };

        let active_expedition = gs.tavern.expeditions.active().map(|e| {
            let (stage, remaining) = match e.current_stage() {
                ExpeditionStage::Encounters(_) => ("encounters", None),
                ExpeditionStage::Boss(_) => ("boss", None),
                ExpeditionStage::Rewards(_) => ("rewards", None),
                ExpeditionStage::Waiting { busy_until, .. } => {
                    let secs = (busy_until - Local::now()).num_seconds();
                    ("waiting", Some(secs))
                }
                ExpeditionStage::Finished => ("finished", None),
                ExpeditionStage::Unknown => ("unknown", None),
            };
            ActiveExpeditionSummary {
                target: format!("{:?}", e.target_thing),
                target_current: e.target_current,
                target_amount: e.target_amount,
                current_floor: e.current_floor,
                heroism: e.heroism,
                stage,
                stage_remaining_sec: remaining,
            }
        });

        let questing_preference = match gs.tavern.questing_preference {
            ExpeditionSetting::PreferQuests => "prefer_quests",
            ExpeditionSetting::PreferExpeditions => "prefer_expeditions",
        };

        let tavern = TavernSummary {
            thirst_for_adventure_sec: gs.tavern.thirst_for_adventure_sec,
            beer_available: gs.tavern.beer_max.saturating_sub(gs.tavern.beer_drunk),
            beer_drunk: gs.tavern.beer_drunk,
            beer_max: gs.tavern.beer_max,
            quicksand_glasses: gs.tavern.quicksand_glasses,
            current_action,
            busy_until_sec_remaining,
            mode,
            questing_preference,
            can_change_questing_preference: gs.tavern.can_change_questing_preference(),
            quests,
            expeditions,
            active_expedition,
        };

        let arena = match gs.arena.next_free_fight {
            None => ArenaSummary {
                off_cooldown: true,
                next_free_fight_sec_remaining: None,
                fights_for_xp_today: gs.arena.fights_for_xp,
                enemy_ids: gs.arena.enemy_ids.iter().copied().filter(|&id| id != 0).collect(),
            },
            Some(ts) => {
                let secs = (ts - Local::now()).num_seconds();
                ArenaSummary {
                    off_cooldown: secs <= 0,
                    next_free_fight_sec_remaining: Some(secs),
                    fights_for_xp_today: gs.arena.fights_for_xp,
                    enemy_ids: gs.arena.enemy_ids.iter().copied().filter(|&id| id != 0).collect(),
                }
            }
        };

        let dungeons = dungeons_summary(gs, ch.level);
        let blacksmith = gs.blacksmith.as_ref().map(|b| BlacksmithSummary {
            metal: b.metal,
            arcane: b.arcane,
            dismantle_left: b.dismantle_left,
        });
        let fortress = gs.fortress.as_ref().map(|f| fortress_summary(f, ch.silver, gs));
        let shops = shops_summary(gs, main);
        let tasks = tasks_summary(gs);
        let mail = mail_summary(gs);
        let pets = gs.pets.as_ref().map(pets_summary);
        let underworld = gs
            .underworld
            .as_ref()
            .map(|uw| underworld_summary(uw, ch.silver));
        let hellevator = hellevator_summary(gs);
        let legendary_dungeon = legendary_summary(gs);

        let equipment: Vec<EquippedSummary> = ch
            .equipment
            .0
            .iter()
            .filter_map(|(slot, item_opt)| {
                item_opt.as_ref().map(|item| EquippedSummary {
                    slot: slot_name(slot),
                    item: item_brief(item),
                    main_stat_score: item_main_stat_score(item, main),
                })
            })
            .collect();

        let backpack: Vec<BackpackItemSummary> = ch
            .inventory
            .backpack
            .iter()
            .enumerate()
            .filter_map(|(i, item_opt)| {
                item_opt.as_ref().map(|item| {
                    let target_slot = item.typ.equipment_slot();
                    let delta = target_slot.map(|ts| {
                        let equipped_score = ch
                            .equipment
                            .0
                            .iter()
                            .find(|(slot, _)| *slot == ts)
                            .and_then(|(_, item_opt)| item_opt.as_ref())
                            .map(|it| item_main_stat_score(it, main))
                            .unwrap_or(0);
                        let new_score = item_main_stat_score(item, main);
                        new_score as i32 - equipped_score as i32
                    });
                    let potion = match &item.typ {
                        ItemType::Potion(p) => Some(PotionBrief {
                            kind: potion_kind_name(p.typ),
                            size: potion_size_name(p.size),
                        }),
                        _ => None,
                    };
                    let is_special = matches!(
                        item.typ,
                        ItemType::Potion(_)
                            | ItemType::Scrapbook
                            | ItemType::DungeonKey { .. }
                            | ItemType::HeartOfDarkness
                            | ItemType::WheelOfFortune
                            | ItemType::Mannequin
                            | ItemType::ToiletKey
                            | ItemType::QuickSandGlass
                    );
                    // Treat equals as junk too — otherwise swap-outs after
                    // equipping an upgrade can sit in the backpack forever.
                    let is_junk = !is_special
                        && (target_slot.is_none() || delta.map(|d| d <= 0).unwrap_or(true));
                    BackpackItemSummary {
                        slot: i + 1,
                        item: item_brief(item),
                        sell_price_silver: item.price,
                        target_equipment_slot: target_slot.map(slot_name),
                        main_stat_delta_vs_equipped: delta,
                        is_junk,
                        potion,
                    }
                })
            })
            .collect();

        Self {
            character,
            tavern,
            arena,
            dungeons,
            blacksmith,
            fortress,
            shops,
            tasks,
            mail,
            pets,
            underworld,
            hellevator,
            legendary_dungeon,
            equipment,
            backpack,
        }
    }
}

fn hellevator_summary(gs: &GameState) -> HellevatorSummary {
    let status = match gs.hellevator.status() {
        HellevatorStatus::NotAvailable => "not_available",
        HellevatorStatus::NotEntered => "not_entered",
        HellevatorStatus::RewardClaimable => "reward_claimable",
        HellevatorStatus::Active(_) => "active",
    };
    let (key_cards, current_floor, points, daily_claimable, daily_yesterday_claimable) =
        if let HellevatorStatus::Active(h) = gs.hellevator.status() {
            let dc = h.rewards_today.as_ref().map(|r| r.claimable()).unwrap_or(false);
            let dyc = h
                .rewards_yesterday
                .as_ref()
                .map(|r| r.claimable())
                .unwrap_or(false);
            (h.key_cards, h.current_floor, h.points, dc, dyc)
        } else {
            (0, 0, 0, false, false)
        };
    HellevatorSummary {
        status,
        key_cards,
        current_floor,
        points,
        daily_claimable,
        daily_yesterday_claimable,
    }
}

fn legendary_summary(gs: &GameState) -> LegendaryDungeonSummary {
    let status = match gs.legendary_dungeon.status() {
        LegendaryDungeonStatus::Unavailable => "unavailable",
        LegendaryDungeonStatus::NotEntered(_) => "not_entered",
        LegendaryDungeonStatus::Ended(_) => "ended",
        LegendaryDungeonStatus::DoorSelect { .. } => "door_select",
        LegendaryDungeonStatus::PickGem { .. } => "pick_gem",
        LegendaryDungeonStatus::Healing { .. } => "healing",
        LegendaryDungeonStatus::Room { .. } => "room",
        LegendaryDungeonStatus::TakeItem { .. } => "take_item",
        LegendaryDungeonStatus::Unknown => "unknown",
    };
    LegendaryDungeonSummary { status }
}

fn underworld_summary(
    uw: &sf_api::gamestate::underworld::Underworld,
    silver: u64,
) -> UnderworldSummary {
    let buildings_order = [
        UnderworldBuildingType::HeartOfDarkness,
        UnderworldBuildingType::Gate,
        UnderworldBuildingType::GoldPit,
        UnderworldBuildingType::SoulExtractor,
        UnderworldBuildingType::GoblinPit,
        UnderworldBuildingType::TortureChamber,
        UnderworldBuildingType::GladiatorTrainer,
        UnderworldBuildingType::TrollBlock,
        UnderworldBuildingType::Adventuromatic,
        UnderworldBuildingType::Keeper,
    ];
    let no_upgrade_in_progress = uw.upgrade_building.is_none();
    let buildings: Vec<UnderworldBuildingBrief> = buildings_order
        .iter()
        .map(|&bt| {
            let b = uw.buildings.get(bt);
            let affordable =
                b.upgrade_cost.silver <= silver && b.upgrade_cost.souls <= uw.souls_current;
            UnderworldBuildingBrief {
                name: underworld_building_name(bt),
                level: b.level,
                upgrade_cost_silver: b.upgrade_cost.silver,
                upgrade_cost_souls: b.upgrade_cost.souls,
                upgrade_time_sec: b.upgrade_cost.time.as_secs(),
                buildable_now: no_upgrade_in_progress && affordable,
            }
        })
        .collect();

    let units: Vec<UnderworldUnitBrief> = [
        UnderworldUnitType::Goblin,
        UnderworldUnitType::Troll,
        UnderworldUnitType::Keeper,
    ]
    .iter()
    .map(|&ut| {
        let u = uw.units.get(ut);
        UnderworldUnitBrief {
            name: underworld_unit_name(ut),
            level: u.level,
            count: u.count,
            upgrade_cost_silver: u.upgrade_cost.silver,
            upgrade_cost_souls: u.upgrade_cost.souls,
            upgrade_next_level: u.upgrade_next_lvl,
        }
    })
    .collect();

    let resources_order = [
        UnderworldResourceType::Souls,
        UnderworldResourceType::Silver,
        UnderworldResourceType::ThirstForAdventure,
    ];
    let resources: Vec<UnderworldResourceBrief> = resources_order
        .iter()
        .map(|&rt| {
            let p = uw.production.get(rt);
            UnderworldResourceBrief {
                name: underworld_resource_name(rt),
                current: p.last_collectable,
                limit: p.limit,
                per_hour: p.per_hour,
            }
        })
        .collect();

    let upgrade_in_progress = uw.upgrade_building.map(|target| UnderworldUpgradeInfo {
        target: underworld_building_name(target).to_string(),
        finishes_in_sec: uw
            .upgrade_finish
            .map(|t| (t - Local::now()).num_seconds())
            .unwrap_or(0),
    });

    UnderworldSummary {
        souls_current: uw.souls_current,
        souls_limit: uw.souls_limit,
        resources,
        buildings,
        units,
        upgrade_in_progress,
        lure_level: uw.lure_level,
        lured_today: uw.lured_today,
    }
}

pub fn underworld_building_name(bt: UnderworldBuildingType) -> &'static str {
    match bt {
        UnderworldBuildingType::HeartOfDarkness => "heart_of_darkness",
        UnderworldBuildingType::Gate => "gate",
        UnderworldBuildingType::GoldPit => "gold_pit",
        UnderworldBuildingType::SoulExtractor => "soul_extractor",
        UnderworldBuildingType::GoblinPit => "goblin_pit",
        UnderworldBuildingType::TortureChamber => "torture_chamber",
        UnderworldBuildingType::GladiatorTrainer => "gladiator_trainer",
        UnderworldBuildingType::TrollBlock => "troll_block",
        UnderworldBuildingType::Adventuromatic => "adventuromatic",
        UnderworldBuildingType::Keeper => "keeper",
    }
}

pub fn underworld_building_from_name(n: &str) -> Option<UnderworldBuildingType> {
    Some(match n {
        "heart_of_darkness" => UnderworldBuildingType::HeartOfDarkness,
        "gate" => UnderworldBuildingType::Gate,
        "gold_pit" => UnderworldBuildingType::GoldPit,
        "soul_extractor" => UnderworldBuildingType::SoulExtractor,
        "goblin_pit" => UnderworldBuildingType::GoblinPit,
        "torture_chamber" => UnderworldBuildingType::TortureChamber,
        "gladiator_trainer" => UnderworldBuildingType::GladiatorTrainer,
        "troll_block" => UnderworldBuildingType::TrollBlock,
        "adventuromatic" => UnderworldBuildingType::Adventuromatic,
        "keeper" => UnderworldBuildingType::Keeper,
        _ => return None,
    })
}

pub fn underworld_unit_name(ut: UnderworldUnitType) -> &'static str {
    match ut {
        UnderworldUnitType::Goblin => "goblin",
        UnderworldUnitType::Troll => "troll",
        UnderworldUnitType::Keeper => "keeper",
    }
}

pub fn underworld_unit_from_name(n: &str) -> Option<UnderworldUnitType> {
    Some(match n {
        "goblin" => UnderworldUnitType::Goblin,
        "troll" => UnderworldUnitType::Troll,
        "keeper" => UnderworldUnitType::Keeper,
        _ => return None,
    })
}

pub fn underworld_resource_name(rt: UnderworldResourceType) -> &'static str {
    match rt {
        UnderworldResourceType::Souls => "souls",
        UnderworldResourceType::Silver => "silver",
        UnderworldResourceType::ThirstForAdventure => "thirst_for_adventure",
    }
}

pub fn underworld_resource_from_name(n: &str) -> Option<UnderworldResourceType> {
    Some(match n {
        "souls" => UnderworldResourceType::Souls,
        "silver" => UnderworldResourceType::Silver,
        "thirst_for_adventure" => UnderworldResourceType::ThirstForAdventure,
        _ => return None,
    })
}

fn pets_summary(p: &sf_api::gamestate::unlockables::Pets) -> PetsSummary {
    let next_free_exploration_sec_remaining = p
        .next_free_exploration
        .map(|t| (t - Local::now()).num_seconds());

    let habitats: Vec<PetHabitatSummary> = [
        HabitatType::Shadow,
        HabitatType::Light,
        HabitatType::Earth,
        HabitatType::Fire,
        HabitatType::Water,
    ]
    .iter()
    .map(|&h| {
        let hab = p.habitats.get(h);
        let (is_finished, next_fight_lvl, fights_won) = match hab.exploration {
            HabitatExploration::Finished => (true, None, 20),
            HabitatExploration::Exploring { fights_won, next_fight_lvl } => {
                (false, Some(next_fight_lvl), fights_won)
            }
        };
        let hungry_pet_ids: Vec<u32> = hab
            .pets
            .iter()
            .filter(|pet| pet.level > 0 && pet.level < p.max_pet_level && pet.fruits_today < PET_DAILY_FEED_CAP)
            .map(|pet| pet.id)
            .collect();
        let strongest = hab
            .pets
            .iter()
            .filter(|pet| pet.level > 0)
            .max_by_key(|pet| pet.level);
        PetHabitatSummary {
            name: habitat_name(h),
            fruits_wallet: hab.fruits,
            fights_won,
            is_finished,
            next_fight_lvl,
            battled_opponent_today: hab.battled_opponent,
            hungry_pet_ids,
            strongest_pet_id: strongest.map(|pet| pet.id),
            strongest_pet_level: strongest.map(|pet| pet.level).unwrap_or(0),
        }
    })
    .collect();

    let opponent = {
        let o = &p.opponent;
        if o.id == 0 {
            None
        } else {
            Some(PetOpponentSummary {
                pid: o.id,
                habitat: o.habitat.map(habitat_name),
                next_free_battle_sec_remaining: o
                    .next_free_battle
                    .map(|t| (t - Local::now()).num_seconds()),
                pet_count: o.pet_count,
                level_total: o.level_total,
            })
        }
    };

    PetsSummary {
        rank: p.rank,
        honor: p.honor,
        total_collected: p.total_collected,
        max_pet_level: p.max_pet_level,
        next_free_exploration_sec_remaining,
        habitats,
        opponent,
    }
}

pub fn habitat_name(h: HabitatType) -> &'static str {
    match h {
        HabitatType::Shadow => "shadow",
        HabitatType::Light => "light",
        HabitatType::Earth => "earth",
        HabitatType::Fire => "fire",
        HabitatType::Water => "water",
    }
}

pub fn habitat_from_name(n: &str) -> Option<HabitatType> {
    Some(match n {
        "shadow" => HabitatType::Shadow,
        "light" => HabitatType::Light,
        "earth" => HabitatType::Earth,
        "fire" => HabitatType::Fire,
        "water" => HabitatType::Water,
        _ => return None,
    })
}

fn shops_summary(gs: &GameState, main: AttributeType) -> Vec<ShopSummary> {
    let class = gs.character.class;
    let equipment = &gs.character.equipment;
    [ShopType::Weapon, ShopType::Magic]
        .iter()
        .map(|&st| {
            let shop = gs.shops.get(st);
            let shop_name = match st {
                ShopType::Weapon => "weapon",
                ShopType::Magic => "magic",
            };
            let items: Vec<ShopItemBrief> = shop
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    // Skip placeholder items the game uses for empty slots.
                    if item.price == 0 || item.price == u32::MAX {
                        return None;
                    }
                    let target_slot = item.typ.equipment_slot();
                    let delta = target_slot.map(|ts| {
                        let equipped_score = equipment
                            .0
                            .iter()
                            .find(|(slot, _)| *slot == ts)
                            .and_then(|(_, it)| it.as_ref())
                            .map(|it| item_main_stat_score(it, main))
                            .unwrap_or(0);
                        let new_score = item_main_stat_score(item, main);
                        new_score as i32 - equipped_score as i32
                    });
                    Some(ShopItemBrief {
                        pos: i as u8,
                        item: item_brief(item),
                        price_silver: item.price,
                        target_equipment_slot: target_slot.map(slot_name),
                        main_stat_delta_vs_equipped: delta,
                        can_equip: item.can_be_equipped_by(class),
                    })
                })
                .collect();
            ShopSummary {
                shop: shop_name,
                items,
            }
        })
        .collect()
}

fn tasks_summary(gs: &GameState) -> TasksSummary {
    let tasks = &gs.specials.tasks;
    let daily_earned = tasks.daily.earned_points();
    let daily_total = tasks.daily.total_points();
    let event_earned = tasks.event.earned_points();
    let event_total = tasks.event.total_points();
    let daily_claimable: Vec<u8> = (0..3u8)
        .filter(|&pos| tasks.daily.can_open_chest(pos as usize))
        .collect();
    let event_claimable: Vec<u8> = (0..3u8)
        .filter(|&pos| tasks.event.can_open_chest(pos as usize))
        .collect();
    TasksSummary {
        daily_earned_points: daily_earned,
        daily_total_points: daily_total,
        daily_claimable_chests: daily_claimable,
        event_earned_points: event_earned,
        event_total_points: event_total,
        event_claimable_chests: event_claimable,
    }
}

fn mail_summary(gs: &GameState) -> MailSummary {
    let m = &gs.mail;
    let now = Local::now();
    let claimables_pending: Vec<i64> = m
        .claimables
        .iter()
        .filter(|c| {
            c.status != ClaimableStatus::Claimed
                && c.claimable_until.map(|t| t > now).unwrap_or(true)
        })
        .map(|c| c.msg_id)
        .collect();
    let unread = m.inbox.iter().filter(|e| !e.read).count();
    MailSummary {
        inbox_total: m.inbox.len(),
        inbox_unread: unread,
        inbox_capacity: m.inbox_capacity,
        claimables_pending,
    }
}

fn dungeons_summary(gs: &GameState, my_level: u16) -> DungeonsSummary {
    let (off_cooldown, remaining) = match gs.dungeons.next_free_fight {
        None => (true, None),
        Some(t) => {
            let secs = (t - Local::now()).num_seconds();
            (secs <= 0, Some(secs))
        }
    };

    let mut available: Vec<DungeonBrief> = Vec::new();
    let tower = {
        let prog = gs.dungeons.light.get(LightDungeon::Tower);
        match prog {
            DungeonProgress::Open { finished } => {
                let d = Dungeon::Light(LightDungeon::Tower);
                let enemy = gs.dungeons.current_enemy(d);
                let winnable = enemy
                    .map(|m| m.level.saturating_add(DUNGEON_SAFE_MARGIN) <= my_level)
                    .unwrap_or(false);
                Some(TowerBrief {
                    current_floor: *finished,
                    enemy_level: enemy.map(|m| m.level),
                    enemy_class: enemy.map(|m| format!("{:?}", m.class)),
                    winnable,
                })
            }
            _ => None,
        }
    };

    let portal = gs.dungeons.portal.as_ref().map(|p| PortalBrief {
        can_fight: p.can_fight,
        finished: p.finished,
        enemy_level: p.enemy_level,
        enemy_hp_percentage: p.enemy_hp_percentage,
    });

    for (dkey, prog) in gs.dungeons.light.iter() {
        if let DungeonProgress::Open { finished } = prog {
            if dkey == LightDungeon::Tower {
                // Tower is reported separately — see `tower` above.
                continue;
            }
            let d = Dungeon::Light(dkey);
            let enemy = gs.dungeons.current_enemy(d);
            available.push(DungeonBrief {
                name: format!("{:?}", dkey),
                kind: "light",
                current_floor: *finished,
                enemy_level: enemy.map(|m| m.level),
                enemy_class: enemy.map(|m| format!("{:?}", m.class)),
            });
        }
    }
    for (dkey, prog) in gs.dungeons.shadow.iter() {
        if let DungeonProgress::Open { finished } = prog {
            let d = Dungeon::Shadow(dkey);
            let enemy = gs.dungeons.current_enemy(d);
            available.push(DungeonBrief {
                name: format!("{:?}", dkey),
                kind: "shadow",
                current_floor: *finished,
                enemy_level: enemy.map(|m| m.level),
                enemy_class: enemy.map(|m| format!("{:?}", m.class)),
            });
        }
    }

    let best_winnable_name = available
        .iter()
        .filter(|d| {
            d.enemy_level
                .map(|lv| lv.saturating_add(DUNGEON_SAFE_MARGIN) <= my_level)
                .unwrap_or(false)
        })
        .min_by_key(|d| d.enemy_level.unwrap_or(u16::MAX))
        .map(|d| d.name.clone());

    DungeonsSummary {
        off_cooldown,
        next_free_fight_sec_remaining: remaining,
        available,
        best_winnable_name,
        tower,
        portal,
    }
}

fn fortress_summary(
    f: &sf_api::gamestate::fortress::Fortress,
    silver: u64,
    gs: &GameState,
) -> FortressSummary {
    let wood = f.resources.get(FortressResourceType::Wood);
    let stone = f.resources.get(FortressResourceType::Stone);
    let exp = f.resources.get(FortressResourceType::Experience);

    let buildings: Vec<FortressBuildingBrief> = [
        FortressBuildingType::Fortress,
        FortressBuildingType::LaborersQuarters,
        FortressBuildingType::WoodcuttersHut,
        FortressBuildingType::Quarry,
        FortressBuildingType::GemMine,
        FortressBuildingType::Academy,
        FortressBuildingType::ArcheryGuild,
        FortressBuildingType::Barracks,
        FortressBuildingType::MagesTower,
        FortressBuildingType::Treasury,
        FortressBuildingType::Smithy,
        FortressBuildingType::Wall,
    ]
    .iter()
    .map(|bt| {
        let b = f.buildings.get(*bt);
        FortressBuildingBrief {
            name: fortress_building_name(*bt),
            level: b.level,
            wood_cost: b.upgrade_cost.wood,
            stone_cost: b.upgrade_cost.stone,
            silver_cost: b.upgrade_cost.silver,
            buildable_now: f.can_build(*bt, silver),
        }
    })
    .collect();

    let upgrade_in_progress = f.building_upgrade.target.map(|target| {
        let finishes_in = f
            .building_upgrade
            .finish
            .map(|t| (t - Local::now()).num_seconds())
            .unwrap_or(0);
        FortressUpgradeInfo {
            target: fortress_building_name(target).to_string(),
            finishes_in_sec: finishes_in,
        }
    });

    let reroll_free = f
        .attack_free_reroll
        .map(|t| t <= Local::now())
        .unwrap_or(true);

    let units: Vec<FortressUnitBrief> = [
        FortressUnitType::Soldier,
        FortressUnitType::Magician,
        FortressUnitType::Archer,
    ]
    .iter()
    .map(|&ut| {
        let u = f.units.get(ut);
        let training_building = fortress_unit_training_building(ut);
        let has_building = f.buildings.get(training_building).level > 0;
        let training_finishes_in_sec = u
            .training
            .finish
            .map(|t| (t - Local::now()).num_seconds());
        FortressUnitBrief {
            name: fortress_unit_name(ut),
            level: u.level,
            count: u.count,
            in_training: u.in_training,
            training_finishes_in_sec,
            training_cost_wood: u.training.cost.wood,
            training_cost_stone: u.training.cost.stone,
            training_cost_silver: u.training.cost.silver,
            training_time_sec: u.training.cost.time.as_secs(),
            upgrade_cost_wood: u.upgrade_cost.wood,
            upgrade_cost_stone: u.upgrade_cost.stone,
            upgrade_next_level: u.upgrade_next_lvl,
            has_training_building: has_building,
        }
    })
    .collect();

    let attack_target_pid = f.attack_target;
    let attack_target_soldier_advice = attack_target_pid.and_then(|pid| {
        gs.lookup
            .lookup_pid(pid)
            .and_then(|op| op.fortress.as_ref())
            .map(|of| of.soldier_advice)
    });

    FortressSummary {
        honor: f.honor,
        wood_current: wood.current,
        wood_limit: wood.limit,
        stone_current: stone.current,
        stone_limit: stone.limit,
        experience_current: exp.current,
        experience_limit: exp.limit,
        buildings,
        units,
        upgrade_in_progress,
        attack_target_pid,
        attack_target_present: f.attack_target.is_some(),
        attack_target_soldier_advice,
        attack_reroll_free: reroll_free,
        attack_reroll_silver_cost: f.opponent_reroll_price,
    }
}

pub fn fortress_unit_name(ut: FortressUnitType) -> &'static str {
    match ut {
        FortressUnitType::Soldier => "soldier",
        FortressUnitType::Magician => "magician",
        FortressUnitType::Archer => "archer",
    }
}

pub fn fortress_unit_from_name(n: &str) -> Option<FortressUnitType> {
    Some(match n {
        "soldier" => FortressUnitType::Soldier,
        "magician" => FortressUnitType::Magician,
        "archer" => FortressUnitType::Archer,
        _ => return None,
    })
}

fn fortress_unit_training_building(ut: FortressUnitType) -> FortressBuildingType {
    match ut {
        FortressUnitType::Soldier => FortressBuildingType::Barracks,
        FortressUnitType::Magician => FortressBuildingType::MagesTower,
        FortressUnitType::Archer => FortressBuildingType::ArcheryGuild,
    }
}

pub fn fortress_building_name(bt: FortressBuildingType) -> &'static str {
    match bt {
        FortressBuildingType::Fortress => "fortress",
        FortressBuildingType::LaborersQuarters => "laborers_quarters",
        FortressBuildingType::WoodcuttersHut => "woodcutters_hut",
        FortressBuildingType::Quarry => "quarry",
        FortressBuildingType::GemMine => "gem_mine",
        FortressBuildingType::Academy => "academy",
        FortressBuildingType::ArcheryGuild => "archery_guild",
        FortressBuildingType::Barracks => "barracks",
        FortressBuildingType::MagesTower => "mages_tower",
        FortressBuildingType::Treasury => "treasury",
        FortressBuildingType::Smithy => "smithy",
        FortressBuildingType::Wall => "wall",
    }
}

pub fn fortress_building_from_name(n: &str) -> Option<FortressBuildingType> {
    Some(match n {
        "fortress" => FortressBuildingType::Fortress,
        "laborers_quarters" => FortressBuildingType::LaborersQuarters,
        "woodcutters_hut" => FortressBuildingType::WoodcuttersHut,
        "quarry" => FortressBuildingType::Quarry,
        "gem_mine" => FortressBuildingType::GemMine,
        "academy" => FortressBuildingType::Academy,
        "archery_guild" => FortressBuildingType::ArcheryGuild,
        "barracks" => FortressBuildingType::Barracks,
        "mages_tower" => FortressBuildingType::MagesTower,
        "treasury" => FortressBuildingType::Treasury,
        "smithy" => FortressBuildingType::Smithy,
        "wall" => FortressBuildingType::Wall,
        _ => return None,
    })
}

pub fn fortress_resource_from_name(n: &str) -> Option<FortressResourceType> {
    Some(match n {
        "wood" => FortressResourceType::Wood,
        "stone" => FortressResourceType::Stone,
        "experience" => FortressResourceType::Experience,
        _ => return None,
    })
}

pub fn find_light_dungeon(name: &str) -> Option<LightDungeon> {
    use strum::IntoEnumIterator;
    LightDungeon::iter().find(|d| format!("{:?}", d) == name)
}

pub fn find_shadow_dungeon(name: &str) -> Option<ShadowDungeon> {
    use strum::IntoEnumIterator;
    ShadowDungeon::iter().find(|d| format!("{:?}", d) == name)
}

fn item_main_stat_score(item: &Item, main: AttributeType) -> u32 {
    let m = *item.attributes.get(main);
    let c = *item.attributes.get(AttributeType::Constitution);
    let base = m * 2 + c;
    let weapon_bonus = match &item.typ {
        sf_api::gamestate::items::ItemType::Weapon { min_dmg, max_dmg } => min_dmg + max_dmg,
        _ => 0,
    };
    base + weapon_bonus
}

pub fn next_point_cost(class: Class, attribute: AttributeType, times_bought: u32, level: u16) -> u64 {
    let mult = attribute_cost_multiplier(class, attribute);
    let n = times_bought as u64;
    (n * n * n * mult + 25) * level as u64
}

fn attribute_cost_multiplier(class: Class, attribute: AttributeType) -> u64 {
    let main = class.main_attribute();
    match attribute {
        a if a == main => 1,
        AttributeType::Constitution => 1,
        AttributeType::Luck => 5,
        _ => 2,
    }
}

fn attr_name(a: AttributeType) -> &'static str {
    match a {
        AttributeType::Strength => "strength",
        AttributeType::Dexterity => "dexterity",
        AttributeType::Intelligence => "intelligence",
        AttributeType::Constitution => "constitution",
        AttributeType::Luck => "luck",
    }
}

fn attr_stat(ch: &Character, a: AttributeType) -> AttributeStat {
    let times_bought = *ch.attribute_times_bought.get(a);
    AttributeStat {
        base: *ch.attribute_basis.get(a),
        additions: *ch.attribute_additions.get(a),
        times_bought,
        next_point_cost_silver: next_point_cost(ch.class, a, times_bought, ch.level),
    }
}

fn item_brief(item: &Item) -> ItemBrief {
    use sf_api::command::AttributeType::*;
    let a = &item.attributes;
    ItemBrief {
        kind: format!("{:?}", item.typ),
        strength: *a.get(Strength),
        dexterity: *a.get(Dexterity),
        intelligence: *a.get(Intelligence),
        constitution: *a.get(Constitution),
        luck: *a.get(Luck),
        armor_or_weapon_val: item.type_specific_val,
    }
}

fn slot_name(s: EquipmentSlot) -> &'static str {
    match s {
        EquipmentSlot::Hat => "hat",
        EquipmentSlot::BreastPlate => "chest",
        EquipmentSlot::Gloves => "gloves",
        EquipmentSlot::FootWear => "boots",
        EquipmentSlot::Amulet => "amulet",
        EquipmentSlot::Belt => "belt",
        EquipmentSlot::Ring => "ring",
        EquipmentSlot::Talisman => "talisman",
        EquipmentSlot::Weapon => "weapon",
        EquipmentSlot::Shield => "offhand",
    }
}

pub fn potion_kind_name(t: PotionType) -> &'static str {
    match t {
        PotionType::Strength => "strength",
        PotionType::Dexterity => "dexterity",
        PotionType::Intelligence => "intelligence",
        PotionType::Constitution => "constitution",
        PotionType::Luck => "luck",
        PotionType::EternalLife => "eternal_life",
    }
}

pub fn potion_size_name(s: PotionSize) -> &'static str {
    match s {
        PotionSize::Small => "small",
        PotionSize::Medium => "medium",
        PotionSize::Large => "large",
    }
}
