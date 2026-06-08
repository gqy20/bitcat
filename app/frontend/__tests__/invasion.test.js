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
    engine.enemies = [{ id: 'enemy-0', x: engine.player.x + 0.5, y: engine.player.y, r: 13 }];

    engine.handleInput({ type: 'confirm' });

    expect(engine.enemies).toHaveLength(0);
    expect(engine.defeated).toBe(1);
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
});
