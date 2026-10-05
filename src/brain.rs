use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

use crate::actions::Decision;
use crate::game::StateSummary;
use crate::log::DecisionLog;

const SYSTEM_PROMPT: &str = "You are playing a Shakes & Fidget character via a bot. Goal: level \
up efficiently and stay strong.

IMPORTANT: TRUST THE CURRENT STATE, NOT THE RECENT DECISIONS LOG. The bot's autopilot handles \
in-progress expeditions (encounter / boss / reward picks + waiting timers) silently — those \
steps DO NOT appear in the recent decisions log. If the current state shows \
`active_expedition: null`, the previous expedition is already DONE regardless of what the \
recent log says. Do not infer in-progress state from history; read the current state only.

Priority order when the character is idle:
 1. If tavern.mode is 'quests' and quests are available: pick the quest with the best XP per \
minute (consider silver and item rewards as tie-breakers). Use start_quest.
 2. If tavern.mode is 'expeditions' and active_expedition is null and \
thirst_for_adventure_sec >= the cheapest expedition's cost: start a new expedition. Use \
start_expedition. The bot auto-pilots encounter/boss/reward picks — you only pick WHICH one.
 3. If silver is low and backpack has sellable junk: use sell_item.
 4. If backpack has a clear equipment upgrade (higher main-stat or Constitution than the \
currently equipped slot item): use equip_item.
 5. If silver covers next_point_cost_silver: use buy_attribute on main attribute or constitution.
 6. If arena.off_cooldown and enemy_ids are non-empty: use fight_arena (bot picks the weakest).
 7. If ALU is drained and nothing else useful: use start_guard_work (1-10 hours).
 8. Only if NOTHING above applies: use wait.

Hard rules (bot enforces these in code — if you break them the action gets rejected):
 - NEVER spend mushrooms. The runtime refuses mushroom-costing actions.
 - If an action in the recent decisions log just failed with a specific reason, don't repeat it \
until the state actually changed to fix that reason.

Return exactly one action via the choose_action tool.";

pub async fn decide(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    state: &StateSummary,
    recent: &[DecisionLog],
) -> Result<Decision> {
    let state_json = serde_json::to_value(state)?;
    let recent_json: Vec<Value> = recent
        .iter()
        .map(|d| {
            json!({
                "ts": d.ts,
                "action": d.action,
                "reason": d.reason,
                "result": d.result,
                "invalid_reason": d.invalid_reason,
            })
        })
        .collect();

    // KEY FACTS: a tiny preamble that defeats recent-log bias. Claude's "wait"
    // regressions happened because it inferred expedition-in-progress from the
    // recent log even when active_expedition was null — spell the invariants out.
    let active = state.tavern.active_expedition.is_some();
    let can_start_new = state.tavern.current_action == "idle" && !active;
    let key_facts = format!(
        "KEY FACTS (trust these over the recent decisions log):\n\
         - current_action: {ca}\n\
         - active_expedition_present: {active}\n\
         - tavern_mode: {mode}\n\
         - thirst_for_adventure_sec: {thirst}\n\
         - silver: {silver}\n\
         - mushrooms: {mush} (NEVER spend)\n\
         - arena_off_cooldown: {arena_off}\n\
         - can_start_new_activity: {can_start_new}\n",
        ca = state.tavern.current_action,
        active = active,
        mode = state.tavern.mode,
        thirst = state.tavern.thirst_for_adventure_sec,
        silver = state.character.silver,
        mush = state.character.mushrooms,
        arena_off = state.arena.off_cooldown,
        can_start_new = can_start_new,
    );

    let user_prompt = format!(
        "{key_facts}\nFull StateSummary:\n{state_pp}\n\nLast {n} decisions (oldest first):\n{recent_pp}",
        state_pp = serde_json::to_string_pretty(&state_json)?,
        n = recent.len(),
        recent_pp = serde_json::to_string_pretty(&recent_json)?,
    );

    let body = json!({
        "model": model,
        "max_tokens": 1024,
        "system": SYSTEM_PROMPT,
        "tools": [tool_schema()],
        "tool_choice": {"type": "tool", "name": "choose_action"},
        "messages": [{"role": "user", "content": user_prompt}],
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("anthropic request failed")?;

    let status = resp.status();
    let text = resp.text().await.context("read anthropic response body")?;
    if !status.is_success() {
        return Err(anyhow!("anthropic returned {status}: {text}"));
    }

    let v: Value = serde_json::from_str(&text).context("anthropic response not JSON")?;
    let tool_input = v["content"]
        .as_array()
        .and_then(|arr| arr.iter().find(|b| b["type"] == "tool_use"))
        .map(|b| b["input"].clone())
        .ok_or_else(|| anyhow!("no tool_use block in response: {text}"))?;

    let decision: Decision = serde_json::from_value(tool_input.clone())
        .with_context(|| format!("tool input didn't match Decision schema: {tool_input}"))?;
    Ok(decision)
}

fn tool_schema() -> Value {
    json!({
        "name": "choose_action",
        "description": "Choose the next action for the Shakes & Fidget character.",
        "input_schema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "start_quest",
                        "start_expedition",
                        "buy_attribute",
                        "equip_item",
                        "sell_item",
                        "start_guard_work",
                        "fight_arena",
                        "set_questing_preference",
                        "wait"
                    ]
                },
                "quest_index": {"type": "integer", "minimum": 0, "maximum": 2, "description": "Required for start_quest."},
                "expedition_index": {"type": "integer", "minimum": 0, "maximum": 2, "description": "Required for start_expedition."},
                "attribute": {"type": "string", "enum": ["strength","dexterity","intelligence","constitution","luck"], "description": "Required for buy_attribute."},
                "points": {"type": "integer", "minimum": 1, "description": "Required for buy_attribute."},
                "backpack_slot": {"type": "integer", "minimum": 1, "description": "Required for equip_item and sell_item."},
                "hours": {"type": "integer", "minimum": 1, "maximum": 10, "description": "Required for start_guard_work."},
                "prefer_quests": {"type": "boolean", "description": "Required for set_questing_preference."},
                "reason": {"type": "string", "description": "One short sentence explaining why you picked this action."}
            },
            "required": ["action", "reason"]
        }
    })
}
