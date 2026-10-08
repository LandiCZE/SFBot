# sf-bot

Rust bot that plays a Shakes & Fidget character via [`sf-api`]. Fully heuristic:
all decisions come from `strategy.rs`; when no heuristic applies, the bot waits.
Every action is validated before execution and the bot **never spends mushrooms**
by design.

[`sf-api`]: https://docs.rs/sf-api

## Run locally

```bash
cp .env.example .env       # fill in SF_USERNAME, SF_PASSWORD, (optional SF_SERVER / SF_CHARACTER)
cargo run
```

Environment variables:

| Key | Default | Meaning |
|---|---|---|
| `SF_USERNAME` | — | S&F login (email for SSO, character name for per-server). |
| `SF_PASSWORD` | — | S&F password. |
| `SF_SERVER` | *empty* | Blank → SSO. Set a hostname like `s31.sfgame.eu` for per-server. |
| `SF_CHARACTER` | *empty* | For SSO with multiple characters, pick by display name. |
| `DRY_RUN` | `true` | `false` to actually execute actions. |
| `RUN_SECONDS` | `3600` | Hard runtime cap. `0` disables the cap. |
| `MAX_CYCLES` | `10000` | Hard cycle cap. |

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
4. The cron will auto-trigger on the next `:00` or `:30`. For an immediate test,
   use the **Run workflow** button on the Actions tab (workflow_dispatch).

Private repos work too but hit the 2000 min/month free cap after ~2.5 runs/day.
Public repos have no cap.

## Layout

```
src/
  main.rs      # loop: Update → autopilot expedition → strategy → log
  game.rs      # StateSummary built from sf-api GameState (what strategy sees)
  actions.rs   # Action enum + validator (gates everything, enforces no-mushroom rule)
  strategy.rs  # Pure Rust heuristic action picker (None → wait)
  log.rs       # Append-only decisions.jsonl
```
