import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function loadBeads() {
  window.BitCatGames = {};
  const dirname = path.dirname(fileURLToPath(import.meta.url));
  const source = fs.readFileSync(path.join(dirname, '../js/games/beads.js'), 'utf8');
  vm.runInThisContext(source);
  return window.BitCatBeadsTest;
}

const config = {
  game_type: 'beads',
  title: 'Pixel Beads',
  grid: { width: 16, height: 16, cell_size: 36 },
  player: { speed_ms: 120, initial_length: 3 },
  rules: { walls_kill: false, self_kill: false, food_count: 0, speed_ramp: 1, win_length: 256 },
  theme: { head: 'cat', body: 'dot', food: 'fish', trail_alpha: 0.55 },
  dialogue: { start: 'start', win: 'win', lose: 'lose' },
};

describe('Pixel Beads rules', () => {
  it('places the selected color on confirm', () => {
    const { BeadsEngine } = loadBeads();
    const engine = new BeadsEngine(config, {});

    engine.paletteIndex = 1;
    engine.handleInput({ type: 'confirm' });

    expect(engine.board[0][0]).toBe(1);
    expect(engine.score).toBeGreaterThanOrEqual(0);
  });

  it('cycles palette and supports undo input', () => {
    const { BeadsEngine } = loadBeads();
    const engine = new BeadsEngine(config, {});

    engine.handleInput({ type: 'cycle', dir: 1 });
    expect(engine.paletteIndex).toBe(3);
    engine.handleInput({ type: 'confirm' });
    engine.handleInput({ type: 'undo' });

    expect(engine.board[0][0]).toBe(-1);
  });

  it('wins when every required bead matches', () => {
    const { BeadsEngine } = loadBeads();
    const engine = new BeadsEngine(config, {});
    for (let y = 0; y < config.grid.height; y++) {
      for (let x = 0; x < config.grid.width; x++) {
        engine.board[y][x] = engine.target[y][x];
      }
    }

    engine.updateScore();
    if (engine.completion().ratio >= 1) engine.finish('win');

    expect(engine.getState()).toBe('win');
    expect(engine.score).toBeGreaterThan(0);
  });
});
