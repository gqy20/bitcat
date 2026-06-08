(function () {
  window.BitCatGames = window.BitCatGames || {};

  function clamp(n, min, max) {
    return Math.max(min, Math.min(max, n));
  }

  function lerp(a, b, t) {
    return a + (b - a) * t;
  }

  function easeOutCubic(t) {
    return 1 - Math.pow(1 - t, 3);
  }

  function samePoint(a, b) {
    return a.x === b.x && a.y === b.y;
  }

  function toFinePoint(point) {
    return {
      x: point.x * SUBGRID + Math.floor(SUBGRID / 2),
      y: point.y * SUBGRID + Math.floor(SUBGRID / 2),
    };
  }

  function toCoarsePoint(point) {
    return {
      x: Math.floor(point.x / SUBGRID),
      y: Math.floor(point.y / SUBGRID),
    };
  }

  function sameCoarseCell(finePoint, coarsePoint) {
    const coarse = toCoarsePoint(finePoint);
    return coarse.x === coarsePoint.x && coarse.y === coarsePoint.y;
  }

  function keyOf(p) {
    return `${p.x},${p.y}`;
  }

  function coarseKeyOf(p) {
    const coarse = toCoarsePoint(p);
    return keyOf(coarse);
  }

  const SUBGRID = 4;
  const MAX_DIRECTION_QUEUE = 3;

  class SnakeEngine {
    constructor(config, host, rng) {
      this.config = config;
      this.host = host || {};
      this.rng = rng || Math.random;
      this.vocab = normalizeVocab(config.snake_vocab);
      this.gameKind = 'snake';
      this.supportsBoost = true;
      this.state = 'ready';
      this.score = 0;
      this.dir = { x: 1, y: 0 };
      this.directionQueue = [];
      this.stepMs = Number(config.player.speed_ms) || 140;
      this.tickMs = 0;
      this.ended = false;
      this.boostHeld = false;
      this.snake = [];
      this.prevSnake = [];
      this.growRemainder = 0;
      this.food = { x: 0, y: 0 };
      this.answerFoods = [];
      this.question = null;
      this.questionIndex = 0;
      this.correctCount = 0;
      this.wrongCount = 0;
      this.combo = 0;
      this.maxCombo = 0;
      this.lastReview = null;
      this.missed = [];
      this.foodEaten = 0;
      this.animMs = 0;
      this.foodBursts = [];
      this.effects = [];
      this.hudFeedback = null;
      this.boardFlashMs = 0;
      this.comboPulseMs = 0;
      this.questionPulseMs = 0;
      this.screenShakeMs = 0;
      this.reset();
    }

    reset() {
      const grid = this.config.grid;
      const len = clamp(Number(this.config.player.initial_length) || 3, 1, 10);
      const startX = Math.floor(grid.width / 2);
      const startY = Math.floor(grid.height / 2);
      const start = toFinePoint({ x: startX, y: startY });
      this.snake = [];
      for (let i = 0; i < len * SUBGRID; i++) {
        this.snake.push({ x: start.x - i, y: start.y });
      }
      this.prevSnake = this.snake.map((p) => ({ ...p }));
      this.directionQueue = [];
      this.growRemainder = 0;
      this.boardFlashMs = 0;
      this.comboPulseMs = 0;
      this.questionPulseMs = 0;
      this.screenShakeMs = 0;
      this.lastReview = null;
      this.hudFeedback = null;
      if (this.vocab) this.nextQuestion();
      else this.spawnFood();
    }

    getState() {
      return this.state;
    }

    readyText() {
      if (this.vocab) return '看英文，吃掉正确中文释义；空格 / Enter 可加速';
      return '方向键 / WASD 移动，吃到食物变长，A / 空格 / Enter 开始';
    }

    endText(result) {
      if (!this.vocab) return `得分 ${this.score}，Enter / A 再来一局，Esc / B 退出`;
      const missedTerms = Array.from(new Set(this.missed.map((item) => item.term))).slice(0, 3);
      const topic = this.vocab.topic ? `${this.vocab.topic} · ` : '';
      if (result === 'win') {
        return missedTerms.length
          ? `${topic}完成 ${this.correctCount}/${this.vocab.targetCorrect}。再练：${missedTerms.join('、')}`
          : `${topic}完成 ${this.correctCount}/${this.vocab.targetCorrect}，这局很稳。`;
      }
      return missedTerms.length
        ? `${topic}得分 ${this.score}。重点复习：${missedTerms.join('、')}`
        : `${topic}得分 ${this.score}，Enter / A 再来一局。`;
    }

    hudText() {
      return `${this.coarseLength()}${this.boostHeld ? ' BOOST' : ''}`;
    }

    handleInput(input) {
      if (!input) return;
      this.host.log && this.host.log(`input type=${input.type || ''} dx=${input.dx ?? ''} dy=${input.dy ?? ''} state=${this.state}`);
      if (this.ended) {
        if (input.type === 'confirm') this.host.restartGame && this.host.restartGame();
        else if (input.type === 'cancel') this.host.closeEndedGame && this.host.closeEndedGame(this.state);
        return;
      }
      if (input.type === 'confirm' && this.state === 'ready') {
        this.state = 'playing';
        return;
      }
      if (input.type === 'boost') {
        this.boostHeld = Boolean(input.active);
        if (this.boostHeld && this.state === 'ready') this.state = 'playing';
        return;
      }
      if (input.type === 'pause' && (this.state === 'playing' || this.state === 'paused')) {
        this.state = this.state === 'playing' ? 'paused' : 'playing';
        return;
      }
      if (input.type === 'cancel') {
        this.finish('cancel');
        return;
      }
      if (input.type === 'direction') {
        const next = { x: Math.sign(input.dx || 0), y: Math.sign(input.dy || 0) };
        if (Math.abs(next.x) + Math.abs(next.y) !== 1) return;
        if (this.state === 'ready') this.state = 'playing';
        this.enqueueDirection(next);
      }
    }

    update(dtMs) {
      this.animMs += dtMs;
      this.updateFoodBursts(dtMs);
      this.updateEffects(dtMs);
      this.boardFlashMs = Math.max(0, this.boardFlashMs - dtMs);
      this.comboPulseMs = Math.max(0, this.comboPulseMs - dtMs);
      this.questionPulseMs = Math.max(0, this.questionPulseMs - dtMs);
      this.screenShakeMs = Math.max(0, this.screenShakeMs - dtMs);
      if (this.state !== 'playing' || this.ended) return;
      this.tickMs += dtMs;
      while (this.tickMs >= this.effectiveFineStepMs() && this.state === 'playing') {
        this.tickMs -= this.effectiveFineStepMs();
        this.step();
      }
    }

    step() {
      this.prevSnake = this.snake.map((p) => ({ ...p }));
      this.dir = this.consumeQueuedDirection();
      const head = this.snake[0];
      let next = { x: head.x + this.dir.x, y: head.y + this.dir.y };
      const grid = this.config.grid;
      const rules = this.config.rules;
      const fineWidth = grid.width * SUBGRID;
      const fineHeight = grid.height * SUBGRID;

      if (!rules.walls_kill) {
        next = {
          x: (next.x + fineWidth) % fineWidth,
          y: (next.y + fineHeight) % fineHeight,
        };
      } else if (next.x < 0 || next.y < 0 || next.x >= fineWidth || next.y >= fineHeight) {
        this.finish('lose');
        return;
      }

      const eatenAnswer = this.vocab ? this.answerFoods.find((food) => sameCoarseCell(next, food)) : null;
      const ate = this.vocab ? Boolean(eatenAnswer) : sameCoarseCell(next, this.food);
      const growsOnThisMove = ate && (!this.vocab || eatenAnswer.correct);
      const tailMoves = !growsOnThisMove && this.growRemainder <= 0;
      const body = tailMoves ? this.snake.slice(0, -1) : this.snake;
      if (rules.self_kill && body.some((p) => samePoint(p, next))) {
        this.finish('lose');
        return;
      }

      this.snake.unshift(next);
      if (ate) {
        if (this.vocab) {
          const shouldGrow = this.consumeAnswer(eatenAnswer);
          this.settleTail(shouldGrow);
        } else {
          this.score += this.boostHeld ? 14 : 10;
          this.foodEaten += 1;
          this.addFoodBurst(this.food);
          this.stepMs = Math.max(40, Math.floor(this.stepMs * rules.speed_ramp));
          this.settleTail(true);
          if (this.coarseLength() >= rules.win_length) {
            this.finish('win');
            return;
          }
          this.spawnFood();
        }
      } else {
        this.settleTail(false);
      }
    }

    effectiveStepMs() {
      return this.boostHeld ? Math.max(28, Math.floor(this.stepMs * 0.55)) : this.stepMs;
    }

    effectiveFineStepMs() {
      return Math.max(12, this.effectiveStepMs() / SUBGRID);
    }

    coarseLength() {
      return Math.max(1, Math.ceil(this.snake.length / SUBGRID));
    }

    settleTail(shouldGrow) {
      if (shouldGrow) this.growRemainder += SUBGRID;
      if (this.growRemainder > 0) {
        this.growRemainder -= 1;
      } else {
        this.snake.pop();
      }
    }

    spawnFood() {
      const grid = this.config.grid;
      const occupied = new Set(this.snake.map(coarseKeyOf));
      const free = [];
      for (let y = 0; y < grid.height; y++) {
        for (let x = 0; x < grid.width; x++) {
          if (!occupied.has(`${x},${y}`)) free.push({ x, y });
        }
      }
      if (free.length === 0) {
        this.finish('win');
        return;
      }
      const choices = this.foodEaten < 20 ? this.centerFoodChoices(free) : free;
      this.food = choices[Math.floor(this.rng() * choices.length) % choices.length];
    }

    nextQuestion() {
      if (!this.vocab || this.correctCount >= this.vocab.targetCorrect) {
        this.finish('win');
        return;
      }
      const entry = this.vocab.entries[this.questionIndex % this.vocab.entries.length];
      this.questionIndex += 1;
      const choices = buildAnswerChoices(entry, this.vocab.entries, this.vocab.answerCount, this.rng);
      this.question = { ...entry, choices };
      this.spawnAnswerFoods(choices);
    }

    spawnAnswerFoods(choices) {
      const grid = this.config.grid;
      const occupied = new Set(this.snake.map(coarseKeyOf));
      const head = toCoarsePoint(this.snake[0]);
      const free = [];
      for (let y = 0; y < grid.height; y++) {
        for (let x = 0; x < grid.width; x++) {
          const point = { x, y };
          if (occupied.has(keyOf(point))) continue;
          if (distanceToSnakeHead(head, point) < 4) continue;
          if (isVocabHudCell(point, grid)) continue;
          free.push(point);
        }
      }
      const centered = this.foodEaten < 20 ? this.centerFoodChoices(free) : free;
      const pool = centered.length >= choices.length ? centered : free;
      shuffle(pool, this.rng);
      this.answerFoods = choices.map((choice, index) => ({
        ...(pool[index % pool.length] || { x: 0, y: 0 }),
        label: choice.label,
        correct: choice.correct,
      }));
      this.food = this.answerFoods.find((food) => food.correct) || this.answerFoods[0] || { x: 0, y: 0 };
    }

    consumeAnswer(food) {
      this.foodEaten += 1;
      this.addFoodBurst(food);
      if (food.correct) {
        this.rememberReview(food, true);
        this.score += this.boostHeld ? 18 : 14;
        this.correctCount += 1;
        this.combo += 1;
        this.maxCombo = Math.max(this.maxCombo, this.combo);
        this.stepMs = Math.max(48, Math.floor(this.stepMs * this.config.rules.speed_ramp));
        this.boardFlashMs = 220;
        this.comboPulseMs = 420;
        this.questionPulseMs = 380;
        this.setHudFeedback('正确', this.question, food, true, `+${this.boostHeld ? 18 : 14}`);
        if (this.correctCount >= this.vocab.targetCorrect) {
          this.finish('win');
          return true;
        }
        this.nextQuestion();
        return true;
      } else {
        this.rememberReview(food, false);
        this.score = Math.max(0, this.score - 4);
        this.wrongCount += 1;
        this.combo = 0;
        this.boardFlashMs = 180;
        this.screenShakeMs = 260;
        this.setHudFeedback('再记一次', this.question, food, false, '-4');
        if (this.question) {
          this.missed.push({
            id: this.question.id,
            term: this.question.term,
            meaning: this.question.meaning,
            picked: food.label,
            explanation: this.question.explanation || '',
          });
        }
        this.nextQuestion();
        return false;
      }
    }

    enqueueDirection(next) {
      if (!next || (next.x === 0 && next.y === 0)) return;
      const latest = this.directionQueue.length
        ? this.directionQueue[this.directionQueue.length - 1]
        : this.dir;
      if (samePoint(next, latest) || (next.x === -latest.x && next.y === -latest.y)) return;
      if (this.directionQueue.length >= MAX_DIRECTION_QUEUE) {
        this.directionQueue.shift();
      }
      this.directionQueue.push(next);
    }

    consumeQueuedDirection() {
      let current = this.dir;
      while (this.directionQueue.length) {
        const next = this.directionQueue.shift();
        if (samePoint(next, current)) continue;
        if (next.x === -current.x && next.y === -current.y) continue;
        current = next;
        break;
      }
      return current;
    }

    centerFoodChoices(free) {
      const grid = this.config.grid;
      const marginX = Math.max(3, Math.floor(grid.width * 0.24));
      const marginY = Math.max(3, Math.floor(grid.height * 0.22));
      const minX = marginX;
      const maxX = grid.width - marginX - 1;
      const minY = marginY;
      const maxY = grid.height - marginY - 1;
      const center = free.filter((point) => (
        point.x >= minX && point.x <= maxX && point.y >= minY && point.y <= maxY
      ));
      return center.length > 0 ? center : free;
    }

    addFoodBurst(point) {
      for (let i = 0; i < 10; i++) {
        const angle = (Math.PI * 2 * i) / 10 + this.rng() * 0.35;
        const speed = 0.0018 + this.rng() * 0.0012;
        this.foodBursts.push({
          x: point.x + 0.5,
          y: point.y + 0.5,
          vx: Math.cos(angle) * speed,
          vy: Math.sin(angle) * speed,
          age: 0,
          life: 420 + this.rng() * 180,
        });
      }
    }

    updateFoodBursts(dtMs) {
      for (const burst of this.foodBursts) {
        burst.age += dtMs;
        burst.x += burst.vx * dtMs;
        burst.y += burst.vy * dtMs;
        burst.vy += 0.000002 * dtMs;
      }
      this.foodBursts = this.foodBursts.filter((burst) => burst.age < burst.life);
    }

    addEffect(text, point, color, opts = {}) {
      this.effects.push({
        text,
        x: point.x,
        y: point.y,
        color,
        age: 0,
        scale: Number(opts.scale) || 1,
        stroke: opts.stroke || null,
      });
    }

    updateEffects(dtMs) {
      for (const effect of this.effects) effect.age += dtMs;
      this.effects = this.effects.filter((effect) => effect.age < 760);
      if (this.hudFeedback) {
        this.hudFeedback.age += dtMs;
        if (this.hudFeedback.age >= this.hudFeedback.life) this.hudFeedback = null;
      }
    }

    finish(result) {
      if (this.ended) return;
      this.ended = true;
      this.state = result;
    }

    render(ctx, metrics) {
      ctx.clearRect(0, 0, metrics.width, metrics.height);
      const grid = this.config.grid;
      const cell = metrics.cell;
      const ox = metrics.x;
      const oy = metrics.y;

      ctx.save();
      ctx.translate(ox, oy);
      fillBoardPanel(ctx, grid.width * cell, grid.height * cell);
      drawBoardFlash(ctx, grid, cell, this.boardFlashMs, this.comboPulseMs, this.screenShakeMs);
      drawSnakeField(ctx, grid, cell);
      if (this.vocab) {
        for (const food of this.answerFoods) {
          drawAnswerFood(ctx, food, cell, this.config.theme.food, this.animMs, this.questionPulseMs);
        }
      } else {
        drawFood(ctx, this.food, cell, this.config.theme.food, this.animMs);
      }
      drawFoodBursts(ctx, this.foodBursts, cell);
      drawEffects(ctx, this.effects, cell);
      drawSnake(ctx, this.renderSnake(), cell, this.dir, this.config.theme, this.animMs);
      if (this.boostHeld) drawBoostBadge(ctx, grid.width * cell, cell);
      ctx.restore();
      if (this.vocab) {
        const vocabProgress = {
          score: this.score,
          correct: this.correctCount,
          target: this.vocab.targetCorrect,
          combo: this.combo,
          wrong: this.wrongCount,
        };
        metrics.vocabProgress = vocabProgress;
        drawVocabTopPanel(ctx, this.question, metrics, vocabProgress, this.questionPulseMs);
        drawVocabBottomPanel(ctx, this.lastReview, this.hudFeedback, metrics, this.animMs);
      }
    }

    renderSnake() {
      if (this.state !== 'playing' || this.ended || this.prevSnake.length === 0) {
        return this.snake;
      }
      const progress = clamp(this.tickMs / this.effectiveFineStepMs(), 0, 1);
      const eased = easeOutCubic(progress);
      return this.snake.map((point, index) => {
        const previous = this.prevSnake[index] || this.prevSnake[this.prevSnake.length - 1] || point;
        if (Math.abs(point.x - previous.x) > 1 || Math.abs(point.y - previous.y) > 1) return point;
        return {
          x: lerp(previous.x, point.x, eased),
          y: lerp(previous.y, point.y, eased),
        };
      });
    }

    rememberReview(food, correct) {
      if (!this.question) return;
      this.lastReview = {
        term: this.question.term,
        meaning: this.question.meaning,
        example: this.question.example || '',
        hint: this.question.hint || '',
        explanation: this.question.explanation || '',
        picked: food && food.label ? food.label : '',
        correct: Boolean(correct),
      };
    }

    setHudFeedback(label, question, food, correct, delta) {
      if (!question) return;
      this.hudFeedback = {
        label,
        term: question.term,
        meaning: question.meaning,
        hint: question.hint || '',
        explanation: question.explanation || '',
        picked: food && food.label ? food.label : '',
        correct: Boolean(correct),
        delta,
        age: 0,
        life: correct ? 980 : 2200,
      };
    }
  }

  function fillBoardPanel(ctx, width, height) {
    const gradient = ctx.createLinearGradient(0, 0, width, height);
    gradient.addColorStop(0, 'rgba(9,15,20,0.58)');
    gradient.addColorStop(0.52, 'rgba(13,20,27,0.42)');
    gradient.addColorStop(1, 'rgba(7,11,17,0.62)');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);
  }

  function drawSnakeField(ctx, grid, cell) {
    const width = grid.width * cell;
    const height = grid.height * cell;
    ctx.save();
    ctx.strokeStyle = 'rgba(5,8,12,0.72)';
    ctx.lineWidth = 5;
    ctx.strokeRect(2.5, 2.5, width - 5, height - 5);
    ctx.strokeStyle = 'rgba(247,251,255,0.22)';
    ctx.lineWidth = 2;
    ctx.strokeRect(1, 1, width - 2, height - 2);
    ctx.strokeStyle = 'rgba(112,214,255,0.18)';
    ctx.lineWidth = 1;
    ctx.strokeRect(8.5, 8.5, width - 17, height - 17);
    if (cell >= 14) {
      ctx.fillStyle = 'rgba(247,251,255,0.042)';
      const dot = Math.max(1, cell * 0.055);
      for (let y = 1; y < grid.height; y += 3) {
        for (let x = 1; x < grid.width; x += 3) {
          ctx.beginPath();
          ctx.arc(x * cell, y * cell, dot, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    }
    ctx.restore();
  }

  function drawBoostBadge(ctx, width, cell) {
    const w = Math.max(76, cell * 5.5);
    const h = Math.max(24, cell * 1.45);
    const x = width - w - cell * 0.8;
    const y = cell * 0.8;
    ctx.save();
    ctx.fillStyle = 'rgba(255,209,102,0.20)';
    ctx.strokeStyle = 'rgba(255,209,102,0.86)';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, h / 2);
    ctx.fill();
    ctx.stroke();
    ctx.fillStyle = '#ffd166';
    ctx.font = `800 ${Math.max(13, cell * 0.72)}px "Segoe UI", sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('BOOST', x + w / 2, y + h / 2 + 1);
    ctx.restore();
  }

  function cellCenter(p, cell) {
    return {
      x: p.x * cell + cell / 2,
      y: p.y * cell + cell / 2,
    };
  }

  function finePointCenter(p, cell) {
    return {
      x: (p.x / SUBGRID) * cell,
      y: (p.y / SUBGRID) * cell,
    };
  }

  function drawSnake(ctx, snake, cell, dir, theme, timeMs = 0) {
    if (!snake.length) return;
    const bodyWidth = Math.max(8, cell * 0.72);
    const points = snake.map((p) => finePointCenter(p, cell));
    ctx.save();
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.shadowColor = 'rgba(0,0,0,0.28)';
    ctx.shadowBlur = Math.max(4, cell * 0.25);

    if (points.length > 1) {
      ctx.strokeStyle = 'rgba(32,36,42,0.36)';
      ctx.lineWidth = bodyWidth + Math.max(3, cell * 0.22);
      drawRoundedPolyline(ctx, points, cell);
      ctx.stroke();

      const gradient = ctx.createLinearGradient(points[points.length - 1].x, points[points.length - 1].y, points[0].x, points[0].y);
      gradient.addColorStop(0, 'rgba(112,214,255,0.82)');
      gradient.addColorStop(0.55, theme.body === 'trail' ? 'rgba(184,242,230,0.86)' : 'rgba(184,242,230,0.94)');
      gradient.addColorStop(1, '#ffd166');
      ctx.strokeStyle = gradient;
      ctx.lineWidth = bodyWidth;
      drawRoundedPolyline(ctx, points, cell);
      ctx.stroke();
    }

    drawSnakeTail(ctx, points[points.length - 1], bodyWidth);
    drawSnakeHead(ctx, points[0], cell, dir, timeMs);
    ctx.restore();
  }

  function drawRoundedPolyline(ctx, points, cell) {
    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);
    if (points.length === 2) {
      ctx.lineTo(points[1].x, points[1].y);
      return;
    }
    const radius = Math.max(2, cell * 0.48);
    for (let i = 1; i < points.length - 1; i++) {
      const prev = points[i - 1];
      const curr = points[i];
      const next = points[i + 1];
      const prevDx = prev.x - curr.x;
      const prevDy = prev.y - curr.y;
      const nextDx = next.x - curr.x;
      const nextDy = next.y - curr.y;
      if (prevDx === -nextDx && prevDy === -nextDy) {
        ctx.lineTo(curr.x, curr.y);
        continue;
      }
      const prevLen = Math.hypot(prevDx, prevDy) || 1;
      const nextLen = Math.hypot(nextDx, nextDy) || 1;
      const r = Math.min(radius, prevLen * 0.5, nextLen * 0.5);
      const cornerStart = {
        x: curr.x + (prevDx / prevLen) * r,
        y: curr.y + (prevDy / prevLen) * r,
      };
      const cornerEnd = {
        x: curr.x + (nextDx / nextLen) * r,
        y: curr.y + (nextDy / nextLen) * r,
      };
      ctx.lineTo(cornerStart.x, cornerStart.y);
      ctx.quadraticCurveTo(curr.x, curr.y, cornerEnd.x, cornerEnd.y);
    }
    const tail = points[points.length - 1];
    ctx.lineTo(tail.x, tail.y);
  }

  function drawSnakeTail(ctx, tail, bodyWidth) {
    ctx.save();
    ctx.fillStyle = 'rgba(112,214,255,0.72)';
    ctx.beginPath();
    ctx.arc(tail.x, tail.y, bodyWidth * 0.42, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function drawSnakeHead(ctx, head, cell, dir, timeMs = 0) {
    const r = Math.max(6, cell * 0.56);
    const angle = Math.atan2(dir.y, dir.x);
    const bob = Math.sin(timeMs / 130) * cell * 0.025;
    ctx.save();
    ctx.translate(head.x, head.y + bob);
    ctx.rotate(angle);
    ctx.fillStyle = '#ffd166';
    ctx.beginPath();
    ctx.ellipse(0, 0, r * 1.12, r * 0.92, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = 'rgba(255,255,255,0.35)';
    ctx.beginPath();
    ctx.ellipse(r * 0.18, -r * 0.28, r * 0.34, r * 0.20, -0.45, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = '#20242a';
    const eye = Math.max(2, cell * 0.13);
    ctx.beginPath();
    ctx.arc(r * 0.28, -r * 0.32, eye, 0, Math.PI * 2);
    ctx.arc(r * 0.28, r * 0.32, eye, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = 'rgba(32,36,42,0.55)';
    ctx.lineWidth = Math.max(1.5, cell * 0.08);
    ctx.beginPath();
    ctx.moveTo(r * 0.72, 0);
    ctx.lineTo(r * 1.04, -r * 0.14);
    ctx.moveTo(r * 0.72, 0);
    ctx.lineTo(r * 1.04, r * 0.14);
    ctx.stroke();
    ctx.restore();
  }

  function drawFood(ctx, p, cell, kind, timeMs = 0) {
    const c = cellCenter(p, cell);
    const pulse = 1 + Math.sin(timeMs / 180 + p.x * 0.31 + p.y * 0.17) * 0.08;
    const r = Math.max(4, cell * 0.34) * pulse;
    ctx.save();
    ctx.shadowColor = 'rgba(255,209,102,0.45)';
    ctx.shadowBlur = Math.max(4, cell * 0.26);
    ctx.fillStyle = kind === 'fish' ? '#8ecae6' : kind === 'butterfly' ? '#ffafcc' : '#ef476f';
    ctx.beginPath();
    ctx.ellipse(c.x, c.y, r * 1.12, r * 0.78, 0, 0, Math.PI * 2);
    ctx.fill();
    if (kind === 'fish') {
      ctx.beginPath();
      ctx.moveTo(c.x - r * 0.94, c.y);
      ctx.lineTo(c.x - r * 1.45, c.y - r * 0.48);
      ctx.lineTo(c.x - r * 1.45, c.y + r * 0.48);
      ctx.closePath();
      ctx.fill();
    }
    ctx.fillStyle = 'rgba(255,255,255,0.76)';
    ctx.beginPath();
    ctx.arc(c.x + r * 0.38, c.y - r * 0.22, Math.max(1.5, r * 0.16), 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function drawFoodBursts(ctx, bursts, cell) {
    if (!bursts.length) return;
    ctx.save();
    for (const burst of bursts) {
      const alpha = clamp(1 - burst.age / burst.life, 0, 1);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = '#ffd166';
      ctx.beginPath();
      ctx.arc(burst.x * cell, burst.y * cell, Math.max(1.5, cell * 0.12 * alpha), 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.restore();
  }

  function drawBoardFlash(ctx, grid, cell, flashMs, pulseMs, shakeMs) {
    const width = grid.width * cell;
    const height = grid.height * cell;
    const flashAlpha = clamp(flashMs / 220, 0, 1);
    const pulseAlpha = clamp(pulseMs / 420, 0, 1);
    const shake = shakeMs > 0 ? Math.sin(shakeMs * 0.75) * Math.min(4, cell * 0.12) : 0;
    if (!flashAlpha && !pulseAlpha && !shake) return;
    ctx.save();
    ctx.translate(shake, 0);
    if (flashAlpha > 0) {
      ctx.fillStyle = `rgba(184, 242, 230, ${0.12 * flashAlpha})`;
      ctx.fillRect(0, 0, width, height);
      ctx.strokeStyle = `rgba(255, 209, 102, ${0.52 * flashAlpha})`;
      ctx.lineWidth = Math.max(3, cell * 0.12);
      ctx.strokeRect(cell * 0.35, cell * 0.35, width - cell * 0.7, height - cell * 0.7);
    }
    if (pulseAlpha > 0) {
      ctx.fillStyle = `rgba(255, 209, 102, ${0.06 * pulseAlpha})`;
      ctx.beginPath();
      ctx.roundRect(cell * 0.9, cell * 0.9, width - cell * 1.8, height - cell * 1.8, 14);
      ctx.fill();
    }
    ctx.restore();
  }

  function drawAnswerFood(ctx, food, cell, kind, timeMs = 0, pulseMs = 0) {
    drawFood(ctx, food, cell, kind, timeMs);
    const c = cellCenter(food, cell);
    const fontSize = Math.max(10, Math.min(18, cell * 0.62));
    const padX = Math.max(6, cell * 0.22);
    const maxW = Math.max(58, Math.min(150, cell * 7.5));
    ctx.save();
    ctx.font = `800 ${fontSize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    const lines = wrapLines(ctx, food.label, maxW - padX * 2, 2);
    const textW = Math.max(...lines.map((line) => ctx.measureText(line).width), 0);
    ctx.restore();
    const lineHeight = fontSize + 2;
    const h = Math.max(22, lines.length * lineHeight + 10);
    const w = Math.min(maxW, Math.max(cell * 1.8, textW + padX * 2));
    const x = c.x - w / 2;
    const y = c.y + cell * 0.42;
    const pulse = 1 + Math.sin(timeMs / 120 + food.x * 0.41 + food.y * 0.23) * 0.06 + clamp(pulseMs / 380, 0, 1) * 0.12;
    ctx.save();
    ctx.translate(c.x, c.y);
    ctx.scale(pulse, pulse);
    ctx.translate(-c.x, -c.y);
    ctx.fillStyle = food.correct ? 'rgba(8, 28, 24, 0.88)' : 'rgba(20, 24, 30, 0.82)';
    ctx.strokeStyle = food.correct ? 'rgba(184, 242, 230, 0.80)' : 'rgba(255, 255, 255, 0.20)';
    ctx.lineWidth = food.correct ? 2 : 1.5;
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, 8);
    ctx.fill();
    ctx.stroke();
    if (food.correct) {
      ctx.strokeStyle = 'rgba(255, 209, 102, 0.36)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.roundRect(x - 3, y - 3, w + 6, h + 6, 10);
      ctx.stroke();
    }
    ctx.fillStyle = '#f7fbff';
    ctx.font = `800 ${fontSize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    const firstLineY = y + h / 2 - ((lines.length - 1) * lineHeight) / 2 + 1;
    lines.forEach((line, index) => {
      ctx.fillText(line, c.x, firstLineY + index * lineHeight);
    });
    ctx.restore();
  }

  function drawVocabTopPanel(ctx, question, metrics, progress, pulseMs = 0) {
    if (metrics?.vocabLayout?.mode === 'side') {
      drawVocabQuestionSidePanel(ctx, question, metrics.vocabLayout.left, pulseMs);
      return;
    }
    if (!question || !metrics?.vocabLayout?.top) return;
    const panel = metrics.vocabLayout.top;
    const compact = panel.w < 560;
    const pulse = 1 + clamp(pulseMs / 380, 0, 1) * 0.025;
    const pad = compact ? 12 : 18;
    const labelSize = clamp(panel.h * 0.14, 10, 12);
    const termSize = clamp(panel.h * 0.30, 20, compact ? 26 : 30);
    const metaSize = clamp(panel.h * 0.15, 11, 13);
    const exampleSize = clamp(panel.h * 0.17, 11, 14);
    const badgeW = compact ? 94 : 142;
    const textW = panel.w - badgeW - pad * 3;
    const labelY = panel.y + pad;
    const termY = compact
      ? panel.y + panel.h * 0.58
      : panel.y + pad + labelSize + termSize * 0.62;
    const exampleY = panel.y + panel.h - pad;

    ctx.save();
    ctx.translate(panel.x + panel.w / 2, panel.y + panel.h / 2);
    ctx.scale(pulse, pulse);
    ctx.translate(-(panel.x + panel.w / 2), -(panel.y + panel.h / 2));
    drawInfoPanel(ctx, panel, pulseMs > 0 ? 'rgba(184, 242, 230, 0.72)' : 'rgba(255, 209, 102, 0.46)');

    ctx.fillStyle = 'rgba(247, 251, 255, 0.64)';
    ctx.font = `800 ${labelSize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    ctx.fillText('WORD', panel.x + pad, labelY);

    ctx.fillStyle = '#ffd166';
    ctx.font = `900 ${termSize}px "Segoe UI", sans-serif`;
    ctx.textBaseline = 'alphabetic';
    ctx.fillText(fitTextToWidth(ctx, question.term, textW), panel.x + pad, termY);

    if (question.example && !compact) {
      ctx.fillStyle = 'rgba(247, 251, 255, 0.78)';
      ctx.font = `650 ${exampleSize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      ctx.textBaseline = 'bottom';
      ctx.fillText(fitTextToWidth(ctx, question.example, textW), panel.x + pad, exampleY);
    }

    const badgeX = panel.x + panel.w - badgeW - pad;
    const badgeY = panel.y + pad;
    const badgeH = panel.h - pad * 2;
    ctx.fillStyle = 'rgba(8, 12, 16, 0.38)';
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.14)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(badgeX, badgeY, badgeW, badgeH, 8);
    ctx.fill();
    ctx.stroke();

    ctx.fillStyle = '#b8f2e6';
    ctx.font = `900 ${compact ? 17 : 20}px "Segoe UI", sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(`${progress.correct}/${progress.target}`, badgeX + badgeW / 2, badgeY + badgeH * 0.36);

    ctx.fillStyle = 'rgba(247, 251, 255, 0.68)';
    ctx.font = `750 ${compact ? 10 : 12}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    const meta = progress.combo > 1
      ? `score ${progress.score}  x${progress.combo}`
      : `score ${progress.score}${progress.wrong ? `  miss ${progress.wrong}` : ''}`;
    ctx.fillText(fitTextToWidth(ctx, meta, badgeW - 12), badgeX + badgeW / 2, badgeY + badgeH * 0.70);
    ctx.restore();
  }

  function drawVocabBottomPanel(ctx, review, feedback, metrics, timeMs = 0) {
    if (metrics?.vocabLayout?.mode === 'side') {
      drawVocabStatusSidePanel(ctx, review, feedback, metrics.vocabLayout.right, metrics, timeMs);
      return;
    }
    if (!metrics?.vocabLayout?.bottom) return;
    const panel = metrics.vocabLayout.bottom;
    const compact = panel.w < 560;
    const active = feedback || review;
    const accent = feedback
      ? (feedback.correct ? '#b8f2e6' : '#ffafcc')
      : (review?.correct ? '#b8f2e6' : '#ffd166');
    drawInfoPanel(ctx, panel, active ? colorToStroke(accent, 0.58) : 'rgba(255, 255, 255, 0.16)');
    const pad = compact ? 12 : 16;

    ctx.save();
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    if (feedback) {
      const progress = clamp(feedback.age / feedback.life, 0, 1);
      const alpha = progress < 0.12 ? progress / 0.12 : clamp((1 - progress) / 0.34, 0, 1);
      ctx.globalAlpha = Math.max(0.18, alpha);
      ctx.fillStyle = accent;
      ctx.font = `900 ${compact ? 17 : 21}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      ctx.fillText(feedback.label, panel.x + pad, panel.y + panel.h * 0.38);
      ctx.fillStyle = 'rgba(247, 251, 255, 0.88)';
      ctx.font = `750 ${compact ? 12 : 14}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      const text = feedback.correct
        ? `${feedback.term} = ${feedback.meaning}${feedback.delta ? `  ${feedback.delta}` : ''}`
        : (feedback.explanation || `${feedback.term} = ${feedback.meaning}；刚才选了：${feedback.picked}`);
      drawPanelLines(ctx, text, panel.x + pad, panel.y + panel.h * 0.70, panel.w - pad * 2, compact ? 15 : 17, feedback.correct ? 1 : 2);
      ctx.restore();
      return;
    }

    if (review) {
      const title = review.correct ? '上一题 · 正确' : '上一题 · 再记一次';
      ctx.fillStyle = accent;
      ctx.font = `850 ${compact ? 13 : 15}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      ctx.fillText(title, panel.x + pad, panel.y + panel.h * 0.32);
      ctx.fillStyle = '#f7fbff';
      ctx.font = `760 ${compact ? 12 : 14}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      const picked = !review.correct && review.picked ? `  刚才选了：${review.picked}` : '';
      const text = !review.correct && review.explanation
        ? review.explanation
        : `${review.term} = ${review.meaning}${picked}`;
      drawPanelLines(ctx, text, panel.x + pad, panel.y + panel.h * 0.66, panel.w - pad * 2, compact ? 15 : 17, !review.correct ? 2 : 1);
      ctx.restore();
      return;
    }

    ctx.fillStyle = 'rgba(247, 251, 255, 0.62)';
    ctx.font = `750 ${compact ? 12 : 14}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.fillText('吃掉正确释义，错误选项不会增长。', panel.x + pad, panel.y + panel.h / 2);
    ctx.restore();
  }

  function drawVocabQuestionSidePanel(ctx, question, panel, pulseMs = 0) {
    if (!question || !panel) return;
    const pulse = 1 + clamp(pulseMs / 380, 0, 1) * 0.018;
    const pad = clamp(panel.w * 0.10, 14, 22);
    const labelSize = clamp(panel.w * 0.055, 10, 12);
    const termSize = clamp(panel.w * 0.145, 24, 34);
    const bodySize = clamp(panel.w * 0.066, 12, 15);
    const textW = panel.w - pad * 2;

    ctx.save();
    ctx.translate(panel.x + panel.w / 2, panel.y + panel.h / 2);
    ctx.scale(pulse, pulse);
    ctx.translate(-(panel.x + panel.w / 2), -(panel.y + panel.h / 2));
    drawInfoPanel(ctx, panel, pulseMs > 0 ? 'rgba(184, 242, 230, 0.58)' : 'rgba(255, 209, 102, 0.32)');

    ctx.fillStyle = 'rgba(247, 251, 255, 0.62)';
    ctx.font = `800 ${labelSize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    ctx.fillText('WORD', panel.x + pad, panel.y + pad);

    ctx.fillStyle = '#ffd166';
    ctx.font = `900 ${termSize}px "Segoe UI", sans-serif`;
    ctx.textBaseline = 'alphabetic';
    drawPanelLines(ctx, question.term, panel.x + pad, panel.y + pad + labelSize + termSize * 0.95, textW, termSize * 1.08, 2);

    const ruleY = panel.y + panel.h * 0.38;
    ctx.fillStyle = 'rgba(247, 251, 255, 0.82)';
    ctx.font = `760 ${bodySize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    drawPanelLines(ctx, '吃掉正确中文释义，避开干扰项。', panel.x + pad, ruleY, textW, bodySize * 1.42, 3);

    if (question.example) {
      ctx.fillStyle = 'rgba(247, 251, 255, 0.62)';
      ctx.font = `650 ${Math.max(11, bodySize - 1)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      drawPanelLines(ctx, question.example, panel.x + pad, panel.y + panel.h * 0.68, textW, bodySize * 1.35, 5);
    }
    ctx.restore();
  }

  function drawVocabStatusSidePanel(ctx, review, feedback, panel, metrics, timeMs = 0) {
    if (!panel) return;
    const progress = currentVocabProgress(metrics);
    const active = feedback || review;
    const accent = feedback
      ? (feedback.correct ? '#b8f2e6' : '#ffafcc')
      : (review?.correct ? '#b8f2e6' : '#ffd166');
    const pad = clamp(panel.w * 0.10, 14, 22);
    const textW = panel.w - pad * 2;
    const labelSize = clamp(panel.w * 0.055, 10, 12);
    const numberSize = clamp(panel.w * 0.16, 26, 36);
    const bodySize = clamp(panel.w * 0.062, 12, 15);

    drawInfoPanel(ctx, panel, active ? colorToStroke(accent, 0.46) : 'rgba(184, 242, 230, 0.24)');
    ctx.save();
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';

    ctx.fillStyle = 'rgba(247, 251, 255, 0.58)';
    ctx.font = `800 ${labelSize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.fillText('PROGRESS', panel.x + pad, panel.y + pad);

    const scoreY = panel.y + pad + labelSize + 6;
    ctx.fillStyle = '#b8f2e6';
    ctx.font = `900 ${numberSize}px "Segoe UI", sans-serif`;
    ctx.fillText(`${progress.correct}/${progress.target}`, panel.x + pad, scoreY);

    ctx.fillStyle = 'rgba(247, 251, 255, 0.70)';
    ctx.font = `750 ${bodySize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    const meta = progress.combo > 1
      ? `score ${progress.score}  x${progress.combo}`
      : `score ${progress.score}${progress.wrong ? `  miss ${progress.wrong}` : ''}`;
    ctx.fillText(meta, panel.x + pad, scoreY + numberSize + 8);

    drawSideDivider(ctx, panel.x + pad, panel.y + panel.h * 0.34, textW);

    if (feedback) {
      const progressRatio = clamp(feedback.age / feedback.life, 0, 1);
      const alpha = progressRatio < 0.12 ? progressRatio / 0.12 : clamp((1 - progressRatio) / 0.34, 0, 1);
      ctx.globalAlpha = Math.max(0.24, alpha);
      ctx.fillStyle = accent;
      ctx.font = `900 ${clamp(panel.w * 0.084, 16, 21)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      ctx.fillText(feedback.label, panel.x + pad, panel.y + panel.h * 0.39);
      ctx.fillStyle = 'rgba(247, 251, 255, 0.84)';
      ctx.font = `720 ${bodySize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      const text = feedback.correct
        ? `${feedback.term} = ${feedback.meaning}${feedback.delta ? `  ${feedback.delta}` : ''}`
        : (feedback.explanation || `${feedback.term} = ${feedback.meaning}；刚才选了：${feedback.picked}`);
      drawPanelLines(ctx, text, panel.x + pad, panel.y + panel.h * 0.47, textW, bodySize * 1.42, 8);
      ctx.restore();
      return;
    }

    if (review) {
      ctx.fillStyle = accent;
      ctx.font = `850 ${clamp(panel.w * 0.066, 13, 16)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      ctx.fillText(review.correct ? '上一题 · 正确' : '上一题 · 再记一次', panel.x + pad, panel.y + panel.h * 0.39);
      ctx.fillStyle = '#f7fbff';
      ctx.font = `780 ${bodySize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      const picked = !review.correct && review.picked ? `  刚才选了：${review.picked}` : '';
      const text = !review.correct && review.explanation
        ? review.explanation
        : `${review.term} = ${review.meaning}${picked}`;
      drawPanelLines(ctx, text, panel.x + pad, panel.y + panel.h * 0.47, textW, bodySize * 1.42, 8);
      ctx.restore();
      return;
    }

    ctx.fillStyle = 'rgba(247, 251, 255, 0.62)';
    ctx.font = `720 ${bodySize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    drawPanelLines(ctx, '上一题反馈会显示在这里。', panel.x + pad, panel.y + panel.h * 0.42, textW, bodySize * 1.42, 3);
    ctx.restore();
  }

  function currentVocabProgress(metrics) {
    return metrics?.vocabProgress || { score: 0, correct: 0, target: 0, combo: 0, wrong: 0 };
  }

  function drawSideDivider(ctx, x, y, w) {
    ctx.save();
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.12)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x + w, y);
    ctx.stroke();
    ctx.restore();
  }

  function drawInfoPanel(ctx, panel, strokeStyle) {
    ctx.save();
    ctx.fillStyle = 'rgba(8, 12, 17, 0.74)';
    ctx.strokeStyle = strokeStyle;
    ctx.lineWidth = 1.25;
    ctx.beginPath();
    ctx.roundRect(panel.x, panel.y, panel.w, panel.h, 7);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  }

  function colorToStroke(hex, alpha) {
    if (hex === '#b8f2e6') return `rgba(184, 242, 230, ${alpha})`;
    if (hex === '#ffafcc') return `rgba(255, 175, 204, ${alpha})`;
    if (hex === '#ffd166') return `rgba(255, 209, 102, ${alpha})`;
    return `rgba(255, 255, 255, ${alpha})`;
  }

  function drawQuestionPanel(ctx, question, grid, cell, pulseMs = 0, combo = 0) {
    if (!question) return;
    const width = grid.width * cell;
    const x = cell * 0.8;
    const y = cell * 0.8;
    const w = Math.min(width - cell * 1.6, Math.max(310, Math.min(cell * 14, 430)));
    const h = Math.max(52, cell * 2.6);
    const pulse = 1 + clamp(pulseMs / 380, 0, 1) * 0.05;
    ctx.save();
    ctx.translate(x + w / 2, y + h / 2);
    ctx.scale(pulse, pulse);
    ctx.translate(-(x + w / 2), -(y + h / 2));
    ctx.fillStyle = 'rgba(9, 13, 18, 0.78)';
    ctx.strokeStyle = pulseMs > 0 ? 'rgba(184, 242, 230, 0.78)' : 'rgba(255, 209, 102, 0.52)';
    ctx.lineWidth = pulseMs > 0 ? 2.5 : 2;
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, 10);
    ctx.fill();
    ctx.stroke();
    if (combo > 1) {
      ctx.fillStyle = 'rgba(255, 209, 102, 0.14)';
      ctx.beginPath();
      ctx.roundRect(x + 4, y + 4, w - 8, h - 8, 8);
      ctx.fill();
    }
    ctx.fillStyle = '#ffd166';
    ctx.font = `900 ${Math.max(20, cell * 1.15)}px "Segoe UI", sans-serif`;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText(question.term, x + cell * 0.75, y + h * 0.42);
    if (question.example) {
      ctx.fillStyle = 'rgba(247, 251, 255, 0.82)';
      ctx.font = `600 ${Math.max(11, cell * 0.54)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      ctx.fillText(fitText(question.example, 44), x + cell * 0.75, y + h * 0.78);
    }
    ctx.restore();
  }

  function drawReviewPanel(ctx, review, metrics, timeMs = 0) {
    if (!review || !metrics) return;
    const cell = metrics.cell;
    const boardX = Number(metrics.x) || 0;
    const boardY = Number(metrics.y) || 0;
    const leftSpace = boardX - 12;
    if (leftSpace < 170) return;
    const w = clamp(leftSpace - 20, 176, 260);
    const h = Math.max(118, cell * 4.6);
    const x = Math.max(12, boardX - w - 16);
    const y = Math.max(68, boardY + cell * 1.1);
    const accent = review.correct ? '#b8f2e6' : '#ffafcc';
    const glow = 0.18 + Math.sin(timeMs / 240) * 0.04;
    ctx.save();
    ctx.fillStyle = 'rgba(9, 13, 18, 0.68)';
    ctx.strokeStyle = review.correct ? `rgba(184, 242, 230, ${0.48 + glow})` : `rgba(255, 175, 204, ${0.48 + glow})`;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, 10);
    ctx.fill();
    ctx.stroke();

    ctx.fillStyle = `rgba(${review.correct ? '184, 242, 230' : '255, 175, 204'}, 0.10)`;
    ctx.beginPath();
    ctx.roundRect(x + 5, y + 5, w - 10, Math.min(h - 10, cell * 1.6), 8);
    ctx.fill();

    ctx.fillStyle = accent;
    ctx.font = `800 ${Math.max(10, cell * 0.48)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    ctx.fillText(review.correct ? '上一题 · 正确' : '上一题 · 再记', x + cell * 0.55, y + cell * 0.42);

    ctx.fillStyle = '#ffd166';
    ctx.font = `900 ${Math.max(17, cell * 0.86)}px "Segoe UI", sans-serif`;
    ctx.fillText(fitText(review.term, 15), x + cell * 0.55, y + cell * 1.30);

    ctx.fillStyle = '#f7fbff';
    ctx.font = `800 ${Math.max(13, cell * 0.60)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.fillText(fitText(review.meaning, 14), x + cell * 0.55, y + cell * 2.24);

    if (!review.correct && review.picked) {
      ctx.fillStyle = 'rgba(255, 175, 204, 0.86)';
      ctx.font = `700 ${Math.max(10, cell * 0.45)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      ctx.fillText(`刚才选了：${fitText(review.picked, 10)}`, x + cell * 0.55, y + cell * 2.86);
    }

    if (review.example) {
      ctx.fillStyle = 'rgba(247, 251, 255, 0.72)';
      ctx.font = `650 ${Math.max(10, cell * 0.46)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      const example = fitTextToWidth(ctx, review.example, w - cell * 1.1);
      ctx.fillText(example, x + cell * 0.55, y + cell * 3.48);
    }
    ctx.restore();
  }

  function drawHudFeedback(ctx, feedback, grid, cell) {
    if (!feedback) return;
    const width = grid.width * cell;
    const progress = clamp(feedback.age / feedback.life, 0, 1);
    const alpha = progress < 0.12 ? progress / 0.12 : clamp((1 - progress) / 0.28, 0, 1);
    if (alpha <= 0) return;
    const panelY = cell * 0.8;
    const w = Math.min(Math.max(cell * 7.4, 220), Math.max(220, width * 0.28));
    const h = Math.max(54, cell * 2.45);
    const x = Math.max(cell * 0.8, width - w - cell * 0.8);
    const y = panelY + cell * 0.18;
    const boxW = w;
    const accent = feedback.correct ? '#b8f2e6' : '#ffafcc';
    const dy = (1 - easeOutCubic(clamp(progress / 0.22, 0, 1))) * cell * 0.35;

    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.translate(0, dy);
    ctx.shadowColor = feedback.correct ? 'rgba(184, 242, 230, 0.22)' : 'rgba(255, 175, 204, 0.24)';
    ctx.shadowBlur = Math.max(8, cell * 0.42);
    ctx.fillStyle = feedback.correct ? 'rgba(8, 28, 24, 0.82)' : 'rgba(36, 13, 24, 0.82)';
    ctx.strokeStyle = feedback.correct ? 'rgba(184, 242, 230, 0.86)' : 'rgba(255, 175, 204, 0.86)';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.roundRect(x, y, boxW, h, 10);
    ctx.fill();
    ctx.stroke();

    ctx.fillStyle = accent;
    ctx.font = `900 ${Math.max(17, cell * 0.78)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText(feedback.label, x + cell * 0.6, y + h * 0.36);

    ctx.fillStyle = feedback.correct ? '#ffd166' : '#ffafcc';
    ctx.font = `900 ${Math.max(15, cell * 0.68)}px "Segoe UI", sans-serif`;
    ctx.textAlign = 'right';
    ctx.fillText(feedback.delta || '', x + boxW - cell * 0.62, y + h * 0.36);

    ctx.fillStyle = 'rgba(247, 251, 255, 0.88)';
    ctx.font = `750 ${Math.max(11, cell * 0.50)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.textAlign = 'left';
    ctx.fillText(`${fitText(feedback.term, 13)} = ${fitText(feedback.meaning, 12)}`, x + cell * 0.6, y + h * 0.73);
    ctx.restore();
  }

  function drawEffects(ctx, effects, cell) {
    if (!effects.length) return;
    ctx.save();
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (const effect of effects) {
      const alpha = clamp(1 - effect.age / 760, 0, 1);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = effect.color;
      const scale = 1 + (effect.scale || 1) * 0.2 * alpha;
      const x = effect.x * cell + cell / 2;
      const y = effect.y * cell + cell / 2 - effect.age * 0.025;
      ctx.save();
      ctx.translate(x, y);
      ctx.scale(scale, scale);
      ctx.font = `900 ${Math.max(14, cell * 0.62)}px "Segoe UI", "Microsoft YaHei", sans-serif`;
      ctx.shadowColor = 'rgba(0, 0, 0, 0.38)';
      ctx.shadowBlur = Math.max(2, cell * 0.18);
      if (effect.stroke) {
        ctx.lineWidth = Math.max(2, cell * 0.08);
        ctx.strokeStyle = effect.stroke;
        ctx.strokeText(effect.text, 0, 0);
      }
      ctx.fillText(effect.text, 0, 0);
      ctx.restore();
    }
    ctx.restore();
  }

  function normalizeVocab(raw) {
    if (!raw || !Array.isArray(raw.entries) || raw.entries.length < 2) return null;
    const entries = raw.entries
      .map((entry) => ({
        id: String(entry.id || entry.term || '').trim(),
        term: String(entry.term || '').trim(),
        meaning: String(entry.meaning || '').trim(),
        distractors: Array.isArray(entry.distractors)
          ? entry.distractors.map((item) => String(item || '').trim()).filter(Boolean)
          : [],
        example: String(entry.example || '').trim(),
        hint: String(entry.hint || '').trim(),
        explanation: String(entry.explanation || '').trim(),
      }))
      .filter((entry) => entry.id && entry.term && entry.meaning);
    if (entries.length < 2) return null;
    return {
      mode: raw.mode || 'meaning_choice',
      topic: String(raw.topic || '').trim(),
      level: String(raw.level || '').trim(),
      answerCount: clamp(Number(raw.answer_count) || 4, 2, 6),
      targetCorrect: clamp(Number(raw.target_correct) || Math.min(12, entries.length), 1, 50),
      entries,
    };
  }

  function buildAnswerChoices(entry, entries, answerCount, rng) {
    const labels = [entry.meaning];
    for (const item of entry.distractors || []) {
      if (!labels.includes(item)) labels.push(item);
      if (labels.length >= answerCount) break;
    }
    const shuffledEntries = entries.filter((item) => item.id !== entry.id);
    shuffle(shuffledEntries, rng);
    for (const item of shuffledEntries) {
      if (!labels.includes(item.meaning)) labels.push(item.meaning);
      if (labels.length >= answerCount) break;
    }
    const choices = labels.slice(0, answerCount).map((label) => ({
      label,
      correct: label === entry.meaning,
    }));
    shuffle(choices, rng);
    return choices;
  }

  function shuffle(items, rng) {
    for (let i = items.length - 1; i > 0; i--) {
      const j = Math.floor(rng() * (i + 1));
      [items[i], items[j]] = [items[j], items[i]];
    }
    return items;
  }

  function distanceToSnakeHead(head, point) {
    if (!head) return 99;
    return Math.abs(head.x - point.x) + Math.abs(head.y - point.y);
  }

  function isVocabHudCell(point, grid) {
    const leftLimit = Math.ceil(grid.width * 0.42);
    const rightStart = Math.floor(grid.width * 0.68);
    const topBandLimit = Math.ceil(grid.height * 0.23);
    return (
      point.y <= topBandLimit
      && (point.x <= leftLimit || point.x >= rightStart)
    );
  }

  function fitText(text, maxChars) {
    const value = String(text || '');
    if (value.length <= maxChars) return value;
    return `${value.slice(0, Math.max(1, maxChars - 1))}…`;
  }

  function fitTextToWidth(ctx, text, maxWidth) {
    const value = String(text || '');
    if (ctx.measureText(value).width <= maxWidth) return value;
    let lo = 0;
    let hi = value.length;
    while (lo < hi) {
      const mid = Math.ceil((lo + hi) / 2);
      if (ctx.measureText(`${value.slice(0, mid)}…`).width <= maxWidth) lo = mid;
      else hi = mid - 1;
    }
    return `${value.slice(0, Math.max(1, lo))}…`;
  }

  function wrapLines(ctx, text, maxWidth, maxLines) {
    const value = String(text || '').trim();
    if (!value) return [''];
    const words = value.includes(' ') ? value.split(/\s+/) : Array.from(value);
    const joiner = value.includes(' ') ? ' ' : '';
    const lines = [];
    let current = '';
    for (const word of words) {
      const candidate = current ? `${current}${joiner}${word}` : word;
      if (ctx.measureText(candidate).width <= maxWidth || !current) {
        current = candidate;
      } else {
        lines.push(current);
        current = word;
      }
      if (lines.length >= maxLines) break;
    }
    if (current && lines.length < maxLines) lines.push(current);
    const consumed = lines.join(joiner).replace(/\s/g, '');
    const source = value.replace(/\s/g, '');
    if (source.length > consumed.length && lines.length > 0) {
      lines[lines.length - 1] = fitTextToWidth(ctx, `${lines[lines.length - 1]}…`, maxWidth);
    }
    return lines.slice(0, maxLines);
  }

  function drawPanelLines(ctx, text, x, y, maxWidth, lineHeight, maxLines) {
    const lines = wrapLines(ctx, text, maxWidth, maxLines);
    const top = y - ((lines.length - 1) * lineHeight) / 2;
    lines.forEach((line, index) => {
      ctx.fillText(line, x, top + index * lineHeight);
    });
  }

  function wrapText(ctx, text, x, y, maxWidth, lineHeight, maxLines) {
    const value = String(text || '').trim();
    if (!value) return;
    const words = value.includes(' ') ? value.split(/\s+/) : Array.from(value);
    const lines = [];
    let current = '';
    for (const word of words) {
      const candidate = current ? `${current}${value.includes(' ') ? ' ' : ''}${word}` : word;
      if (ctx.measureText(candidate).width <= maxWidth || !current) {
        current = candidate;
      } else {
        lines.push(current);
        current = word;
      }
      if (lines.length >= maxLines) break;
    }
    if (current && lines.length < maxLines) lines.push(current);
    lines.slice(0, maxLines).forEach((line, index) => {
      const suffix = index === maxLines - 1 && words.join(value.includes(' ') ? ' ' : '').length > lines.join('').length ? '…' : '';
      ctx.fillText(`${line}${suffix}`, x, y + index * lineHeight);
    });
  }

  window.BitCatGames.snake = (config, host) => new SnakeEngine(config, host);
  window.BitCatGames.SnakeEngine = SnakeEngine;
})();
