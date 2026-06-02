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

  function keyOf(p) {
    return `${p.x},${p.y}`;
  }

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
      this.nextDir = { x: 1, y: 0 };
      this.stepMs = Number(config.player.speed_ms) || 140;
      this.tickMs = 0;
      this.ended = false;
      this.boostHeld = false;
      this.snake = [];
      this.prevSnake = [];
      this.food = { x: 0, y: 0 };
      this.answerFoods = [];
      this.question = null;
      this.questionIndex = 0;
      this.correctCount = 0;
      this.wrongCount = 0;
      this.combo = 0;
      this.maxCombo = 0;
      this.missed = [];
      this.foodEaten = 0;
      this.animMs = 0;
      this.foodBursts = [];
      this.effects = [];
      this.reset();
    }

    reset() {
      const grid = this.config.grid;
      const len = clamp(Number(this.config.player.initial_length) || 3, 1, 10);
      const startX = Math.floor(grid.width / 2);
      const startY = Math.floor(grid.height / 2);
      this.snake = [];
      for (let i = 0; i < len; i++) {
        this.snake.push({ x: startX - i, y: startY });
      }
      this.prevSnake = this.snake.map((p) => ({ ...p }));
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

    hudText() {
      return `${this.snake.length}${this.boostHeld ? ' BOOST' : ''}`;
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
        if (next.x === -this.dir.x && next.y === -this.dir.y) return;
        this.nextDir = next;
      }
    }

    update(dtMs) {
      this.animMs += dtMs;
      this.updateFoodBursts(dtMs);
      this.updateEffects(dtMs);
      if (this.state !== 'playing' || this.ended) return;
      this.tickMs += dtMs;
      while (this.tickMs >= this.effectiveStepMs() && this.state === 'playing') {
        this.tickMs -= this.effectiveStepMs();
        this.step();
      }
    }

    step() {
      this.prevSnake = this.snake.map((p) => ({ ...p }));
      this.dir = this.nextDir;
      const head = this.snake[0];
      let next = { x: head.x + this.dir.x, y: head.y + this.dir.y };
      const grid = this.config.grid;
      const rules = this.config.rules;

      if (!rules.walls_kill) {
        next = {
          x: (next.x + grid.width) % grid.width,
          y: (next.y + grid.height) % grid.height,
        };
      } else if (next.x < 0 || next.y < 0 || next.x >= grid.width || next.y >= grid.height) {
        this.finish('lose');
        return;
      }

      const eatenAnswer = this.vocab ? this.answerFoods.find((food) => samePoint(next, food)) : null;
      const ate = this.vocab ? Boolean(eatenAnswer) : samePoint(next, this.food);
      const body = ate ? this.snake : this.snake.slice(0, -1);
      if (rules.self_kill && body.some((p) => samePoint(p, next))) {
        this.finish('lose');
        return;
      }

      this.snake.unshift(next);
      if (ate) {
        if (this.vocab) {
          const shouldGrow = this.consumeAnswer(eatenAnswer);
          if (!shouldGrow) this.snake.pop();
        } else {
          this.score += this.boostHeld ? 14 : 10;
          this.foodEaten += 1;
          this.addFoodBurst(next);
          this.stepMs = Math.max(40, Math.floor(this.stepMs * rules.speed_ramp));
          if (this.snake.length >= rules.win_length) {
            this.finish('win');
            return;
          }
          this.spawnFood();
        }
      } else {
        this.snake.pop();
      }
    }

    effectiveStepMs() {
      return this.boostHeld ? Math.max(28, Math.floor(this.stepMs * 0.55)) : this.stepMs;
    }

    spawnFood() {
      const grid = this.config.grid;
      const occupied = new Set(this.snake.map(keyOf));
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
      const occupied = new Set(this.snake.map(keyOf));
      const free = [];
      for (let y = 0; y < grid.height; y++) {
        for (let x = 0; x < grid.width; x++) {
          const point = { x, y };
          if (occupied.has(keyOf(point))) continue;
          if (distanceToSnakeHead(this.snake[0], point) < 4) continue;
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
        this.score += this.boostHeld ? 18 : 14;
        this.correctCount += 1;
        this.combo += 1;
        this.maxCombo = Math.max(this.maxCombo, this.combo);
        this.stepMs = Math.max(48, Math.floor(this.stepMs * this.config.rules.speed_ramp));
        this.addEffect('GOOD', food, '#b8f2e6');
        if (this.correctCount >= this.vocab.targetCorrect) {
          this.finish('win');
          return true;
        }
        this.nextQuestion();
        return true;
      } else {
        this.score = Math.max(0, this.score - 4);
        this.wrongCount += 1;
        this.combo = 0;
        if (this.question) {
          this.missed.push({
            id: this.question.id,
            term: this.question.term,
            meaning: this.question.meaning,
            picked: food.label,
          });
        }
        this.addEffect('MISS', food, '#ffafcc');
        this.nextQuestion();
        return false;
      }
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

    addEffect(text, point, color) {
      this.effects.push({ text, x: point.x, y: point.y, color, age: 0 });
    }

    updateEffects(dtMs) {
      for (const effect of this.effects) effect.age += dtMs;
      this.effects = this.effects.filter((effect) => effect.age < 760);
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
      drawSnakeField(ctx, grid, cell);
      if (this.vocab) {
        for (const food of this.answerFoods) drawAnswerFood(ctx, food, cell, this.config.theme.food, this.animMs);
        drawQuestionPanel(ctx, this.question, grid, cell);
      } else {
        drawFood(ctx, this.food, cell, this.config.theme.food, this.animMs);
      }
      drawFoodBursts(ctx, this.foodBursts, cell);
      drawEffects(ctx, this.effects, cell);
      drawSnake(ctx, this.renderSnake(), cell, this.dir, this.config.theme, this.animMs);
      if (this.boostHeld) drawBoostBadge(ctx, grid.width * cell, cell);
      ctx.restore();
    }

    renderSnake() {
      if (this.state !== 'playing' || this.ended || this.prevSnake.length === 0) {
        return this.snake;
      }
      const progress = clamp(this.tickMs / this.effectiveStepMs(), 0, 1);
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
  }

  function fillBoardPanel(ctx, width, height) {
    const gradient = ctx.createLinearGradient(0, 0, width, height);
    gradient.addColorStop(0, 'rgba(10,16,22,0.46)');
    gradient.addColorStop(0.52, 'rgba(16,24,32,0.30)');
    gradient.addColorStop(1, 'rgba(8,12,18,0.48)');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);
  }

  function drawSnakeField(ctx, grid, cell) {
    const width = grid.width * cell;
    const height = grid.height * cell;
    ctx.save();
    ctx.strokeStyle = 'rgba(8,12,16,0.58)';
    ctx.lineWidth = 6;
    ctx.strokeRect(3, 3, width - 6, height - 6);
    ctx.strokeStyle = 'rgba(255,255,255,0.42)';
    ctx.lineWidth = 2;
    ctx.strokeRect(1, 1, width - 2, height - 2);
    ctx.strokeStyle = 'rgba(112,214,255,0.36)';
    ctx.lineWidth = 1;
    ctx.strokeRect(7.5, 7.5, width - 15, height - 15);
    if (cell >= 14) {
      ctx.fillStyle = 'rgba(255,255,255,0.065)';
      const dot = Math.max(1, cell * 0.055);
      for (let y = 1; y < grid.height; y += 2) {
        for (let x = 1; x < grid.width; x += 2) {
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

  function drawSnake(ctx, snake, cell, dir, theme, timeMs = 0) {
    if (!snake.length) return;
    const bodyWidth = Math.max(8, cell * 0.72);
    const points = snake.map((p) => cellCenter(p, cell));
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

  function drawAnswerFood(ctx, food, cell, kind, timeMs = 0) {
    drawFood(ctx, food, cell, kind, timeMs);
    const c = cellCenter(food, cell);
    const text = fitText(food.label, 8);
    const fontSize = Math.max(11, Math.min(18, cell * 0.62));
    const padX = Math.max(6, cell * 0.22);
    const h = Math.max(22, fontSize + 10);
    const w = Math.max(cell * 1.8, text.length * fontSize * 0.9 + padX * 2);
    const x = c.x - w / 2;
    const y = c.y + cell * 0.42;
    ctx.save();
    ctx.fillStyle = food.correct ? 'rgba(8, 28, 24, 0.82)' : 'rgba(20, 24, 30, 0.82)';
    ctx.strokeStyle = food.correct ? 'rgba(184, 242, 230, 0.58)' : 'rgba(255, 255, 255, 0.20)';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, 8);
    ctx.fill();
    ctx.stroke();
    ctx.fillStyle = '#f7fbff';
    ctx.font = `800 ${fontSize}px "Segoe UI", "Microsoft YaHei", sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(text, c.x, y + h / 2 + 1);
    ctx.restore();
  }

  function drawQuestionPanel(ctx, question, grid, cell) {
    if (!question) return;
    const width = grid.width * cell;
    const x = cell * 0.8;
    const y = cell * 0.8;
    const w = Math.min(width - cell * 1.6, Math.max(cell * 10, 310));
    const h = Math.max(52, cell * 2.6);
    ctx.save();
    ctx.fillStyle = 'rgba(9, 13, 18, 0.78)';
    ctx.strokeStyle = 'rgba(255, 209, 102, 0.52)';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, 10);
    ctx.fill();
    ctx.stroke();
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

  function drawEffects(ctx, effects, cell) {
    if (!effects.length) return;
    ctx.save();
    ctx.font = `900 ${Math.max(14, cell * 0.62)}px "Segoe UI", sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (const effect of effects) {
      const alpha = clamp(1 - effect.age / 760, 0, 1);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = effect.color;
      ctx.fillText(effect.text, effect.x * cell + cell / 2, effect.y * cell + cell / 2 - effect.age * 0.025);
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
      }))
      .filter((entry) => entry.id && entry.term && entry.meaning);
    if (entries.length < 2) return null;
    return {
      mode: raw.mode || 'meaning_choice',
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

  function fitText(text, maxChars) {
    const value = String(text || '');
    if (value.length <= maxChars) return value;
    return `${value.slice(0, Math.max(1, maxChars - 1))}…`;
  }

  window.BitCatGames.snake = (config, host) => new SnakeEngine(config, host);
  window.BitCatGames.SnakeEngine = SnakeEngine;
})();
