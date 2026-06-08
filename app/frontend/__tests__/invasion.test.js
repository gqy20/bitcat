import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function loadInvasion() {
  window.BitCatGames = {};
  const dirname = path.dirname(fileURLToPath(import.meta.url));
  const source = fs.readFileSync(path.join(dirname, '../js/games/invasion.js'), 'utf8');
  vm.runInThisContext(source);
  return window.BitCatInvasionTest;
}

const config = {
  game_type: 'invasion',
  title: 'Desktop Invasion',
  grid: { width: 28, height: 18, cell_size: 32 },
  player: { speed_ms: 140, initial_length: 3 },
  rules: { walls_kill: false, self_kill: false, food_count: 5, speed_ramp: 0.96, win_length: 12 },
  theme: { head: 'cat', body: 'trail', food: 'fish', trail_alpha: 0.55 },
  dialogue: { start: 'start', win: 'win', lose: 'lose' },
};

describe('Desktop Invasion rules', () => {
  it('registers as an external game engine', () => {
    const { InvasionEngine } = loadInvasion();

    expect(window.BitCatGames.invasion).toBeTypeOf('function');
    expect(window.BitCatGames.invasion(config, {})).toBeInstanceOf(InvasionEngine);
  });

  it('guards a nearby enemy and awards score', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    engine.state = 'playing';
    engine.enemies = [{ id: 'enemy-0', x: engine.player.x + 0.5, y: engine.player.y, r: 13, targetId: engine.targets[0].id }];

    engine.handleInput({ type: 'confirm' });

    expect(engine.enemies).toHaveLength(0);
    expect(engine.defeated).toBe(1);
    expect(engine.maxCombo).toBe(1);
    expect(engine.endDetails().guarded_targets).toBe(1);
    expect(engine.score).toBeGreaterThan(0);
  });

  it('marks a target stolen when an enemy reaches it', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    engine.state = 'playing';
    const target = engine.targets[0];
    engine.enemies = [{ id: 'enemy-0', x: target.x, y: target.y, r: 13, speed: 0.001, targetId: target.id }];

    engine.updateEnemies(16);

    expect(target.stolen).toBe(true);
    expect(engine.stolen).toBe(1);
  });

  it('supports pulse skill and cooldown for clustered enemies', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    engine.state = 'playing';
    engine.enemies = [
      { id: 'enemy-0', variant: 'crawler', x: engine.player.x + 0.5, y: engine.player.y, r: 13, hp: 1, targetId: engine.targets[0].id },
      { id: 'enemy-1', variant: 'skitter', x: engine.player.x, y: engine.player.y + 0.7, r: 10, hp: 1, targetId: engine.targets[1].id },
      { id: 'enemy-2', variant: 'brute', x: engine.player.x + 0.8, y: engine.player.y + 0.8, r: 16, hp: 2, targetId: engine.targets[2].id },
    ];

    engine.handleInput({ type: 'skill', slot: 1 });

    expect(engine.defeated).toBe(2);
    expect(engine.enemies).toHaveLength(1);
    expect(engine.enemies[0].hp).toBe(1);
    expect(engine.pulseCooldownMs).toBeGreaterThan(0);
  });

  it('starts a sprint skill with cooldown', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    engine.state = 'playing';

    engine.handleInput({ type: 'skill', slot: 2 });

    expect(engine.sprintMs).toBeGreaterThan(0);
    expect(engine.sprintCooldownMs).toBeGreaterThan(0);
  });

  it('dash can collide with and defeat a nearby enemy', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    engine.state = 'playing';
    engine.sprintMs = 500;
    engine.enemies = [{ id: 'enemy-0', variant: 'crawler', x: engine.player.x + 0.4, y: engine.player.y, r: 13, hp: 1, targetId: engine.targets[0].id }];

    engine.handleSprintCollisions();

    expect(engine.enemies).toHaveLength(0);
    expect(engine.defeated).toBe(1);
  });

  it('saving a memory target reduces pulse cooldown', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    const memory = engine.targets.find((target) => target.kind === 'memory_shard');
    engine.pulseCooldownMs = 3000;

    engine.applyTargetSave(memory);

    expect(engine.pulseCooldownMs).toBeLessThan(3000);
    expect(engine.savedTargets.has(memory.id)).toBe(true);
  });

  it('stealing a reminder target raises alarm pressure', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    engine.state = 'playing';
    const reminder = engine.targets.find((target) => target.kind === 'reminder_note');
    engine.enemies = [{ id: 'enemy-0', variant: 'crawler', x: reminder.x, y: reminder.y, r: 13, hp: 1, speed: 0.001, targetId: reminder.id }];

    engine.updateEnemies(16);

    expect(reminder.stolen).toBe(true);
    expect(engine.enemies.length).toBeGreaterThan(0);
  });

  it('end text summarizes protected targets and clutch saves', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    engine.clutchSaves = 2;
    engine.savedTargets.add(engine.targets[0].id);

    const text = engine.endText('win');

    expect(text).toContain('保住');
    expect(text).toContain('最后一刻救场 2 次');
  });

  it('loads runtime projection from IPC', async () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {
      invoke: async (cmd) => {
        expect(cmd).toBe('cmd_get_game_projection');
        return {
          version: 1,
          items: [
            { id: 'mem-1', kind: 'memory_shard', title: 'release checklist', weight: 5 },
            { id: 'rem-1', kind: 'reminder_note', title: 'stand up', weight: 3 },
          ],
        };
      },
    });

    await Promise.resolve();
    await Promise.resolve();

    expect(engine.targets[0].title).toBe('release checklist');
    expect(engine.targets[0].kind).toBe('memory_shard');
    expect(engine.targets).toHaveLength(2);
  });

  it('reports detailed end metrics for flawless wins', () => {
    const { InvasionEngine } = loadInvasion();
    const engine = new InvasionEngine(config, {});
    engine.state = 'playing';
    engine.defeated = config.rules.win_length;
    engine.maxCombo = 4;
    engine.elapsedMs = 12345;

    engine.finish('win');

    expect(engine.endDetails()).toMatchObject({
      defeated: config.rules.win_length,
      stolen: 0,
      max_combo: 4,
      elapsed_ms: 12345,
      flawless: true,
    });
  });
});
