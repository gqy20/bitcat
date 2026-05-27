import { describe, expect, it } from 'vitest';

const BOARD_SIZE = 15;
const EMPTY = 0;
const HUMAN = 1;
const AI = 2;
const DIRS = [
  [1, 0],
  [0, 1],
  [1, 1],
  [1, -1],
];

function hasFive(board, stone) {
  for (let y = 0; y < BOARD_SIZE; y++) {
    for (let x = 0; x < BOARD_SIZE; x++) {
      if (board[y][x] !== stone) continue;
      for (const [dx, dy] of DIRS) {
        let count = 1;
        for (let step = 1; step < 5; step++) {
          const nx = x + dx * step;
          const ny = y + dy * step;
          if (nx < 0 || ny < 0 || nx >= BOARD_SIZE || ny >= BOARD_SIZE || board[ny][nx] !== stone) break;
          count += 1;
        }
        if (count >= 5) return true;
      }
    }
  }
  return false;
}

class GomokuEngine {
  constructor(invoke) {
    this.board = Array.from({ length: BOARD_SIZE }, () => Array(BOARD_SIZE).fill(EMPTY));
    this.state = 'playing';
    this.moves = [];
    this.aiThinking = false;
    this.invoke = invoke;
    this.recorded = false;
  }

  playHuman(x, y) {
    if (this.board[y][x] !== EMPTY || this.aiThinking) return;
    this.place(x, y, HUMAN);
    if (hasFive(this.board, HUMAN)) {
      this.finish('win');
      return;
    }
    return this.askAiMove({ x, y });
  }

  async askAiMove(lastMove) {
    this.aiThinking = true;
    try {
      const move = await this.invoke('cmd_gomoku_ai_move', { board: this.board, lastMove });
      if (this.board[move.y][move.x] !== EMPTY) throw new Error('occupied');
      this.place(move.x, move.y, AI);
      if (hasFive(this.board, AI)) this.state = 'lose';
    } finally {
      this.aiThinking = false;
    }
  }

  place(x, y, stone) {
    this.board[y][x] = stone;
    this.moves.push({ x, y, stone });
  }

  finish(result) {
    this.state = result;
    this.recorded = true;
  }
}

describe('GomokuEngine rules', () => {
  it('detects five in every major direction', () => {
    for (const [dx, dy] of DIRS) {
      const board = Array.from({ length: BOARD_SIZE }, () => Array(BOARD_SIZE).fill(EMPTY));
      const startX = dx < 0 ? 8 : 3;
      const startY = dy < 0 ? 8 : 3;
      for (let i = 0; i < 5; i++) board[startY + dy * i][startX + dx * i] = HUMAN;
      expect(hasFive(board, HUMAN)).toBe(true);
    }
  });

  it('passes current board and last human move to AI IPC', async () => {
    const calls = [];
    const engine = new GomokuEngine(async (cmd, payload) => {
      calls.push({ cmd, payload });
      return { x: 8, y: 7 };
    });

    await engine.playHuman(7, 7);

    expect(calls[0].cmd).toBe('cmd_gomoku_ai_move');
    expect(calls[0].payload.lastMove).toEqual({ x: 7, y: 7 });
    expect(calls[0].payload.board[7][7]).toBe(HUMAN);
    expect(engine.board[7][8]).toBe(AI);
  });

  it('wins before asking AI when human completes five', async () => {
    const engine = new GomokuEngine(async () => {
      throw new Error('should not ask ai');
    });
    for (let x = 0; x < 4; x++) engine.place(x, 0, HUMAN);

    await engine.playHuman(4, 0);

    expect(engine.state).toBe('win');
    expect(engine.recorded).toBe(true);
  });
});
