# Pet Animation Visual Roadmap

> Status: active
> Updated: 2026-05-15

This plan turns the pet animation research into the next concrete implementation steps. The research baseline lives in `docs/research/pet-animation-optimization.md`; this document reflects the current local code and should be used for day-to-day sequencing.

## Current Baseline

The first animation engine upgrades are already in the local tree:

- `app/frontend/js/pet.js` uses elapsed-driven timelines with per-frame durations.
- `core/src/pet.rs` mirrors the same timeline model for Rust-side tests and logic.
- transient moods such as `happy`, `talk`, `confused`, `gamewin`, and `gamelose` use repeat plus fallback instead of abrupt fixed timeouts.
- `core/src/pet_event.rs`, `app/src/pet_event_bus.rs`, and `core/src/mood_policy.rs` provide semantic events, notification TTLs, mood TTLs, deduplication, throttling, and event diagnostics.
- `app/frontend/js/app.js` now routes dance and music-reactive performances through `PerformerHost`, but those performances still temporarily bypass the normal pet update/render path.

The main remaining risk is not animation math; it is drift between the real pet implementation, tests, sprite coverage, and performance priority rules.

## Phase 1: Test Reality Alignment

Goal: make frontend tests exercise the real pet state machine instead of a copied historical version.

- Export `PetStateMachine`, `STATE_CONFIG`, and test helpers from `app/frontend/js/pet.js` in a way that still works in the browser window.
- Update `app/frontend/__tests__/pet.test.js` to import the real implementation.
- Remove the copied `PetStateMachine` and copied old `STATE_CONFIG` from the test file.
- Add assertions for timeline durations, repeat plus fallback, notification expiry, mood TTL expiry, and mode priority.

Verification:

```powershell
cd app/frontend
npx vitest run app/frontend/__tests__/pet.test.js
```

## Phase 2: Semantic Visual Completion

Goal: every semantic visual state that the frontend can enter should have explicit sprite frames.

Current gap:

- `focused` and `preparing` exist in `app/frontend/js/pet.js`.
- `focused` and `preparing` do not exist in `app/frontend/js/sprite.js`, so rendering falls back to idle.

Work:

- Add `FOCUSED_*` frames for a quiet attentive expression.
- Add `PREPARING_*` frames for tool preparation or active work.
- Register both states in `SPRITES`.
- Extend sprite tests so fallback only happens for truly unknown states.
- Keep the palette and 16x16 pixel constraints unchanged.

Design intent:

- `focused` should read as calm attention, not celebration.
- `preparing` should read as active processing, not error or panic.
- Both should be visually distinct at 128x128 and still recognizable in collapsed mini rendering.

Verification:

```powershell
cd app/frontend
npx vitest run
```

## Phase 3: Performance Priority Rules

Goal: define how dances and music-reactive performances interact with semantic pet events.

Current behavior:

- When `PerformerHost` is active, `app/frontend/js/app.js` updates and renders the performer instead of the normal pet state machine.
- Pet notifications and reactions can still arrive during a performance, but the visual priority and restore behavior are not clearly documented.

Decisions to make:

- Should `ToolFailed` or `ToolBlocked` interrupt a dance immediately?
- Should `AiWriting` and `ToolRunning` queue until the performance ends, or remain background-only?
- Should `SetMode(Sleep)` cancel performance, wait, or be ignored until completion?
- After performance ends, should the pet restore the previous semantic visual state or recompute from current notifications/mode/mood?

Likely first policy:

- hard interrupts: `ToolBlocked`, `ToolFailed`, `SetMode(Sleep)`, `Exit`.
- soft background events: `AiThinking`, `AiWriting`, `ToolPreparing`, `ToolRunning`, `React`.
- on performance end, recompute from current `mode`, active notifications, and current reaction mood.

Verification:

```powershell
cd app/frontend
npx vitest run app/frontend/__tests__/dance.test.js app/frontend/__tests__/pet.test.js
```

## Phase 4: Idle Variants

Goal: make long idle sessions feel less mechanical without changing the core rendering model.

Work:

- Add a small idle variant controller to the frontend state machine.
- Start with two low-risk variants: ear twitch and look around.
- Trigger variants only while the semantic visual state is idle.
- Use cooldowns and weights so variants feel occasional rather than busy.
- Keep variants additive and easy to disable if they distract.

Initial behavior target:

- base idle blink remains the default loop.
- a variant may play every 8-25 seconds.
- variants fall back to the base idle timeline without resetting unrelated semantic state.

## Later: External Spritesheet Manifest

Goal: move from hard-coded JS pixel arrays toward artist-editable sprite assets.

This should wait until Phases 1-4 stabilize. The migration should preserve the built-in sprite data as fallback.

Expected work:

- define a small manifest format for palette, frame size, spritesheet path, and animation timelines.
- add a loader that can read `manifest.json` plus `sprites.png`.
- keep current hard-coded sprites as the default pet if loading fails.
- document the asset authoring workflow.

Main risks:

- loading order inside the Tauri pet window;
- DPI and canvas scaling;
- collapsed mini rendering;
- keeping Rust and frontend animation definitions in sync.

## Open Notes

- The Rust `PetState` enum currently does not include a first-class dancing state. Avoid adding one until the performance priority policy is clear.
- The frontend has more visual states than Rust `PetState` because semantic notification mapping is intentionally owned by the pet window. Keep that split unless shared configuration becomes necessary.
- If timeline data keeps growing, consider generating Rust and JS animation constants from one source of truth before introducing external manifests.
