import { describe, expect, it } from 'vitest';

class SnakeEngine {
  constructor(config, rng = () => 0) {
    this.config = config;
    this.rng = rng;
    this.state = 'ready';
    this.score = 0;
    this.dir = { x: 1, y: 0 };
    this.nextDir = { x: 1, y: 0 };
    this.stepMs = config.player.speed_ms;
    this.tickMs = 0;
    this.ended = false;
    this.snake = [];
    this.reset();
  }

  reset() {
    const len = this.config.player.initial_length;
    const startX = Math.floor(this.config.grid.width / 2);
    const startY = Math.floor(this.config.grid.height / 2);
    this.snake = [];
    for (let i = 0; i < len; i++) this.snake.push({ x: startX - i, y: startY });
    this.food = { x: startX + 1, y: startY };
  }

  handleInput(input) {
    if (input.type === 'confirm' && this.state === 'ready') this.state = 'playing';
    if (input.type === 'pause' && (this.state === 'playing' || this.state === 'paused')) {
      this.state = this.state === 'playing' ? 'paused' : 'playing';
    }
    if (input.type === 'cancel') this.finish('cancel');
    if (input.type === 'direction') {
      const next = { x: Math.sign(input.dx || 0), y: Math.sign(input.dy || 0) };
      if (Math.abs(next.x) + Math.abs(next.y) !== 1) return;
      if (next.x === -this.dir.x && next.y === -this.dir.y) return;
      if (this.state === 'ready') this.state = 'playing';
      this.nextDir = next;
    }
  }

  update(dt) {
    if (this.state !== 'playing' || this.ended) return;
    this.tickMs += dt;
    while (this.tickMs >= this.stepMs && this.state === 'playing') {
      this.tickMs -= this.stepMs;
      this.step();
    }
  }

  step() {
    this.dir = this.nextDir;
    const head = this.snake[0];
    const next = { x: head.x + this.dir.x, y: head.y + this.dir.y };
    const grid = this.config.grid;
    if (next.x < 0 || next.y < 0 || next.x >= grid.width || next.y >= grid.height) {
      this.finish('lose');
      return;
    }
    const ate = next.x === this.food.x && next.y === this.food.y;
    const body = ate ? this.snake : this.snake.slice(0, -1);
    if (body.some((p) => p.x === next.x && p.y === next.y)) {
      this.finish('lose');
      return;
    }
    this.snake.unshift(next);
    if (ate) {
      this.score += 10;
      if (this.snake.length >= this.config.rules.win_length) {
        this.finish('win');
      }
    } else {
      this.snake.pop();
    }
  }

  finish(result) {
    this.ended = true;
    this.state = result;
  }
}

const config = {
  grid: { width: 10, height: 8, cell_size: 24 },
  player: { speed_ms: 100, initial_length: 3 },
  rules: { walls_kill: true, self_kill: true, food_count: 1, speed_ramp: 0.95, win_length: 5 },
};

describe('SnakeEngine rules', () => {
  it('starts playing on confirm and moves right', () => {
    const engine = new SnakeEngine(config);
    const x = engine.snake[0].x;
    engine.handleInput({ type: 'confirm' });
    engine.update(100);
    expect(engine.snake[0].x).toBe(x + 1);
  });

  it('does not reverse directly', () => {
    const engine = new SnakeEngine(config);
    engine.handleInput({ type: 'confirm' });
    engine.handleInput({ type: 'direction', dx: -1, dy: 0 });
    engine.update(100);
    expect(engine.dir).toEqual({ x: 1, y: 0 });
  });

  it('grows and scores when eating food', () => {
    const engine = new SnakeEngine(config);
    engine.handleInput({ type: 'confirm' });
    engine.update(100);
    expect(engine.snake.length).toBe(4);
    expect(engine.score).toBe(10);
  });

  it('loses on wall collision', () => {
    const engine = new SnakeEngine(config);
    engine.snake = [{ x: 9, y: 1 }, { x: 8, y: 1 }, { x: 7, y: 1 }];
    engine.handleInput({ type: 'confirm' });
    engine.update(100);
    expect(engine.state).toBe('lose');
  });

  it('pauses without updating', () => {
    const engine = new SnakeEngine(config);
    const x = engine.snake[0].x;
    engine.handleInput({ type: 'confirm' });
    engine.handleInput({ type: 'pause' });
    engine.update(500);
    expect(engine.snake[0].x).toBe(x);
    expect(engine.state).toBe('paused');
  });

  it('wins at target length', () => {
    const engine = new SnakeEngine({ ...config, rules: { ...config.rules, win_length: 4 } });
    engine.handleInput({ type: 'confirm' });
    engine.update(100);
    expect(engine.state).toBe('win');
  });
});
