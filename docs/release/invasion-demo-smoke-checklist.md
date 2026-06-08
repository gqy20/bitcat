# Invasion Demo Smoke Checklist

Use this checklist after changing `Desktop Invasion`, game input routing, or points/achievement recording. Run it after the D1 permission gate smoke pass so the Demo build is already in a known permission state.

## Launch Paths

- [ ] Start Invasion from the panel action.
- [ ] Start Invasion through AI `start_game(kind=invasion)`.
- [ ] Start Invasion through `cmd_start_invasion` in a debug/dev build.
- [ ] Restart the app and start Invasion again without stale game state.

## Input Paths

- [ ] Keyboard arrows or WASD move the player.
- [ ] Keyboard confirm/primary attack guards the nearest enemy.
- [ ] Keyboard cancel exits the round.
- [ ] 8BitDo D-pad moves the player.
- [ ] 8BitDo A/primary attack guards the nearest enemy.
- [ ] 8BitDo B/cancel exits the round.
- [ ] Game input capture releases after win, lose, and cancel.

## Gameplay Feedback

- [ ] Enemy spawn warning ring is visible.
- [ ] Threat line points from enemy to its target.
- [ ] Guard success shows score/combo feedback.
- [ ] Stolen targets visibly dim and log a `target stolen` event.
- [ ] HUD shows safe targets, defeated count, combo, and remaining seconds.
- [ ] Win overlay appears after the required defended count.
- [ ] Lose overlay appears after enough targets are stolen.
- [ ] Starting three rounds in a row does not leave old enemies, effects, or input state.

## Projection Safety

- [ ] Runtime projection loads safe titles only.
- [ ] Long-term memory content appears only as short titles.
- [ ] Reminder message bodies are not exposed to the game.
- [ ] Agent Watch raw hook payloads are not exposed to the game.
- [ ] Fallback projection is used when IPC projection fails.

## Points And Logs

- [ ] Starting Invasion writes `GamePlayed` and `InvasionPlayed`.
- [ ] Winning Invasion writes `GameWon` and `InvasionWon`.
- [ ] Flawless win writes `InvasionFlawlessWin`.
- [ ] `cmd_game_end` logs result, score, game type, and Invasion details.
- [ ] Frontend logs include spawn, guard success, target stolen, and finish metrics.
- [ ] Settings points view displays the new Invasion event labels.

## Failure Notes

For each failed checkbox, record:

- Commit or build identifier.
- Launch path used.
- Input device used.
- `cmd_game_log` / app log excerpts around the round.
- Whether runtime projection or fallback projection was used.
