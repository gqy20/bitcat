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
    this.aiThoughts = [];
    this.commentaries = [];
    this.aiThinking = false;
    this.invoke = invoke;
    this.recorded = false;
    this.startedAt = new Date().toISOString();
    this.startedAtMs = Date.now();
    this.lastMoveAtMs = this.startedAtMs;
    this.finishedAt = null;
    this.finishedAtMs = null;
    this.record = null;
    this.pendingAiTiming = null;
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
    this.pendingAiTiming = { started_at: new Date().toISOString(), started_ms: Date.now() };
    try {
      const move = await this.invoke('cmd_gomoku_ai_move', { board: this.board, lastMove });
      if (this.board[move.y][move.x] !== EMPTY) throw new Error('occupied');
      this.place(move.x, move.y, AI);
      this.attachAiThought(move);
      if (this.moves.length >= 4 && this.moves.length % 2 === 0) {
        const commentary = await this.invoke('cmd_gomoku_commentary', {
          board: this.board,
          lastMove: { x: move.x, y: move.y },
        });
        if (commentary) this.commentaries.push(commentary);
      }
      if (hasFive(this.board, AI)) this.state = 'lose';
    } finally {
      this.aiThinking = false;
      this.pendingAiTiming = null;
    }
  }

  place(x, y, stone) {
    const nowMs = Date.now();
    const turnElapsedMs = Math.max(0, nowMs - this.lastMoveAtMs);
    this.board[y][x] = stone;
    this.lastMoveAtMs = nowMs;
    this.moves.push({
      x,
      y,
      stone,
      move: this.moves.length + 1,
      played_at: new Date(nowMs).toISOString(),
      elapsed_ms: Math.max(0, nowMs - this.startedAtMs),
      turn_elapsed_ms: turnElapsedMs,
    });
  }

  attachAiThought(move) {
    const current = this.moves[this.moves.length - 1];
    current.ai_message = move.message || null;
    current.ai_thought = move.thought || move.message || null;
    if (this.pendingAiTiming) {
      const finishedMs = Date.now();
      current.ai_started_at = this.pendingAiTiming.started_at;
      current.ai_finished_at = new Date(finishedMs).toISOString();
      current.ai_elapsed_ms = Math.max(0, finishedMs - this.pendingAiTiming.started_ms);
    }
    this.aiThoughts.push({
      move: current.move,
      x: current.x,
      y: current.y,
      played_at: current.played_at,
      elapsed_ms: current.elapsed_ms,
      turn_elapsed_ms: current.turn_elapsed_ms,
      text: current.ai_thought,
      ai_elapsed_ms: current.ai_elapsed_ms || null,
    });
  }

  finish(result) {
    this.state = result;
    this.finishedAt = new Date().toISOString();
    this.finishedAtMs = Date.now();
    this.record = {
      started_at: this.startedAt,
      finished_at: this.finishedAt,
      duration_ms: Math.max(0, this.finishedAtMs - this.startedAtMs),
      result,
      moves: this.moves.map((move) => ({
        move: move.move,
        x: move.x,
        y: move.y,
        played_at: move.played_at,
        elapsed_ms: move.elapsed_ms,
        turn_elapsed_ms: move.turn_elapsed_ms,
        ai_started_at: move.ai_started_at || null,
        ai_finished_at: move.ai_finished_at || null,
        ai_elapsed_ms: Number.isFinite(move.ai_elapsed_ms) ? move.ai_elapsed_ms : null,
      })),
    };
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
      if (cmd === 'cmd_gomoku_commentary') return { summary: '局势均衡', advantage: 'balanced' };
      return { x: 8, y: 7, message: '我贴住中腹。', thought: '我先保持中心连接。' };
    });

    await engine.playHuman(7, 7);

    expect(calls[0].cmd).toBe('cmd_gomoku_ai_move');
    expect(calls[0].payload.lastMove).toEqual({ x: 7, y: 7 });
    expect(calls[0].payload.board[7][7]).toBe(HUMAN);
    expect(engine.board[7][8]).toBe(AI);
    expect(engine.aiThoughts[0].text).toBe('我先保持中心连接。');
  });

  it('requests commentary after each full round from move four', async () => {
    const calls = [];
    const aiMoves = [
      { x: 8, y: 7, thought: '守住中腹。' },
      { x: 8, y: 8, thought: '连接右侧。' },
    ];
    const engine = new GomokuEngine(async (cmd, payload) => {
      calls.push({ cmd, payload });
      if (cmd === 'cmd_gomoku_commentary') return { summary: '白棋有反击点', advantage: 'ai' };
      return aiMoves.shift();
    });

    await engine.playHuman(7, 7);
    await engine.playHuman(7, 8);

    expect(calls.map((call) => call.cmd)).toEqual([
      'cmd_gomoku_ai_move',
      'cmd_gomoku_ai_move',
      'cmd_gomoku_commentary',
    ]);
    expect(engine.commentaries[0].summary).toBe('白棋有反击点');
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

  it('records game and per-move timing fields', async () => {
    const engine = new GomokuEngine(async () => ({ x: 8, y: 7, thought: '连接右侧。' }));

    await engine.playHuman(7, 7);
    engine.finish('cancel');

    expect(engine.record.duration_ms).toBeGreaterThanOrEqual(0);
    expect(engine.record.moves[0].played_at).toMatch(/T/);
    expect(engine.record.moves[0].elapsed_ms).toBeGreaterThanOrEqual(0);
    expect(engine.record.moves[0].turn_elapsed_ms).toBeGreaterThanOrEqual(0);
    expect(engine.record.moves[1].ai_started_at).toMatch(/T/);
    expect(engine.record.moves[1].ai_finished_at).toMatch(/T/);
    expect(engine.record.moves[1].ai_elapsed_ms).toBeGreaterThanOrEqual(0);
    expect(engine.aiThoughts[0].played_at).toMatch(/T/);
  });
});
