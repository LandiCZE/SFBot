use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use rand::Rng;
use sf_api::command::{BlacksmithAction, Command, ExpeditionSetting, ShopType};
use sf_api::gamestate::dungeons::Dungeon;
use sf_api::gamestate::items::{ItemPosition, PlayerItemPosition};
use sf_api::gamestate::tavern::ExpeditionStage;
use sf_api::session::SimpleSession;
use sf_api::gamestate::fortress::FortressUnitType;
use std::collections::VecDeque;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing_subscriber::EnvFilter;

mod actions;
mod brain;
mod game;
mod log;
mod strategy;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = dotenvy::dotenv() {
        if !matches!(&e, dotenvy::Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound) {
            eprintln!("warning: .env load failed: {e}");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env()?;
    let mut session = login(&cfg).await?;

    let http = reqwest::Client::builder()
        .user_agent("sf-bot/0.1")
        .timeout(Duration::from_secs(30))
        .build()?;

    let log_path = PathBuf::from("decisions.jsonl");
    let mut recent: VecDeque<log::DecisionLog> = VecDeque::with_capacity(6);
    let mut last_claude_at: Option<Instant> = None;
    let mut last_dungeon_refresh: Option<Instant> = None;
    let started_at = Instant::now();

    tracing::info!(
        dry_run = cfg.dry_run,
        max_cycles = cfg.max_cycles,
        run_seconds = cfg.run_seconds,
        claude_min_interval_sec = cfg.claude_min_interval.as_secs(),
        "starting core loop"
    );

    let run_deadline = cfg
        .run_seconds
        .map(|s| started_at + Duration::from_secs(s));

    let mut cycle = 0usize;
    loop {
        cycle += 1;
        if cycle > cfg.max_cycles {
            tracing::info!("hit MAX_CYCLES={}", cfg.max_cycles);
            break;
        }
        if let Some(deadline) = run_deadline {
            if Instant::now() >= deadline {
                tracing::info!(
                    "hit RUN_SECONDS deadline after {} min",
                    started_at.elapsed().as_secs() / 60
                );
                break;
            }
        }
        tracing::info!(
            "--- cycle {cycle} (elapsed {} min) ---",
            started_at.elapsed().as_secs() / 60
        );

        if let Err(e) = session.send_command(Command::Update).await {
            tracing::warn!("Update failed: {e:#} — retry after backoff");
            sleep(Duration::from_secs(10)).await;
            continue;
        }

        // UpdateDungeons every ~5 min — Update doesn't refresh dungeon progress.
        let needs_dungeon_refresh = last_dungeon_refresh
            .map(|t| t.elapsed() > Duration::from_secs(300))
            .unwrap_or(true);
        if needs_dungeon_refresh {
            if let Err(e) = session.send_command(Command::UpdateDungeons).await {
                tracing::debug!("UpdateDungeons failed: {e:#}");
            } else {
                last_dungeon_refresh = Some(Instant::now());
            }
        }

        // Fortress autopilot: finish an upgrade whose timer has elapsed. Server
        // requires an explicit FortressBuildFinish{mushrooms:0} even when the
        // timer is done — Update won't credit it on its own.
        if let Some(gs) = session.game_state() {
            if let Some(f) = gs.fortress.as_ref() {
                if let (Some(target), Some(finish)) = (f.building_upgrade.target, f.building_upgrade.finish) {
                    if finish <= chrono::Local::now() && !cfg.dry_run {
                        if let Err(e) = session
                            .send_command(Command::FortressBuildFinish {
                                f_type: target,
                                mushrooms: 0,
                            })
                            .await
                        {
                            tracing::warn!("FortressBuildFinish failed: {e:#}");
                        } else {
                            tracing::info!(?target, "fortress build finished");
                        }
                    }
                }
            }
        }

        // Legendary dungeon autopilot: walk through the state machine. All
        // decisions are "always first option" except Healing (don't touch —
        // the server auto-charges mushrooms for a heal) and TakeItem (needs
        // inventory logic we don't implement yet).
        let legendary_action = {
            use sf_api::gamestate::legendary_dungeon::{
                LegendaryDungeonStatus, RoomEncounter, RoomStatus, RoomType,
            };
            let gs_opt = session.game_state();
            gs_opt.and_then(|gs| match gs.legendary_dungeon.status() {
                LegendaryDungeonStatus::Room {
                    status: RoomStatus::Entered,
                    encounter,
                    typ,
                    ..
                } => match encounter {
                    RoomEncounter::Monster(_) => Some(Command::LegendaryDungeonMonsterFight),
                    _ if matches!(typ, RoomType::Generic | RoomType::Encounter) => {
                        Some(Command::LegendaryDungeonEncounterInteract)
                    }
                    _ => Some(Command::LegendaryDungeonRoomInteract),
                },
                LegendaryDungeonStatus::Room {
                    status: RoomStatus::Finished,
                    ..
                } => Some(Command::LegendaryDungeonForcedContinue),
                LegendaryDungeonStatus::DoorSelect { doors, .. } => doors.first().map(|d| {
                    Command::LegendaryDungeonPickDoor { pos: 0, typ: d.typ }
                }),
                LegendaryDungeonStatus::PickGem { available_gems, .. } => {
                    available_gems.first().map(|g| Command::LegendaryDungeonPickGem {
                        gem_type: g.typ,
                    })
                }
                // Healing: do nothing. Any interact here could cost mushrooms.
                LegendaryDungeonStatus::Healing { .. } => None,
                // Everything else — not_entered/take_item/unknown/unavailable/ended — skip.
                _ => None,
            })
        };
        if let Some(cmd) = legendary_action {
            if !cfg.dry_run {
                if let Err(e) = session.send_command(cmd).await {
                    tracing::debug!("legendary dungeon autopilot: {e:#}");
                } else {
                    tracing::info!("legendary dungeon: progressed one step");
                }
            }
        }

        // Underworld autopilot: finish an upgrade whose timer has elapsed.
        if let Some(gs) = session.game_state() {
            if let Some(uw) = gs.underworld.as_ref() {
                if let (Some(target), Some(finish)) = (uw.upgrade_building, uw.upgrade_finish) {
                    if finish <= chrono::Local::now() && !cfg.dry_run {
                        if let Err(e) = session
                            .send_command(Command::UnderworldUpgradeFinish {
                                building: target,
                                mushrooms: 0,
                            })
                            .await
                        {
                            tracing::warn!("UnderworldUpgradeFinish failed: {e:#}");
                        } else {
                            tracing::info!(?target, "underworld build finished");
                        }
                    }
                }
            }
        }

        // Fortress attack-target autopilot: if we have a target but haven't
        // seen their OtherFortress info yet, send ViewPlayer so strategy
        // can read soldier_advice. One-shot per target.
        let needs_view_target = {
            let gs = session.game_state();
            match gs {
                Some(gs) => gs.fortress.as_ref().and_then(|f| {
                    f.attack_target.filter(|pid| {
                        gs.lookup
                            .lookup_pid(*pid)
                            .and_then(|op| op.fortress.as_ref())
                            .is_none()
                    })
                }),
                None => None,
            }
        };
        if let Some(target_pid) = needs_view_target {
            if !cfg.dry_run {
                if let Err(e) = session
                    .send_command(Command::ViewPlayer {
                        ident: target_pid.to_string(),
                    })
                    .await
                {
                    tracing::debug!("ViewPlayer for fortress target failed: {e:#}");
                } else {
                    tracing::debug!(?target_pid, "fetched fortress target info");
                }
            }
        }

        // Expedition autopilot: if an expedition is active, drive it ourselves.
        // Claude only picks WHEN to start one.
        let exp_handled = handle_active_expedition(&mut session, run_deadline, cfg.dry_run).await;
        match exp_handled {
            ExpOutcome::Continue => continue,
            ExpOutcome::Idle => {}
            ExpOutcome::Error(e) => {
                tracing::warn!("expedition autopilot error: {e:#} — backing off");
                sleep(Duration::from_secs(10)).await;
                continue;
            }
        }

        let state = {
            let gs = session
                .game_state()
                .ok_or_else(|| anyhow!("game_state() is None after Update"))?;
            game::StateSummary::from_game_state(gs)
        };

        if let Some(rem_sec) = state.tavern.busy_until_sec_remaining {
            if rem_sec > 0 {
                let jitter = { rand::rng().random_range(5..=40) };
                let wait = rem_sec as u64 + jitter;
                tracing::info!(
                    current = %state.tavern.current_action,
                    rem_sec,
                    jitter,
                    "busy — sleeping"
                );
                sleep_bounded(Duration::from_secs(wait), run_deadline).await;
                continue;
            }
            if state.tavern.current_action.starts_with("quest") {
                if cfg.dry_run {
                    tracing::info!("[dry-run] would FinishQuest");
                } else if let Err(e) = session
                    .send_command(Command::FinishQuest { skip: None })
                    .await
                {
                    tracing::warn!("FinishQuest failed: {e:#}");
                    sleep(Duration::from_secs(10)).await;
                } else {
                    tracing::info!("collected quest reward");
                }
                continue;
            }
            if state.tavern.current_action.starts_with("city_guard") {
                if cfg.dry_run {
                    tracing::info!("[dry-run] would FinishWork");
                } else if let Err(e) = session.send_command(Command::FinishWork).await {
                    tracing::warn!("FinishWork failed: {e:#}");
                    sleep(Duration::from_secs(10)).await;
                } else {
                    tracing::info!("collected guard pay");
                }
                continue;
            }
        }

        // Heuristic first — covers the mechanical cases cheaply.
        let decision = if let Some(h) = strategy::pick(&state) {
            tracing::info!(
                reason = h.reason,
                "strategy picked action without calling Claude"
            );
            actions::Decision {
                action: h.action,
                reason: h.reason.to_string(),
            }
        } else {
            if let Some(last) = last_claude_at {
                let since = last.elapsed();
                if since < cfg.claude_min_interval {
                    let wait = cfg.claude_min_interval - since;
                    tracing::debug!("rate-limit Claude by {}s", wait.as_secs());
                    sleep_bounded(wait, run_deadline).await;
                }
            }

            let recent_snapshot: Vec<log::DecisionLog> = recent.iter().cloned().collect();
            match brain::decide(
                &http,
                &cfg.anthropic_api_key,
                &cfg.claude_model,
                &state,
                &recent_snapshot,
            )
            .await
            {
                Ok(d) => {
                    last_claude_at = Some(Instant::now());
                    d
                }
                Err(e) => {
                    tracing::warn!("Claude call failed: {e:#}");
                    record_and_log(
                        &log_path,
                        &mut recent,
                        &state,
                        actions::Decision {
                            action: actions::Action::Wait,
                            reason: format!("claude error: {e}"),
                        },
                        Some("claude_error".into()),
                        "fallback_wait",
                        false,
                    )?;
                    sleep_bounded(cfg.claude_min_interval, run_deadline).await;
                    continue;
                }
            }
        };

        let (invalid_reason, result, executed) = match actions::validate(&decision.action, &state)
        {
            Err(e) => (Some(e.clone()), format!("invalid: {e}"), false),
            Ok(()) => {
                if cfg.dry_run {
                    (None, "dry_run".to_string(), false)
                } else {
                    match execute(&mut session, &decision.action).await {
                        Ok(msg) => (None, msg, true),
                        Err(e) => (None, format!("exec_error: {e}"), false),
                    }
                }
            }
        };

        record_and_log(
            &log_path,
            &mut recent,
            &state,
            decision,
            invalid_reason,
            &result,
            executed,
        )?;

        let jitter_ms = { rand::rng().random_range(2000..=6000) };
        sleep_bounded(Duration::from_millis(jitter_ms), run_deadline).await;
    }

    tracing::info!("core loop finished");
    Ok(())
}

enum ExpOutcome {
    Idle,
    Continue,
    Error(anyhow::Error),
}

/// If there's an active expedition, autopilot one step (encounter/boss/reward
/// pick, or sleep through a waiting period). Returns:
///  - Idle: no active expedition, loop should proceed to normal flow.
///  - Continue: progressed one step (or slept), loop should continue to the next cycle.
///  - Error: fell over, caller should back off.
async fn handle_active_expedition(
    session: &mut SimpleSession,
    deadline: Option<Instant>,
    dry_run: bool,
) -> ExpOutcome {
    let stage = {
        let gs = match session.game_state() {
            Some(gs) => gs,
            None => return ExpOutcome::Idle,
        };
        let active_stage = gs.tavern.expeditions.active().map(|e| e.current_stage());
        let server_is_on_expedition = matches!(
            gs.tavern.current_action,
            sf_api::gamestate::tavern::CurrentAction::Expedition
        );
        match active_stage {
            Some(s) => s,
            None if server_is_on_expedition => {
                // Zombie: sf-api filters out Finished expeditions, but the server
                // still thinks we're busy. Send Continue to ack and clear.
                if dry_run {
                    tracing::info!("[dry-run] would ExpeditionContinue to clear zombie");
                } else if let Err(e) = session.send_command(Command::ExpeditionContinue).await {
                    tracing::warn!("zombie-expedition ack Continue failed: {e:#}");
                } else {
                    tracing::info!("zombie expedition — sent Continue to clear");
                }
                sleep(Duration::from_millis(800)).await;
                return ExpOutcome::Continue;
            }
            None => return ExpOutcome::Idle,
        }
    };

    match stage {
        ExpeditionStage::Waiting { busy_until, .. } => {
            let rem = (busy_until - chrono::Local::now()).num_seconds();
            if rem > 0 {
                let jitter = { rand::rng().random_range(5..=40) };
                let wait = rem as u64 + jitter;
                tracing::info!(rem_sec = rem, jitter, "expedition waiting — sleeping");
                sleep_bounded(Duration::from_secs(wait), deadline).await;
            }
            ExpOutcome::Continue
        }
        ExpeditionStage::Encounters(_) => {
            if dry_run {
                tracing::info!("[dry-run] would ExpeditionPickEncounter(pos=0)");
            } else if let Err(e) = session
                .send_command(Command::ExpeditionPickEncounter { pos: 0 })
                .await
            {
                return ExpOutcome::Error(e.into());
            } else {
                tracing::info!("expedition: picked encounter 0");
            }
            // Server rate-limits burst commands; a brief pause avoids sessionid-invalid.
            sleep(Duration::from_millis(800)).await;
            ExpOutcome::Continue
        }
        ExpeditionStage::Boss(_) => {
            if dry_run {
                tracing::info!("[dry-run] would ExpeditionContinue (engage boss)");
            } else if let Err(e) = session.send_command(Command::ExpeditionContinue).await {
                return ExpOutcome::Error(e.into());
            } else {
                tracing::info!("expedition: engaged boss");
            }
            sleep(Duration::from_millis(800)).await;
            ExpOutcome::Continue
        }
        ExpeditionStage::Rewards(_) => {
            if dry_run {
                tracing::info!("[dry-run] would ExpeditionPickReward(pos=0)");
            } else if let Err(e) = session
                .send_command(Command::ExpeditionPickReward { pos: 0 })
                .await
            {
                return ExpOutcome::Error(e.into());
            } else {
                tracing::info!("expedition: picked reward 0");
            }
            sleep(Duration::from_millis(800)).await;
            ExpOutcome::Continue
        }
        ExpeditionStage::Finished => {
            // Server may still consider the character "busy" with the finished
            // expedition until we ack. Send one Continue to clear it; next
            // Update should show active() == None cleanly.
            if dry_run {
                tracing::info!("[dry-run] would ExpeditionContinue to clear Finished");
            } else if let Err(e) = session.send_command(Command::ExpeditionContinue).await {
                tracing::warn!("ack-finished Continue failed: {e:#}");
            } else {
                tracing::info!("expedition finished — sent Continue to ack");
            }
            sleep(Duration::from_millis(800)).await;
            ExpOutcome::Continue
        }
        ExpeditionStage::Unknown => {
            tracing::warn!("expedition stage unknown — Update + continue");
            ExpOutcome::Continue
        }
    }
}

async fn sleep_bounded(dur: Duration, deadline: Option<Instant>) {
    let actual = match deadline {
        None => dur,
        Some(d) => {
            let now = Instant::now();
            if now >= d {
                return;
            }
            dur.min(d - now)
        }
    };
    sleep(actual).await;
}

fn record_and_log(
    path: &std::path::Path,
    recent: &mut VecDeque<log::DecisionLog>,
    state: &game::StateSummary,
    decision: actions::Decision,
    invalid_reason: Option<String>,
    result: &str,
    executed: bool,
) -> Result<()> {
    let entry = log::DecisionLog {
        ts: Utc::now(),
        snapshot: log::Snapshot::from_state(state),
        action: decision.action,
        reason: decision.reason,
        executed,
        result: result.into(),
        invalid_reason,
    };
    println!(
        "[{}] L{} silver={} mush={} mode={} | {} — \"{}\" ({})",
        entry.ts.format("%H:%M:%S"),
        entry.snapshot.level,
        entry.snapshot.silver,
        entry.snapshot.mushrooms,
        entry.snapshot.tavern_mode,
        serde_json::to_string(&entry.action).unwrap_or_else(|_| "?".into()),
        truncate(&entry.reason, 100),
        entry.result,
    );
    log::append(path, &entry)?;
    if recent.len() >= 5 {
        recent.pop_front();
    }
    recent.push_back(entry);
    Ok(())
}

fn shop_label(st: ShopType) -> &'static str {
    match st {
        ShopType::Weapon => "weapon",
        ShopType::Magic => "magic",
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

async fn execute(session: &mut SimpleSession, action: &actions::Action) -> Result<String> {
    use actions::Action;
    match action {
        Action::StartQuest { quest_index } => {
            session
                .send_command(Command::StartQuest {
                    quest_pos: *quest_index as usize,
                    overwrite_inv: true,
                })
                .await?;
            Ok(format!("started quest #{quest_index}"))
        }
        Action::StartExpedition { expedition_index } => {
            session
                .send_command(Command::ExpeditionStart {
                    pos: *expedition_index as usize,
                })
                .await?;
            Ok(format!("started expedition #{expedition_index}"))
        }
        Action::StartGuardWork { hours } => {
            session
                .send_command(Command::StartWork { hours: *hours })
                .await?;
            Ok(format!("started city guard for {hours}h"))
        }
        Action::SetQuestingPreference { prefer_quests } => {
            let value = if *prefer_quests {
                ExpeditionSetting::PreferQuests
            } else {
                ExpeditionSetting::PreferExpeditions
            };
            session
                .send_command(Command::SetQuestsInsteadOfExpeditions { value })
                .await?;
            Ok(format!(
                "set questing_preference to {}",
                if *prefer_quests {
                    "prefer_quests"
                } else {
                    "prefer_expeditions"
                }
            ))
        }
        Action::BuyAttribute { attribute, points } => {
            let sf_attr = attribute.to_sf();
            let mut bought = 0u32;
            for _ in 0..*points {
                let next = {
                    let gs = session
                        .game_state()
                        .ok_or_else(|| anyhow!("game_state missing before buy"))?;
                    *sf_api::misc::EnumMapGet::get(&gs.character.attribute_basis, sf_attr) + 1
                };
                session
                    .send_command(Command::IncreaseAttribute {
                        attribute: sf_attr,
                        increase_to: next,
                    })
                    .await?;
                bought += 1;
            }
            Ok(format!("bought {bought}x {attribute:?}"))
        }
        Action::EquipItem { backpack_slot } => {
            let (from_pos, to_slot, item_ident) = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before equip"))?;
                let (bag_pos, item) = gs
                    .character
                    .inventory
                    .iter()
                    .enumerate()
                    .find_map(|(i, (bp, io))| {
                        io.and_then(|it| if i + 1 == *backpack_slot { Some((bp, it)) } else { None })
                    })
                    .ok_or_else(|| anyhow!("backpack slot {backpack_slot} empty at execute"))?;
                let to_slot = item
                    .typ
                    .equipment_slot()
                    .ok_or_else(|| anyhow!("item is not equipment"))?;
                (PlayerItemPosition::from(bag_pos), to_slot, item.command_ident())
            };
            session
                .send_command(Command::Equip {
                    from_pos,
                    to_slot,
                    item_ident,
                })
                .await?;
            Ok(format!("equipped backpack slot {backpack_slot}"))
        }
        Action::DismantleItem { backpack_slot } => {
            let (item_pos, item_ident) = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before dismantle"))?;
                let (bag_pos, item) = gs
                    .character
                    .inventory
                    .iter()
                    .enumerate()
                    .find_map(|(i, (bp, io))| {
                        io.and_then(|it| if i + 1 == *backpack_slot { Some((bp, it)) } else { None })
                    })
                    .ok_or_else(|| anyhow!("backpack slot {backpack_slot} empty at execute"))?;
                (PlayerItemPosition::from(bag_pos), item.command_ident())
            };
            session
                .send_command(Command::Blacksmith {
                    item_pos,
                    action: BlacksmithAction::Dismantle,
                    item_ident,
                })
                .await?;
            Ok(format!("dismantled backpack slot {backpack_slot}"))
        }
        Action::FightDungeon => {
            session.send_command(Command::UpdateDungeons).await?;
            let dungeon_name = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing after UpdateDungeons"))?;
                let state = game::StateSummary::from_game_state(gs);
                state
                    .dungeons
                    .best_winnable_name
                    .ok_or_else(|| anyhow!("no winnable dungeon"))?
            };
            // Resolve name back to the Dungeon enum (try light first, then shadow).
            let dungeon = if let Some(d) = game::find_light_dungeon(&dungeon_name) {
                Dungeon::Light(d)
            } else if let Some(d) = game::find_shadow_dungeon(&dungeon_name) {
                Dungeon::Shadow(d)
            } else {
                return Err(anyhow!("couldn't resolve dungeon name {dungeon_name:?}"));
            };
            session
                .send_command(Command::FightDungeon {
                    dungeon,
                    use_mushroom: false,
                })
                .await?;
            Ok(format!("fought dungeon {dungeon_name}"))
        }
        Action::HellevatorEnter => {
            session.send_command(Command::HellevatorEnter).await?;
            Ok("entered hellevator".to_string())
        }
        Action::HellevatorFight => {
            session
                .send_command(Command::HellevatorFight {
                    use_mushroom: false,
                })
                .await?;
            Ok("hellevator fight".to_string())
        }
        Action::HellevatorClaimDaily => {
            session.send_command(Command::HellevatorClaimDaily).await?;
            Ok("claimed hellevator daily".to_string())
        }
        Action::HellevatorClaimDailyYesterday => {
            session
                .send_command(Command::HellevatorClaimDailyYesterday)
                .await?;
            Ok("claimed hellevator yesterday".to_string())
        }
        Action::HellevatorClaimFinal => {
            session.send_command(Command::HellevatorClaimFinal).await?;
            Ok("claimed hellevator final".to_string())
        }
        Action::UnderworldUpgradeBuilding { building } => {
            let bt = game::underworld_building_from_name(building)
                .ok_or_else(|| anyhow!("unknown underworld building {building:?}"))?;
            session
                .send_command(Command::UnderworldUpgradeStart {
                    building: bt,
                    mushrooms: 0,
                })
                .await?;
            Ok(format!("underworld: started upgrade of {building}"))
        }
        Action::UnderworldGatherResource { resource } => {
            let rt = game::underworld_resource_from_name(resource)
                .ok_or_else(|| anyhow!("unknown underworld resource {resource:?}"))?;
            session
                .send_command(Command::UnderworldCollect { resource: rt })
                .await?;
            Ok(format!("underworld: collected {resource}"))
        }
        Action::UnderworldUpgradeUnit { unit } => {
            let ut = game::underworld_unit_from_name(unit)
                .ok_or_else(|| anyhow!("unknown underworld unit {unit:?}"))?;
            session.send_command(Command::UnderworldUnitUpgrade { unit: ut }).await?;
            Ok(format!("underworld: upgraded {unit}"))
        }
        Action::FightTower => {
            let current_level = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before FightTower"))?;
                let prog = sf_api::misc::EnumMapGet::get(
                    &gs.dungeons.light,
                    sf_api::gamestate::dungeons::LightDungeon::Tower,
                );
                match prog {
                    sf_api::gamestate::dungeons::DungeonProgress::Open { finished } => (finished + 1) as u8,
                    _ => return Err(anyhow!("tower not open")),
                }
            };
            session
                .send_command(Command::FightTower {
                    current_level,
                    use_mush: false,
                })
                .await?;
            Ok(format!("tower fight at floor {current_level}"))
        }
        Action::FightPortal => {
            session.send_command(Command::FightPortal).await?;
            Ok("portal fight".to_string())
        }
        Action::FortressUpgradeBuilding { building } => {
            let f_type = game::fortress_building_from_name(building)
                .ok_or_else(|| anyhow!("unknown building {building:?}"))?;
            session.send_command(Command::FortressBuild { f_type }).await?;
            Ok(format!("fortress: started upgrade of {building}"))
        }
        Action::FeedPet { pet_id, habitat } => {
            let h = game::habitat_from_name(habitat)
                .ok_or_else(|| anyhow!("unknown habitat {habitat:?}"))?;
            let fruit_idx = h as u32 + 1;
            session
                .send_command(Command::PetFeed {
                    pet_id: *pet_id,
                    fruit_idx,
                })
                .await?;
            Ok(format!("fed pet {pet_id} ({habitat})"))
        }
        Action::FightPetHabitat { habitat } => {
            let h = game::habitat_from_name(habitat)
                .ok_or_else(|| anyhow!("unknown habitat {habitat:?}"))?;
            let (enemy_pos, pet_id) = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before pet habitat fight"))?;
                let pets = gs.pets.as_ref().ok_or_else(|| anyhow!("pets not unlocked"))?;
                let hab = sf_api::misc::EnumMapGet::get(&pets.habitats, h);
                let enemy_pos = match hab.exploration {
                    sf_api::gamestate::unlockables::HabitatExploration::Exploring { fights_won, .. } => {
                        (fights_won + 1) as u32
                    }
                    _ => return Err(anyhow!("habitat not in Exploring state")),
                };
                let pet_id = hab
                    .pets
                    .iter()
                    .filter(|p| p.level > 0)
                    .max_by_key(|p| p.level)
                    .map(|p| p.id)
                    .ok_or_else(|| anyhow!("no pet to send"))?;
                (enemy_pos, pet_id)
            };
            session
                .send_command(Command::FightPetDungeon {
                    use_mush: false,
                    habitat: h,
                    enemy_pos,
                    player_pet_id: pet_id,
                })
                .await?;
            Ok(format!("pet dungeon fight ({habitat}) floor {enemy_pos}"))
        }
        Action::FightPetOpponent { habitat } => {
            let h = game::habitat_from_name(habitat)
                .ok_or_else(|| anyhow!("unknown habitat {habitat:?}"))?;
            let opponent_id = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before pet pvp"))?;
                gs.pets
                    .as_ref()
                    .map(|p| p.opponent.id)
                    .filter(|&id| id != 0)
                    .ok_or_else(|| anyhow!("no pet opponent"))?
            };
            let _ = (h, opponent_id);
            session
                .send_command(Command::FightPetOpponent {
                    habitat: h,
                    opponent_id,
                })
                .await?;
            Ok(format!("pet PvP in {habitat}"))
        }
        Action::BuyShopItem { shop, pos } => {
            let st = match shop.as_str() {
                "weapon" => ShopType::Weapon,
                "magic" => ShopType::Magic,
                other => return Err(anyhow!("unknown shop {other:?}")),
            };
            // Build the BuyShop command. Need: ShopPosition, free bag slot, item_ident.
            let (shop_pos, item_ident, new_pos) = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before buy_shop"))?;
                let shop = sf_api::misc::EnumMapGet::get(&gs.shops, st);
                let (sp, item) = shop
                    .iter()
                    .find(|(sp, _)| sp.position() == (*pos as usize))
                    .ok_or_else(|| anyhow!("shop {shop_label} slot {pos} is empty", shop_label = shop_label(st)))?;
                let bag_pos = gs
                    .character
                    .inventory
                    .free_slot()
                    .ok_or_else(|| anyhow!("no free backpack slot for buy_shop"))?;
                (sp, item.command_ident(), PlayerItemPosition::from(bag_pos))
            };
            session
                .send_command(Command::BuyShop {
                    shop_pos,
                    new_pos,
                    item_ident,
                })
                .await?;
            Ok(format!("bought {shop} slot {pos}"))
        }
        Action::ClaimTaskChest { track, pos } => {
            let cmd = match track.as_str() {
                "daily" => Command::CollectDailyQuestReward { pos: *pos as usize },
                "event" => Command::CollectEventTaskReward { pos: *pos as usize },
                other => return Err(anyhow!("unknown track {other:?}")),
            };
            session.send_command(cmd).await?;
            Ok(format!("claimed {track} chest {pos}"))
        }
        Action::OpenMail { pos } => {
            session
                .send_command(Command::MessageOpen { pos: *pos as i32 })
                .await?;
            Ok(format!("opened mail at pos {pos}"))
        }
        Action::DeleteAllMail => {
            // -1 is the server-side "delete all" sentinel.
            session.send_command(Command::MessageDelete { pos: -1 }).await?;
            Ok("deleted all mail".to_string())
        }
        Action::ClaimPendingMail { msg_id } => {
            session
                .send_command(Command::ClaimableClaim { msg_id: *msg_id })
                .await?;
            Ok(format!("claimed pending mail {msg_id}"))
        }
        Action::FortressTrainUnit { unit, count } => {
            let ut = game::fortress_unit_from_name(unit)
                .ok_or_else(|| anyhow!("unknown unit {unit:?}"))?;
            session
                .send_command(Command::FortressBuildUnit {
                    unit: ut,
                    count: *count,
                })
                .await?;
            Ok(format!("fortress: training {count} {unit}"))
        }
        Action::FortressUpgradeUnit { unit } => {
            let ut = game::fortress_unit_from_name(unit)
                .ok_or_else(|| anyhow!("unknown unit {unit:?}"))?;
            session.send_command(Command::FortressUpgradeUnit { unit: ut }).await?;
            Ok(format!("fortress: upgraded {unit}"))
        }
        Action::FortressAttack => {
            let soldiers = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before FortressAttack"))?;
                let f = gs
                    .fortress
                    .as_ref()
                    .ok_or_else(|| anyhow!("fortress not unlocked"))?;
                sf_api::misc::EnumMapGet::get(&f.units, FortressUnitType::Soldier).count as u32
            };
            session
                .send_command(Command::FortressAttack { soldiers })
                .await?;
            let outcome = session
                .game_state()
                .and_then(|gs| gs.last_fight.as_ref())
                .map(|fight| {
                    format!(
                        "won={} honor_change={} silver_change={}",
                        fight.has_player_won, fight.honor_change, fight.silver_change
                    )
                })
                .unwrap_or_else(|| "no fight data".to_string());
            Ok(format!("fortress attack with {soldiers} soldiers: {outcome}"))
        }
        Action::FortressRerollEnemy => {
            session
                .send_command(Command::FortressNewEnemy {
                    use_mushroom: false,
                })
                .await?;
            Ok("fortress: rerolled enemy".to_string())
        }
        Action::FortressGatherResource { resource } => {
            let r = game::fortress_resource_from_name(resource)
                .ok_or_else(|| anyhow!("unknown resource {resource:?}"))?;
            session
                .send_command(Command::FortressGather { resource: r })
                .await?;
            Ok(format!("fortress: gathered {resource}"))
        }
        Action::DrinkPotion { backpack_slot } => {
            let (from, item_ident) = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before drink"))?;
                let (bag_pos, item) = gs
                    .character
                    .inventory
                    .iter()
                    .enumerate()
                    .find_map(|(i, (bp, io))| {
                        io.and_then(|it| if i + 1 == *backpack_slot { Some((bp, it)) } else { None })
                    })
                    .ok_or_else(|| anyhow!("backpack slot {backpack_slot} empty at execute"))?;
                (ItemPosition::from(bag_pos), item.command_ident())
            };
            session.send_command(Command::UsePotion { from, item_ident }).await?;
            Ok(format!("drank potion from backpack slot {backpack_slot}"))
        }
        Action::SellItem { backpack_slot } => {
            let (item_pos, item_ident) = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing before sell"))?;
                let (bag_pos, item) = gs
                    .character
                    .inventory
                    .iter()
                    .enumerate()
                    .find_map(|(i, (bp, io))| {
                        io.and_then(|it| if i + 1 == *backpack_slot { Some((bp, it)) } else { None })
                    })
                    .ok_or_else(|| anyhow!("backpack slot {backpack_slot} empty at execute"))?;
                (PlayerItemPosition::from(bag_pos), item.command_ident())
            };
            session
                .send_command(Command::SellShop { item_pos, item_ident })
                .await?;
            Ok(format!("sold backpack slot {backpack_slot}"))
        }
        Action::FightArena => {
            session.send_command(Command::CheckArena).await?;
            let enemy_ids: Vec<u32> = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing after CheckArena"))?;
                gs.arena
                    .enemy_ids
                    .iter()
                    .copied()
                    .filter(|&id| id != 0)
                    .collect()
            };
            for id in &enemy_ids {
                session
                    .send_command(Command::ViewPlayer {
                        ident: id.to_string(),
                    })
                    .await?;
            }
            let target = {
                let gs = session
                    .game_state()
                    .ok_or_else(|| anyhow!("game_state missing after lookups"))?;
                enemy_ids
                    .iter()
                    .filter_map(|&id| gs.lookup.lookup_pid(id))
                    .min_by_key(|op| op.level)
                    .map(|op| (op.name.clone(), op.level))
                    .ok_or_else(|| anyhow!("no arena opponent resolved"))?
            };
            session
                .send_command(Command::Fight {
                    name: target.0.clone(),
                    use_mushroom: false,
                })
                .await?;
            Ok(format!("fought {} (L{})", target.0, target.1))
        }
        Action::Wait => Ok("wait".to_string()),
    }
}

struct Config {
    anthropic_api_key: String,
    claude_model: String,
    dry_run: bool,
    max_cycles: usize,
    run_seconds: Option<u64>,
    claude_min_interval: Duration,
    sf_username: String,
    sf_password: String,
    sf_server: String,
    sf_character: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        let anthropic_api_key =
            env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;
        let claude_model =
            env::var("CLAUDE_MODEL").unwrap_or_else(|_| "claude-haiku-4-5-20251001".into());
        let dry_run = env::var("DRY_RUN")
            .map(|v| v.trim().to_lowercase() != "false")
            .unwrap_or(true);
        let max_cycles = env::var("MAX_CYCLES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000usize);
        let run_seconds = env::var("RUN_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|v| if v == 0 { None } else { Some(v) })
            .unwrap_or(Some(3600));
        let claude_min_interval = Duration::from_secs(
            env::var("CLAUDE_MIN_INTERVAL_SEC")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60u64),
        );
        let sf_username = env::var("SF_USERNAME").unwrap_or_default();
        let sf_password = env::var("SF_PASSWORD").unwrap_or_default();
        let sf_server = env::var("SF_SERVER").unwrap_or_default();
        let sf_character = env::var("SF_CHARACTER").unwrap_or_default();
        if sf_username.is_empty() || sf_password.is_empty() {
            return Err(anyhow!("SF_USERNAME and SF_PASSWORD must be set"));
        }
        Ok(Self {
            anthropic_api_key,
            claude_model,
            dry_run,
            max_cycles,
            run_seconds,
            claude_min_interval,
            sf_username,
            sf_password,
            sf_server,
            sf_character,
        })
    }
}

async fn login(cfg: &Config) -> Result<SimpleSession> {
    if cfg.sf_server.is_empty() {
        tracing::info!("SSO login (SF_SERVER is empty)");
        let sessions = SimpleSession::login_sf_account(&cfg.sf_username, &cfg.sf_password)
            .await
            .context("SSO login failed")?;
        pick_sso_character(sessions, &cfg.sf_character).await
    } else {
        tracing::info!(server = %cfg.sf_server, "regular per-server login");
        SimpleSession::login(&cfg.sf_username, &cfg.sf_password, &cfg.sf_server)
            .await
            .context("login failed")
    }
}

async fn pick_sso_character(
    mut sessions: Vec<SimpleSession>,
    want: &str,
) -> Result<SimpleSession> {
    match sessions.len() {
        0 => Err(anyhow!("SSO login returned no characters")),
        1 => Ok(sessions.pop().unwrap()),
        _ => {
            if want.is_empty() {
                return Err(anyhow!(
                    "SSO account has {} characters; set SF_CHARACTER",
                    sessions.len()
                ));
            }
            let want_lc = want.to_lowercase();
            let mut seen: Vec<String> = Vec::new();
            for mut s in sessions {
                s.send_command(Command::Update)
                    .await
                    .context("Update failed resolving SSO character")?;
                let name = s
                    .game_state()
                    .map(|gs| gs.character.name.clone())
                    .unwrap_or_default();
                if name.to_lowercase() == want_lc {
                    return Ok(s);
                }
                seen.push(name);
            }
            Err(anyhow!("no character named {want:?} (saw: {seen:?})"))
        }
    }
}
