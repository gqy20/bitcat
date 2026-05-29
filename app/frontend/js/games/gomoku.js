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
      this.startedAtMs = Date.now();
      this.lastMoveAtMs = this.startedAtMs;
      this.finishedAt = null;
      this.finishedAtMs = null;
      this.recorded = false;
      this.pendingAiTiming = null;
      this.aiThoughts = [];
      this.commentaries = [];
      this.commentaryLoading = false;
      this.lastCommentaryMove = 0;
      this.activeRecommendation = null;
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
      const recommendation = this.recommendationFromPixel(x, y, metrics);
      if (recommendation) {
        this.focusRecommendation(recommendation);
        return true;
      }
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
      this.pendingAiTiming = {
        started_at: new Date().toISOString(),
        started_ms: Date.now(),
      };
      try {
        const move = await this.host.invoke('cmd_gomoku_ai_move', {
          board: cloneBoard(this.board),
          lastMove,
        });
        if (this.state !== 'playing') return;
        if (!move || !this.isEmpty(move.x, move.y)) {
          throw new Error('AI returned an invalid or occupied move');
        }
        this.place(move.x, move.y, AI);
        this.attachAiThought(move);
        this.message = move.message || '轮到你了。';
        if (hasFive(this.board, AI)) {
          this.score = Math.max(0, 100 - this.moves.length);
          this.finish('lose', 'BitCat 连成五子。');
          return;
        }
        if (this.isFull()) {
          this.score = 50;
          this.finish('win', '棋盘已满，平局。');
          return;
        }
        this.maybeRequestCommentary();
      } catch (e) {
        this.errorText = String(e);
        this.finish('lose', 'AI 落子失败。');
        this.host.log && this.host.log(`gomoku ai failed: ${e}`);
      } finally {
        this.aiThinking = false;
        this.pendingAiTiming = null;
      }
    }

    attachAiThought(move) {
      const current = this.lastMove;
      if (!current || current.stone !== AI) return;
      current.ai_message = move.message || null;
      current.ai_thought = move.thought || move.line_summary || move.message || '这手先稳住局势。';
      current.ai_lookahead = normalizeLookahead(move);
      current.ai_reason = move.reason || null;
      current.ai_risk = move.risk || null;
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
        reason: current.ai_reason,
        risk: current.ai_risk,
        lookahead: current.ai_lookahead,
        ai_elapsed_ms: current.ai_elapsed_ms || null,
      });
      this.aiThoughts = this.aiThoughts.slice(-8);
    }

    maybeRequestCommentary() {
      if (!this.host.invoke || this.commentaryLoading || this.state !== 'playing') return;
      if (this.moves.length < 4 || this.moves.length % 2 !== 0) return;
      if (this.lastCommentaryMove === this.moves.length) return;
      const requestedMove = this.moves.length;
      this.lastCommentaryMove = requestedMove;
      this.commentaryLoading = true;
      const lastMove = this.lastMove ? { x: this.lastMove.x, y: this.lastMove.y } : null;
      this.host
        .invoke('cmd_gomoku_commentary', {
          board: cloneBoard(this.board),
          lastMove,
        })
        .then((commentary) => {
          if (!commentary) return;
          this.commentaries.push({
            move: requestedMove,
            summary: commentary.summary || '',
            advantage: commentary.advantage || 'balanced',
            key_points: Array.isArray(commentary.key_points)
              ? commentary.key_points.map((point) => formatCommentaryPoint(point)).filter(Boolean)
              : [],
            recommendations: Array.isArray(commentary.recommendations)
              ? commentary.recommendations.map((item) => normalizeRecommendation(item)).filter(Boolean)
              : [],
            suggestion: commentary.suggestion || '',
            created_at: new Date().toISOString(),
          });
          this.commentaries = this.commentaries.slice(-5);
        })
        .catch((e) => {
          this.host.log && this.host.log(`gomoku commentary failed: ${e}`);
        })
        .finally(() => {
          this.commentaryLoading = false;
        });
    }

    place(x, y, stone) {
      this.board[y][x] = stone;
      const nowMs = Date.now();
      const turnElapsedMs = Math.max(0, nowMs - this.lastMoveAtMs);
      this.lastMove = {
        x,
        y,
        stone,
        move: this.moves.length + 1,
        played_at: new Date(nowMs).toISOString(),
        elapsed_ms: Math.max(0, nowMs - this.startedAtMs),
        turn_elapsed_ms: turnElapsedMs,
      };
      this.lastMoveAtMs = nowMs;
      this.moves.push(this.lastMove);
      playMoveSound(stone);
      this.logEvent('move', {
        move: this.lastMove.move,
        side: stone === HUMAN ? 'human' : 'ai',
        x,
        y,
        played_at: this.lastMove.played_at,
        elapsed_ms: this.lastMove.elapsed_ms,
        turn_elapsed_ms: this.lastMove.turn_elapsed_ms,
      });
    }

    finish(result, message) {
      this.state = result;
      this.message = message || this.message;
      this.finishedAt = this.finishedAt || new Date().toISOString();
      this.finishedAtMs = this.finishedAtMs || Date.now();
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
        duration_ms: Math.max(0, (this.finishedAtMs || Date.now()) - this.startedAtMs),
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
          played_at: move.played_at || null,
          elapsed_ms: Number.isFinite(move.elapsed_ms) ? move.elapsed_ms : null,
          turn_elapsed_ms: Number.isFinite(move.turn_elapsed_ms) ? move.turn_elapsed_ms : null,
          ai_message: move.ai_message || null,
          ai_thought: move.ai_thought || null,
          ai_reason: move.ai_reason || null,
          ai_risk: move.ai_risk || null,
          ai_lookahead: move.ai_lookahead || null,
          ai_started_at: move.ai_started_at || null,
          ai_finished_at: move.ai_finished_at || null,
          ai_elapsed_ms: Number.isFinite(move.ai_elapsed_ms) ? move.ai_elapsed_ms : null,
        })),
        ai_thoughts: this.aiThoughts,
        commentaries: this.commentaries,
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

    recommendationFromPixel(px, py, metrics) {
      const layout = this.lastCommentaryLayout || [];
      return layout.find((item) => px >= item.x && px <= item.x + item.w && py >= item.y && py <= item.y + item.h)?.recommendation || null;
    }

    focusRecommendation(recommendation) {
      if (!recommendation || !Array.isArray(recommendation.coord)) return;
      const [x, y] = recommendation.coord;
      if (!inBounds(x, y)) return;
      this.cursor = { x, y };
      this.activeRecommendation = { x, y };
      this.message = recommendation.text || '已跳到推荐点。';
    }
  }

  function drawBoard(ctx, metrics, engine) {
    const board = boardMetrics(metrics);
    ctx.save();
    ctx.clearRect(0, 0, metrics.width, metrics.height);
    drawGomokuBackdrop(ctx, metrics);
    drawBoardSurface(ctx, board);
    drawCoordinates(ctx, board);
    engine.lastCommentaryLayout = [];
    drawSidePanels(ctx, metrics, board, engine);

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

    drawCommentaryMarkers(ctx, board, engine);
    drawCursor(ctx, board, engine.cursor);
    if (engine.lastMove) drawLastMove(ctx, board, engine.lastMove);
    drawStatusPanel(ctx, metrics, board, engine);
    if (engine.aiThinking) drawThinking(ctx, metrics, board);
    ctx.restore();
  }

  function boardMetrics(metrics) {
    const maxByHeight = metrics.height - 164;
    const sideReserve = metrics.width >= 1180 ? 560 : metrics.width >= 980 ? 420 : 160;
    const maxByWidth = metrics.width - sideReserve;
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

  function drawSidePanels(ctx, metrics, board, engine) {
    if (metrics.width < 940) return;
    const gap = 24;
    const availableSide = Math.max(180, (metrics.width - board.size) / 2 - gap * 1.2);
    const panelW = Math.min(360, availableSide);
    const panelH = Math.min(board.size + 36, metrics.height - 96);
    const y = board.y + Math.max(0, (board.size - panelH) / 2);
    const leftX = Math.max(18, board.x - gap - panelW);
    const rightX = Math.min(metrics.width - panelW - 18, board.x + board.size + gap);
    drawThoughtPanel(ctx, leftX, y, panelW, panelH, engine);
    drawCommentaryPanel(ctx, rightX, y, panelW, panelH, engine);
  }

  function drawInfoPanelFrame(ctx, x, y, w, h, title) {
    ctx.save();
    roundRect(ctx, x, y, w, h, 8);
    ctx.fillStyle = 'rgba(12, 17, 22, 0.76)';
    ctx.fill();
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.1)';
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.fillStyle = '#f7fbff';
    ctx.font = '800 17px "Microsoft YaHei", "Segoe UI", sans-serif';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    ctx.fillText(title, x + 16, y + 14);
    ctx.restore();
  }

  function drawThoughtPanel(ctx, x, y, w, h, engine) {
    drawInfoPanelFrame(ctx, x, y, w, h, 'BitCat 的思考');
    const items = engine.aiThoughts || [];
    ctx.save();
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    if (!items.length) {
      ctx.fillStyle = 'rgba(226, 235, 242, 0.62)';
      ctx.font = '600 15px "Microsoft YaHei", "Segoe UI", sans-serif';
      wrapText(ctx, '等我落子后，这里会记录每一手的想法。', x + 16, y + 50, w - 32, 22, 3);
      ctx.restore();
      return;
    }
    let cy = y + 52;
    items.slice(-4).forEach((item) => {
      ctx.fillStyle = 'rgba(232, 196, 114, 0.95)';
      ctx.font = '800 14px "Microsoft YaHei", "Segoe UI", sans-serif';
      ctx.fillText(`第 ${item.move} 手  ${formatCoord([item.x, item.y])}${item.ai_elapsed_ms ? ` · ${formatDuration(item.ai_elapsed_ms)}` : ''}`, x + 16, cy);
      cy += 22;
      const line = formatThoughtLine(item);
      if (line) {
        ctx.fillStyle = 'rgba(132, 206, 168, 0.9)';
        ctx.font = '800 13px "Microsoft YaHei", "Segoe UI", sans-serif';
        cy += wrapText(ctx, line, x + 16, cy, w - 32, 18, 3) + 6;
      }
      ctx.fillStyle = 'rgba(230, 238, 245, 0.82)';
      ctx.font = '600 14px "Microsoft YaHei", "Segoe UI", sans-serif';
      cy += wrapText(ctx, item.text || '这手先稳住局势。', x + 16, cy, w - 32, 21, 5) + 14;
    });
    ctx.restore();
  }

  function formatThoughtLine(item) {
    const parts = [];
    if (item.reason) parts.push(moveReasonLabel(item.reason));
    if (item.risk) parts.push(moveRiskLabel(item.risk));
    const lookahead = item.lookahead || {};
    const replies = [];
    if (lookahead.black_best_reply) replies.push(`黑棋 ${formatCoord(lookahead.black_best_reply)}`);
    if (lookahead.white_followup) replies.push(`白棋 ${formatCoord(lookahead.white_followup)}`);
    if (replies.length) parts.push(`预读 ${replies.join(' → ')}`);
    if (lookahead.line_eval) parts.push(lineEvalLabel(lookahead.line_eval));
    return parts.filter(Boolean).join(' · ');
  }

  function drawCommentaryPanel(ctx, x, y, w, h, engine) {
    drawInfoPanelFrame(ctx, x, y, w, h, '局势点评');
    const latest = engine.commentaries && engine.commentaries[engine.commentaries.length - 1];
    ctx.save();
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    let cy = y + 52;
    if (!latest) {
      engine.lastCommentaryLayout = [];
      ctx.fillStyle = 'rgba(226, 235, 242, 0.62)';
      ctx.font = '600 15px "Microsoft YaHei", "Segoe UI", sans-serif';
      wrapText(ctx, engine.commentaryLoading ? '正在观察棋势。' : '满两回合后，我会隔一手点评一次。', x + 16, cy, w - 32, 22, 4);
      ctx.restore();
      return;
    }
    ctx.fillStyle = advantageColor(latest.advantage);
    ctx.font = '800 14px "Microsoft YaHei", "Segoe UI", sans-serif';
    ctx.fillText(`${advantageLabel(latest.advantage)} · 第 ${latest.move} 手`, x + 16, cy);
    cy += 27;
    ctx.fillStyle = '#f7fbff';
    ctx.font = '700 15px "Microsoft YaHei", "Segoe UI", sans-serif';
    cy += wrapText(ctx, latest.summary || '局势仍在展开。', x + 16, cy, w - 32, 22, 5) + 14;
    const bottom = y + h - 42;
    const recommendations = latest.recommendations || [];
    recommendations.slice(0, 3).forEach((item) => {
      if (cy >= bottom) return;
      const rowY = cy;
      const label = `${recommendationPriorityLabel(item.priority)} ${recommendationReasonLabel(item.reason)} ${formatCoord(item.coord)} ${item.text || ''}`.trim();
      ctx.fillStyle = recommendationColor(item.priority);
      ctx.font = '800 14px "Microsoft YaHei", "Segoe UI", sans-serif';
      const maxLines = Math.max(1, Math.min(5, Math.floor((bottom - cy) / 21)));
      const used = wrapText(ctx, `→ ${label}`, x + 16, cy, w - 32, 21, maxLines);
      engine.lastCommentaryLayout.push({ x: x + 10, y: rowY - 3, w: w - 20, h: used + 6, recommendation: item });
      cy += used + 8;
    });
    const points = latest.key_points || [];
    points.slice(0, 3).forEach((point) => {
      if (cy >= bottom) return;
      ctx.fillStyle = 'rgba(230, 238, 245, 0.76)';
      ctx.font = '600 14px "Microsoft YaHei", "Segoe UI", sans-serif';
      const maxLines = Math.max(1, Math.min(5, Math.floor((bottom - cy) / 21)));
      cy += wrapText(ctx, `· ${point}`, x + 16, cy, w - 32, 21, maxLines) + 7;
    });
    if (latest.suggestion && cy < bottom) {
      cy += 6;
      ctx.fillStyle = 'rgba(232, 196, 114, 0.92)';
      ctx.font = '700 14px "Microsoft YaHei", "Segoe UI", sans-serif';
      const maxLines = Math.max(2, Math.floor((bottom - cy) / 21));
      wrapText(ctx, latest.suggestion, x + 16, cy, w - 32, 21, maxLines);
    }
    if (engine.commentaryLoading) {
      ctx.fillStyle = 'rgba(226, 235, 242, 0.5)';
      ctx.font = '600 11px "Microsoft YaHei", "Segoe UI", sans-serif';
      ctx.fillText('更新点评中...', x + 16, y + h - 28);
    }
    ctx.restore();
  }

  function advantageLabel(advantage) {
    if (advantage === 'human') return '黑棋主动';
    if (advantage === 'ai') return '白棋主动';
    return '势均力敌';
  }

  function advantageColor(advantage) {
    if (advantage === 'human') return 'rgba(245, 248, 251, 0.94)';
    if (advantage === 'ai') return 'rgba(232, 196, 114, 0.94)';
    return 'rgba(132, 206, 168, 0.94)';
  }

  function formatCommentaryPoint(point) {
    if (!point) return '';
    if (typeof point === 'string') return point;
    const side = commentarySideLabel(point.side);
    const kind = commentaryKindLabel(point.kind);
    const coord = Array.isArray(point.coord) && point.coord.length === 2 ? `(${Number(point.coord[0]) + 1}, ${Number(point.coord[1]) + 1})` : '';
    const text = point.text || '';
    return [side, kind, coord, text].filter(Boolean).join(' ');
  }

  function normalizeRecommendation(item) {
    if (!item || !Array.isArray(item.coord) || item.coord.length !== 2) return null;
    const x = Number(item.coord[0]);
    const y = Number(item.coord[1]);
    if (!inBounds(x, y)) return null;
    return {
      coord: [x, y],
      priority: item.priority || 'interesting',
      reason: item.reason || 'stabilize',
      text: item.text || '',
    };
  }

  function normalizeLookahead(lookahead) {
    if (!lookahead || !Array.isArray(lookahead.lookahead_candidate) || lookahead.lookahead_candidate.length !== 2) return null;
    return {
      candidate: normalizeCoord(lookahead.lookahead_candidate),
      black_best_reply: normalizeCoord(lookahead.black_best_reply),
      white_followup: normalizeCoord(lookahead.white_followup),
      line_eval: lookahead.line_eval || null,
    };
  }

  function normalizeCoord(coord) {
    if (!Array.isArray(coord) || coord.length !== 2) return null;
    const x = Number(coord[0]);
    const y = Number(coord[1]);
    return inBounds(x, y) ? [x, y] : null;
  }

  function drawCommentaryMarkers(ctx, board, engine) {
    const latest = engine.commentaries && engine.commentaries[engine.commentaries.length - 1];
    const recommendations = latest && Array.isArray(latest.recommendations) ? latest.recommendations : [];
    if (!recommendations.length) return;
    ctx.save();
    recommendations.slice(0, 3).forEach((item, index) => {
      const [gx, gy] = item.coord || [];
      if (!inBounds(gx, gy)) return;
      const x = board.left + gx * board.gap;
      const y = board.top + gy * board.gap;
      const active = engine.activeRecommendation && engine.activeRecommendation.x === gx && engine.activeRecommendation.y === gy;
      const r = board.gap * (active ? 0.5 : 0.42);
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.strokeStyle = recommendationColor(item.priority);
      ctx.lineWidth = active ? 3 : 2;
      ctx.setLineDash(item.reason === 'block' ? [5, 4] : []);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.fillStyle = recommendationColor(item.priority);
      ctx.font = `800 ${Math.max(10, board.gap * 0.28)}px "Microsoft YaHei", sans-serif`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(String(index + 1), x, y);
    });
    ctx.restore();
  }

  function formatCoord(coord) {
    return Array.isArray(coord) && coord.length === 2 ? `(${Number(coord[0]) + 1}, ${Number(coord[1]) + 1})` : '';
  }

  function recommendationPriorityLabel(priority) {
    if (priority === 'urgent') return '急所';
    if (priority === 'best') return '首选';
    return '可选';
  }

  function recommendationReasonLabel(reason) {
    if (reason === 'win') return '成五';
    if (reason === 'block') return '防守';
    if (reason === 'fork') return '造双';
    if (reason === 'extend') return '延伸';
    return '稳形';
  }

  function moveReasonLabel(reason) {
    if (reason === 'win_now') return '成五';
    if (reason === 'block_immediate_win') return '挡冲五';
    if (reason === 'create_fork') return '造双';
    if (reason === 'block_fork') return '挡双';
    if (reason === 'desperate_block') return '强防';
    return '布局';
  }

  function moveRiskLabel(risk) {
    if (risk === 'safe') return '安全';
    if (risk === 'allows_human_single_threat') return '留单威胁';
    if (risk === 'allows_human_fork') return '防双风险';
    if (risk === 'forced_loss') return '败势';
    return '';
  }

  function lineEvalLabel(evalLabel) {
    if (evalLabel === 'white_win') return '白棋胜势';
    if (evalLabel === 'stable') return '局面稳定';
    if (evalLabel === 'dangerous') return '仍有危险';
    if (evalLabel === 'losing') return '难以挽回';
    return '变化未明';
  }

  function formatDuration(ms) {
    if (!Number.isFinite(ms)) return '';
    if (ms < 1000) return `${Math.max(0, Math.round(ms))}ms`;
    if (ms < 60_000) return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s`;
    const totalSeconds = Math.floor(ms / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes}:${String(seconds).padStart(2, '0')}`;
  }

  function recommendationColor(priority) {
    if (priority === 'urgent') return 'rgba(238, 106, 86, 0.96)';
    if (priority === 'best') return 'rgba(232, 196, 114, 0.96)';
    return 'rgba(96, 190, 214, 0.92)';
  }

  function commentarySideLabel(side) {
    if (side === 'black') return '黑棋';
    if (side === 'white') return '白棋';
    if (side === 'both') return '双方';
    return '';
  }

  function commentaryKindLabel(kind) {
    if (kind === 'immediate_win') return '冲五';
    if (kind === 'fork') return '双威胁';
    if (kind === 'block') return '防守点';
    if (kind === 'extension') return '延伸';
    if (kind === 'shape') return '棋形';
    return '';
  }

  function wrapText(ctx, text, x, y, maxWidth, lineHeight, maxLines) {
    const chars = String(text || '').split('');
    let line = '';
    let lines = 0;
    let cy = y;
    for (const ch of chars) {
      const next = line + ch;
      if (line && ctx.measureText(next).width > maxWidth) {
        lines += 1;
        if (lines >= maxLines) {
          ctx.fillText(`${line.slice(0, Math.max(0, line.length - 1))}…`, x, cy);
          return lines * lineHeight;
        }
        ctx.fillText(line, x, cy);
        cy += lineHeight;
        line = ch;
      } else {
        line = next;
      }
    }
    if (line) {
      ctx.fillText(line, x, cy);
      lines += 1;
    }
    return Math.max(lineHeight, lines * lineHeight);
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

    const totalMs = Math.max(0, (engine.finishedAtMs || Date.now()) - engine.startedAtMs);
    const turnText = `${engine.aiThinking ? 'BitCat 思考中' : '轮到你落子'} · 第 ${engine.moves.length} 手 · 本局 ${formatDuration(totalMs)}`;
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

  function cloneBoard(board) {
    return board.map((row) => row.slice());
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
