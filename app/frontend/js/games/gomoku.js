(function () {
  window.BitCatGames = window.BitCatGames || {};

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

  class GomokuEngine {
    constructor(config, host) {
      this.config = config;
      this.host = host || {};
      this.state = 'ready';
      this.score = 0;
      this.board = Array.from({ length: BOARD_SIZE }, () => Array(BOARD_SIZE).fill(EMPTY));
      this.cursor = { x: 7, y: 7 };
      this.moves = [];
      this.lastMove = null;
      this.aiThinking = false;
      this.message = '你执黑先行，BitCat 执白。';
      this.errorText = '';
      this.sessionId = `${Date.now()}-${Math.random().toString(16).slice(2)}`;
      this.startedAt = new Date().toISOString();
      this.finishedAt = null;
      this.recorded = false;
    }

    getState() {
      return this.state;
    }

    readyText() {
      return '点击交叉点落子，也可以用方向键移动，Enter 确认。';
    }

    hudText() {
      if (this.aiThinking) return 'BitCat 思考中';
      return `第 ${this.moves.length} 手${this.message ? ` - ${this.message}` : ''}`;
    }

    endText(result) {
      if (result === 'win') return `你完成五连，共 ${this.moves.length} 手。Enter 再来一局，Esc 退出。`;
      if (result === 'lose') return `BitCat 完成五连，共 ${this.moves.length} 手。Enter 再来一局，Esc 退出。`;
      return `本局已结束，共 ${this.moves.length} 手。`;
    }

    handleInput(input) {
      if (!input) return;
      if (this.state === 'win' || this.state === 'lose' || this.state === 'cancel') {
        if (input.type === 'confirm') this.host.restartGame && this.host.restartGame();
        else if (input.type === 'cancel') this.host.closeEndedGame && this.host.closeEndedGame(this.state);
        return;
      }
      if (input.type === 'cancel') {
        this.finish('cancel', '本局已退出。');
        return;
      }
      if (input.type === 'pause' && (this.state === 'playing' || this.state === 'paused')) {
        this.state = this.state === 'playing' ? 'paused' : 'playing';
        return;
      }
      if (input.type === 'direction') {
        this.cursor.x = clamp(this.cursor.x + Math.sign(input.dx || 0), 0, BOARD_SIZE - 1);
        this.cursor.y = clamp(this.cursor.y + Math.sign(input.dy || 0), 0, BOARD_SIZE - 1);
        return;
      }
      if (input.type === 'confirm') {
        if (this.state === 'ready') {
          this.state = 'playing';
          return;
        }
        this.playHuman(this.cursor.x, this.cursor.y);
      }
    }

    handlePointer(x, y) {
      if (this.state === 'ready') this.state = 'playing';
      if (this.state !== 'playing' || this.aiThinking) return true;
      const metrics = this.lastMetrics;
      if (!metrics) return true;
      const cell = this.pointFromPixel(x, y, metrics);
      if (!cell) return true;
      this.cursor = cell;
      this.playHuman(cell.x, cell.y);
      return true;
    }

    update() {}

    render(ctx, metrics) {
      this.lastMetrics = metrics;
      drawBoard(ctx, metrics, this);
    }

    playHuman(x, y) {
      if (this.state !== 'playing' || this.aiThinking) return;
      if (!this.isEmpty(x, y)) {
        this.message = '这里已经有棋子了。';
        return;
      }
      this.place(x, y, HUMAN);
      if (hasFive(this.board, HUMAN)) {
        this.score = 100 + this.moves.length;
        this.finish('win', '你连成五子。');
        return;
      }
      if (this.isFull()) {
        this.score = 50;
        this.finish('win', '棋盘已满，平局。');
        return;
      }
      this.askAiMove({ x, y });
    }

    async askAiMove(lastMove) {
      if (!this.host.invoke) {
        this.finish('lose', '无法连接 AI。');
        return;
      }
      this.aiThinking = true;
      this.message = 'BitCat 正在读棋。';
      try {
        const move = await this.host.invoke('cmd_gomoku_ai_move', {
          board: this.board,
          lastMove,
        });
        if (this.state !== 'playing') return;
        if (!move || !this.isEmpty(move.x, move.y)) {
          throw new Error('AI returned an invalid or occupied move');
        }
        this.place(move.x, move.y, AI);
        this.message = move.message || '轮到你了。';
        if (hasFive(this.board, AI)) {
          this.score = Math.max(0, 100 - this.moves.length);
          this.finish('lose', 'BitCat 连成五子。');
          return;
        }
        if (this.isFull()) {
          this.score = 50;
          this.finish('win', '棋盘已满，平局。');
        }
      } catch (e) {
        this.errorText = String(e);
        this.finish('lose', 'AI 落子失败。');
        this.host.log && this.host.log(`gomoku ai failed: ${e}`);
      } finally {
        this.aiThinking = false;
      }
    }

    place(x, y, stone) {
      this.board[y][x] = stone;
      this.lastMove = { x, y, stone, move: this.moves.length + 1 };
      this.moves.push(this.lastMove);
      playMoveSound(stone);
      this.logEvent('move', {
        move: this.lastMove.move,
        side: stone === HUMAN ? 'human' : 'ai',
        x,
        y,
      });
    }

    finish(result, message) {
      this.state = result;
      this.message = message || this.message;
      this.finishedAt = this.finishedAt || new Date().toISOString();
      playFinishSound(result);
      this.recordGame(result);
    }

    logEvent(type, detail) {
      this.host.log && this.host.log(`gomoku ${type} ${JSON.stringify(detail)}`);
    }

    recordGame(result) {
      if (this.recorded) return;
      this.recorded = true;
      const record = {
        game_type: 'gomoku',
        session_id: this.sessionId,
        started_at: this.startedAt,
        finished_at: this.finishedAt || new Date().toISOString(),
        result,
        score: this.score,
        message: this.message,
        error: this.errorText || null,
        board_size: BOARD_SIZE,
        final_board: this.board,
        moves: this.moves.map((move) => ({
          move: move.move,
          side: move.stone === HUMAN ? 'human' : 'ai',
          stone: move.stone,
          x: move.x,
          y: move.y,
        })),
      };
      if (this.host.invoke) {
        this.host.invoke('cmd_gomoku_record_game', { record }).catch((e) => {
          this.host.log && this.host.log(`gomoku record failed: ${e}`);
        });
      }
    }

    isEmpty(x, y) {
      return inBounds(x, y) && this.board[y][x] === EMPTY;
    }

    isFull() {
      return this.board.every((row) => row.every((cell) => cell !== EMPTY));
    }

    pointFromPixel(px, py, metrics) {
      const board = boardMetrics(metrics);
      const x = Math.round((px - board.left) / board.gap);
      const y = Math.round((py - board.top) / board.gap);
      if (!inBounds(x, y)) return null;
      const ix = board.left + x * board.gap;
      const iy = board.top + y * board.gap;
      if (Math.hypot(px - ix, py - iy) > board.gap * 0.48) return null;
      return { x, y };
    }
  }

  function drawBoard(ctx, metrics, engine) {
    const board = boardMetrics(metrics);
    ctx.save();
    ctx.clearRect(0, 0, metrics.width, metrics.height);
    drawGomokuBackdrop(ctx, metrics);
    drawBoardSurface(ctx, board);
    drawCoordinates(ctx, board);

    ctx.strokeStyle = 'rgba(70, 48, 28, 0.72)';
    ctx.lineWidth = Math.max(1, board.gap * 0.018);
    for (let i = 0; i < BOARD_SIZE; i++) {
      const x = board.left + i * board.gap;
      const y = board.top + i * board.gap;
      ctx.beginPath();
      ctx.moveTo(board.left, y);
      ctx.lineTo(board.right, y);
      ctx.moveTo(x, board.top);
      ctx.lineTo(x, board.bottom);
      ctx.stroke();
    }

    [3, 7, 11].forEach((x) => {
      [3, 7, 11].forEach((y) => {
        drawDot(ctx, board.left + x * board.gap, board.top + y * board.gap, Math.max(3, board.gap * 0.075), 'rgba(58, 38, 22, 0.82)');
      });
    });

    for (let y = 0; y < BOARD_SIZE; y++) {
      for (let x = 0; x < BOARD_SIZE; x++) {
        const stone = engine.board[y][x];
        if (stone) {
          drawStone(ctx, board.left + x * board.gap, board.top + y * board.gap, board.gap * 0.39, stone);
        }
      }
    }

    drawCursor(ctx, board, engine.cursor);
    if (engine.lastMove) drawLastMove(ctx, board, engine.lastMove);
    drawStatusPanel(ctx, metrics, board, engine);
    if (engine.aiThinking) drawThinking(ctx, metrics, board);
    ctx.restore();
  }

  function boardMetrics(metrics) {
    const maxByHeight = metrics.height - 164;
    const maxByWidth = metrics.width - 160;
    const clamped = clamp(Math.min(maxByHeight, maxByWidth), 420, Math.min(metrics.width, metrics.height) - 64);
    const x = Math.floor((metrics.width - clamped) / 2);
    const y = Math.floor((metrics.height - clamped) / 2) - 8;
    const padding = clamped * 0.085;
    const gap = (clamped - padding * 2) / (BOARD_SIZE - 1);
    return {
      x,
      y,
      size: clamped,
      left: x + padding,
      top: y + padding,
      right: x + clamped - padding,
      bottom: y + clamped - padding,
      gap,
    };
  }

  function drawGomokuBackdrop(ctx, metrics) {
    const bg = ctx.createLinearGradient(0, 0, metrics.width, metrics.height);
    bg.addColorStop(0, 'rgba(9, 13, 18, 0.94)');
    bg.addColorStop(0.52, 'rgba(22, 24, 25, 0.9)');
    bg.addColorStop(1, 'rgba(8, 10, 12, 0.95)');
    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, metrics.width, metrics.height);
  }

  function drawBoardSurface(ctx, board) {
    ctx.save();
    ctx.shadowColor = 'rgba(0, 0, 0, 0.38)';
    ctx.shadowBlur = 26;
    ctx.shadowOffsetY = 16;
    roundRect(ctx, board.x, board.y, board.size, board.size, 8);
    const wood = ctx.createLinearGradient(board.x, board.y, board.x + board.size, board.y + board.size);
    wood.addColorStop(0, '#f0cf8b');
    wood.addColorStop(0.46, '#ddb36d');
    wood.addColorStop(1, '#c99654');
    ctx.fillStyle = wood;
    ctx.fill();
    ctx.shadowColor = 'transparent';
    ctx.strokeStyle = 'rgba(255, 248, 218, 0.72)';
    ctx.lineWidth = 2;
    ctx.stroke();

    ctx.globalAlpha = 0.16;
    ctx.strokeStyle = '#8c5f31';
    ctx.lineWidth = 1;
    for (let i = 0; i < 9; i++) {
      const y = board.y + board.size * (0.12 + i * 0.095);
      ctx.beginPath();
      ctx.moveTo(board.x + 18, y);
      ctx.bezierCurveTo(board.x + board.size * 0.34, y - 8, board.x + board.size * 0.62, y + 9, board.x + board.size - 18, y - 2);
      ctx.stroke();
    }
    ctx.restore();
  }

  function drawCoordinates(ctx, board) {
    const labels = ['一', '二', '三', '四', '五', '六', '七', '八', '九', '十', '十一', '十二', '十三', '十四', '十五'];
    ctx.save();
    ctx.fillStyle = 'rgba(74, 50, 28, 0.72)';
    ctx.font = `600 ${Math.max(10, board.gap * 0.24)}px "Microsoft YaHei", sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (let i = 0; i < BOARD_SIZE; i++) {
      const x = board.left + i * board.gap;
      const y = board.top + i * board.gap;
      ctx.fillText(String(i + 1), x, board.top - board.gap * 0.55);
      ctx.fillText(labels[i], board.left - board.gap * 0.7, y);
    }
    ctx.restore();
  }

  function drawStone(ctx, x, y, r, stone) {
    ctx.save();
    ctx.shadowColor = 'rgba(0, 0, 0, 0.34)';
    ctx.shadowBlur = r * 0.32;
    ctx.shadowOffsetY = r * 0.18;
    const grad = ctx.createRadialGradient(x - r * 0.35, y - r * 0.4, r * 0.2, x, y, r);
    if (stone === HUMAN) {
      grad.addColorStop(0, '#5f6871');
      grad.addColorStop(0.32, '#252b31');
      grad.addColorStop(1, '#050607');
    } else {
      grad.addColorStop(0, '#ffffff');
      grad.addColorStop(0.46, '#e9edf1');
      grad.addColorStop(1, '#aeb8c2');
    }
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.shadowColor = 'transparent';
    ctx.strokeStyle = stone === HUMAN ? 'rgba(255,255,255,0.18)' : 'rgba(51, 60, 70, 0.28)';
    ctx.lineWidth = 1.2;
    ctx.stroke();
    ctx.restore();
  }

  function drawCursor(ctx, board, cursor) {
    const x = board.left + cursor.x * board.gap;
    const y = board.top + cursor.y * board.gap;
    const r = board.gap * 0.46;
    ctx.save();
    ctx.strokeStyle = 'rgba(38, 142, 188, 0.95)';
    ctx.lineWidth = 2;
    const len = r * 0.56;
    [[-1, -1], [1, -1], [1, 1], [-1, 1]].forEach(([sx, sy]) => {
      ctx.beginPath();
      ctx.moveTo(x + sx * r, y + sy * (r - len));
      ctx.lineTo(x + sx * r, y + sy * r);
      ctx.lineTo(x + sx * (r - len), y + sy * r);
      ctx.stroke();
    });
    ctx.restore();
  }

  function drawLastMove(ctx, board, move) {
    drawDot(
      ctx,
      board.left + move.x * board.gap,
      board.top + move.y * board.gap,
      Math.max(3.2, board.gap * 0.105),
      move.stone === HUMAN ? '#f5f8fb' : '#1b222a'
    );
  }

  function drawStatusPanel(ctx, metrics, board, engine) {
    const panelY = board.y + board.size + 14;
    const panelH = 46;
    ctx.save();
    roundRect(ctx, board.x, panelY, board.size, panelH, 8);
    ctx.fillStyle = 'rgba(13, 18, 23, 0.82)';
    ctx.fill();
    ctx.strokeStyle = 'rgba(255,255,255,0.08)';
    ctx.stroke();

    const turnText = engine.aiThinking ? 'BitCat 思考中' : '轮到你落子';
    ctx.fillStyle = '#f7fbff';
    ctx.font = '800 15px "Microsoft YaHei", "Segoe UI", sans-serif';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText(turnText, board.x + 18, panelY + panelH / 2);

    ctx.fillStyle = 'rgba(226, 235, 242, 0.82)';
    ctx.font = '600 13px "Microsoft YaHei", "Segoe UI", sans-serif';
    ctx.textAlign = 'right';
    ctx.fillText(engine.message || '五子连线即胜', board.x + board.size - 18, panelY + panelH / 2);
    ctx.restore();
  }

  function drawThinking(ctx, metrics, board) {
    const cx = board.x + board.size - 38;
    const cy = board.y + board.size + 37;
    const t = performance.now() / 280;
    ctx.save();
    for (let i = 0; i < 3; i++) {
      ctx.globalAlpha = 0.35 + 0.45 * ((Math.sin(t + i * 1.7) + 1) / 2);
      drawDot(ctx, cx + i * 10, cy, 3.2, '#e8c472');
    }
    ctx.restore();
  }

  function hasFive(board, stone) {
    for (let y = 0; y < BOARD_SIZE; y++) {
      for (let x = 0; x < BOARD_SIZE; x++) {
        if (board[y][x] !== stone) continue;
        for (const [dx, dy] of DIRS) {
          let count = 1;
          for (let step = 1; step < 5; step++) {
            const nx = x + dx * step;
            const ny = y + dy * step;
            if (!inBounds(nx, ny) || board[ny][nx] !== stone) break;
            count += 1;
          }
          if (count >= 5) return true;
        }
      }
    }
    return false;
  }

  let audioCtx = null;

  function getAudioContext() {
    const Ctx = window.AudioContext || window.webkitAudioContext;
    if (!Ctx) return null;
    if (!audioCtx) audioCtx = new Ctx();
    if (audioCtx.state === 'suspended') audioCtx.resume().catch(() => {});
    return audioCtx;
  }

  function playMoveSound(stone) {
    const ctx = getAudioContext();
    if (!ctx) return;
    const now = ctx.currentTime;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    const filter = ctx.createBiquadFilter();
    osc.type = 'sine';
    osc.frequency.setValueAtTime(stone === HUMAN ? 260 : 340, now);
    osc.frequency.exponentialRampToValueAtTime(stone === HUMAN ? 150 : 210, now + 0.08);
    filter.type = 'lowpass';
    filter.frequency.setValueAtTime(900, now);
    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(stone === HUMAN ? 0.09 : 0.07, now + 0.006);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.12);
    osc.connect(filter);
    filter.connect(gain);
    gain.connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.14);
  }

  function playFinishSound(result) {
    const ctx = getAudioContext();
    if (!ctx || result === 'cancel') return;
    const notes = result === 'win' ? [392, 494, 659] : [330, 247, 196];
    notes.forEach((freq, index) => {
      const now = ctx.currentTime + index * 0.08;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = 'triangle';
      osc.frequency.setValueAtTime(freq, now);
      gain.gain.setValueAtTime(0.0001, now);
      gain.gain.exponentialRampToValueAtTime(0.055, now + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.16);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(now);
      osc.stop(now + 0.18);
    });
  }

  function inBounds(x, y) {
    return Number.isInteger(x) && Number.isInteger(y) && x >= 0 && y >= 0 && x < BOARD_SIZE && y < BOARD_SIZE;
  }

  function clamp(n, min, max) {
    return Math.max(min, Math.min(max, n));
  }

  function drawDot(ctx, x, y, r, color) {
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
  }

  function roundRect(ctx, x, y, w, h, r) {
    ctx.beginPath();
    ctx.moveTo(x + r, y);
    ctx.arcTo(x + w, y, x + w, y + h, r);
    ctx.arcTo(x + w, y + h, x, y + h, r);
    ctx.arcTo(x, y + h, x, y, r);
    ctx.arcTo(x, y, x + w, y, r);
    ctx.closePath();
  }

  window.BitCatGames.gomoku = (config, host) => new GomokuEngine(config, host);
  window.GomokuEngineTest = { GomokuEngine, hasFive };
})();
