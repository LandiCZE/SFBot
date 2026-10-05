# sf-bot

Rust bot that plays a Shakes & Fidget character via [`sf-api`]. Decisions are
driven by Rust heuristics first (`strategy.rs`), with Claude (`brain.rs`) as a
fallback only for genuine tie-breaks the heuristics don't cover. Every action is
validated before execution; the bot **never spends mushrooms** by design.

[`sf-api`]: https://docs.rs/sf-api

## Run locally

```bash
cp .env.example .env       # fill in SF_USERNAME, SF_PASSWORD, (optional SF_SERVER / SF_CHARACTER), ANTHROPIC_API_KEY
cargo run
```

Environment variables:

| Key | Default | Meaning |
|---|---|---|
| `SF_USERNAME` | — | S&F login (email for SSO, character name for per-server). |
| `SF_PASSWORD` | — | S&F password. |
| `SF_SERVER` | *empty* | Blank → SSO. Set a hostname like `s31.sfgame.eu` for per-server. |
| `SF_CHARACTER` | *empty* | For SSO with multiple characters, pick by display name. |
| `ANTHROPIC_API_KEY` | — | Claude Messages API key (only used when heuristics return `None`). |
| `CLAUDE_MODEL` | `claude-haiku-4-5-20251001` | Model to call for tie-breaks. |
| `DRY_RUN` | `true` | `false` to actually execute actions. |
| `RUN_SECONDS` | `3600` | Hard runtime cap. `0` disables the cap. |
| `MAX_CYCLES` | `10000` | Hard cycle cap. |
| `CLAUDE_MIN_INTERVAL_SEC` | `60` | Minimum spacing between Claude calls (plan's "1/min at most"). |

## Run on GitHub Actions (free)

Push this directory to GitHub and the workflow in `.github/workflows/sf-bot.yml`
will fire every 30 min, run the bot for 25 min, and upload `decisions.jsonl` as
an artifact.

Setup:
1. Create a new GitHub repo (public is best — free unlimited Actions minutes).
2. From this directory: `git init && git add . && git commit -m "sf-bot" && git remote add origin <url> && git push -u origin main`.
3. In the repo settings → **Secrets and variables → Actions**, add:
   - `SF_USERNAME`
   - `SF_PASSWORD`
   - `SF_CHARACTER` (if your SSO account has multiple characters)
   - `ANTHROPIC_API_KEY`
4. The cron will auto-trigger on the next `:00` or `:30`. For an immediate test,
   use the **Run workflow** button on the Actions tab (workflow_dispatch).

Private repos work too but hit the 2000 min/month free cap after ~2.5 runs/day.
Public repos have no cap.

## Layout

```
src/
  main.rs      # loop: Update → autopilot expedition → strategy → Claude fallback → log
  game.rs      # StateSummary built from sf-api GameState (what Claude/strategy see)
  actions.rs   # Action enum + validator (gates everything, enforces no-mushroom rule)
  strategy.rs  # Pure Rust heuristic action picker (returns None → defer to Claude)
  brain.rs     # Claude Messages API call via reqwest with tool-use
  log.rs       # Append-only decisions.jsonl
```
