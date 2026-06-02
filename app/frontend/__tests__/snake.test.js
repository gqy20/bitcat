import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function loadSnakeEngine() {
  window.BitCatGames = {};
  const dirname = path.dirname(fileURLToPath(import.meta.url));
  const source = fs.readFileSync(path.join(dirname, '../js/games/snake.js'), 'utf8');
  vm.runInThisContext(source);
  return window.BitCatGames.SnakeEngine;
}

const baseConfig = {
  game_type: 'snake',
  title: 'Word Snake',
  grid: { width: 10, height: 8, cell_size: 24 },
  player: { speed_ms: 100, initial_length: 3 },
  rules: { walls_kill: true, self_kill: true, food_count: 1, speed_ramp: 0.95, win_length: 20 },
  theme: { head: 'cat', body: 'yarn', food: 'mouse', trail_alpha: 0.55 },
  dialogue: { start: 'start', win: 'win', lose: 'lose' },
};

const vocab = {
  mode: 'meaning_choice',
  answer_count: 4,
  target_correct: 2,
  entries: [
    { id: 'create', term: 'create', meaning: '创造', distractors: ['取消', '等待', '借用'] },
    { id: 'review', term: 'review', meaning: '复习', distractors: ['删除', '奔跑', '折叠'] },
    { id: 'select', term: 'select', meaning: '选择', distractors: ['推迟', '描绘', '沉默'] },
    { id: 'focus', term: 'focus', meaning: '专注', distractors: ['庆祝', '移动', '猜测'] },
  ],
};

describe('Word Snake rules', () => {
  it('starts in vocabulary mode when snake_vocab is present', () => {
    const SnakeEngine = loadSnakeEngine();
    const engine = new SnakeEngine({ ...baseConfig, snake_vocab: vocab }, {}, () => 0);

    expect(engine.vocab.targetCorrect).toBe(2);
    expect(engine.question.term).toBe('create');
    expect(engine.answerFoods).toHaveLength(4);
    expect(engine.readyText()).toContain('正确中文释义');
  });

  it('grows and advances after eating the correct meaning', () => {
    const SnakeEngine = loadSnakeEngine();
    const engine = new SnakeEngine({ ...baseConfig, snake_vocab: vocab }, {}, () => 0);
    const beforeLength = engine.snake.length;
    const correct = engine.answerFoods.find((food) => food.correct);

    const grew = engine.consumeAnswer(correct);

    expect(grew).toBe(true);
    expect(engine.correctCount).toBe(1);
    expect(engine.score).toBeGreaterThan(0);
    expect(engine.snake.length).toBe(beforeLength);
    expect(engine.question.term).toBe('review');
  });

  it('does not grow after eating a wrong meaning in the movement path', () => {
    const SnakeEngine = loadSnakeEngine();
    const engine = new SnakeEngine({ ...baseConfig, snake_vocab: vocab }, {}, () => 0);
    const head = engine.snake[0];
    const wrong = { x: head.x + 1, y: head.y, label: '取消', correct: false };
    engine.answerFoods = [wrong];
    engine.food = wrong;

    engine.handleInput({ type: 'confirm' });
    engine.update(100);

    expect(engine.wrongCount).toBe(1);
    expect(engine.snake.length).toBe(baseConfig.player.initial_length);
    expect(engine.missed[0]).toMatchObject({ term: 'create', picked: '取消' });
  });

  it('queues direction changes for tighter turning', () => {
    const SnakeEngine = loadSnakeEngine();
    const engine = new SnakeEngine({ ...baseConfig, snake_vocab: vocab }, {}, () => 0);

    engine.handleInput({ type: 'confirm' });
    engine.handleInput({ type: 'direction', dx: 0, dy: -1 });
    engine.handleInput({ type: 'direction', dx: 1, dy: 0 });

    expect(engine.directionQueue).toHaveLength(2);
    engine.update(100);
    expect(engine.dir).toEqual({ x: 0, y: -1 });
    engine.update(100);
    expect(engine.dir).toEqual({ x: 1, y: 0 });
  });

  it('raises feedback pulses after a correct answer', () => {
    const SnakeEngine = loadSnakeEngine();
    const engine = new SnakeEngine({ ...baseConfig, snake_vocab: vocab }, {}, () => 0);
    const correct = engine.answerFoods.find((food) => food.correct);

    engine.consumeAnswer(correct);

    expect(engine.boardFlashMs).toBeGreaterThan(0);
    expect(engine.comboPulseMs).toBeGreaterThan(0);
    expect(engine.questionPulseMs).toBeGreaterThan(0);
    expect(engine.effects.some((effect) => effect.text === '正确')).toBe(true);
  });
});
