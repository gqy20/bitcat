(function () {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;

  const canvas = document.getElementById('gameCanvas');
  const ctx = canvas.getContext('2d');
  const titleEl = document.getElementById('title');
  const scoreEl = document.getElementById('score');
  const lengthEl = document.getElementById('length');
  const overlay = document.getElementById('overlay');
  const overlayTitle = document.getElementById('overlayTitle');
  const overlayText = document.getElementById('overlayText');
  const overlayActions = document.getElementById('overlayActions');
  const restartBtn = document.getElementById('restartBtn');
  const quitBtn = document.getElementById('quitBtn');

  let engine = null;
  let currentConfig = null;
  let lastTime = performance.now();
  let reported = false;
  let closing = false;
  let lastLoggedState = null;

  function log(msg) {
    if (invoke) invoke('cmd_game_log', { msg }).catch(() => {});
    console.log('[game]', msg);
  }

  function emitBattlePet(kind, detail = {}) {
    if (!invoke) return;
    const event = {
      kind,
      source: detail.source || null,
      skill_id: detail.skillId || null,
      damage: Number.isFinite(detail.damage) ? detail.damage : null,
      hp_ratio: Number.isFinite(detail.hpRatio) ? detail.hpRatio : null,
      interrupted: Boolean(detail.interrupted),
    };
    invoke('cmd_battle_pet_event', { event }).catch(() => {});
  }

  function setGameInputCapture(enabled) {
    if (!invoke) return Promise.resolve();
    return invoke('cmd_game_set_input_capture', { enabled }).catch((e) => {
      log(`cmd_game_set_input_capture failed: ${e}`);
    });
  }

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

  function createRng(seed) {
    let s = seed >>> 0;
    return function rng() {
      s = (s * 1664525 + 1013904223) >>> 0;
      return s / 4294967296;
    };
  }

  class SnakeEngine {
    constructor(config, rng) {
      this.config = config;
      this.rng = rng || Math.random;
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
      this.foodEaten = 0;
      this.animMs = 0;
      this.foodBursts = [];
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
      this.spawnFood();
    }

    getState() {
      return this.state;
    }

    handleInput(input) {
      if (!input) return;
      log(`input type=${input.type || ''} dx=${input.dx ?? ''} dy=${input.dy ?? ''} state=${this.state}`);
      if (this.ended) {
        if (input.type === 'confirm') restartGame();
        else if (input.type === 'cancel') closeEndedGame(this.state);
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

      const ate = samePoint(next, this.food);
      const body = ate ? this.snake : this.snake.slice(0, -1);
      if (rules.self_kill && body.some((p) => samePoint(p, next))) {
        this.finish('lose');
        return;
      }

      this.snake.unshift(next);
      if (ate) {
        this.score += 10;
        this.foodEaten += 1;
        this.addFoodBurst(next);
        this.stepMs = Math.max(40, Math.floor(this.stepMs * rules.speed_ramp));
        if (this.snake.length >= rules.win_length) {
          this.finish('win');
          return;
        }
        this.spawnFood();
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

      drawFood(ctx, this.food, cell, this.config.theme.food, this.animMs);
      drawFoodBursts(ctx, this.foodBursts, cell);
      drawSnake(ctx, this.renderSnake(), cell, this.dir, this.config.theme, this.animMs);
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

  class MemoryEngine {
    constructor(config, rng) {
      this.config = config;
      this.rng = rng || Math.random;
      this.state = 'ready';
      this.score = 0;
      this.ended = false;
      this.cursor = 0;
      this.selected = [];
      this.matched = new Set();
      this.lockMs = 0;
      this.cards = [];
      this.reset();
    }

    reset() {
      const count = this.config.grid.width * this.config.grid.height;
      const values = [];
      for (let i = 0; i < count / 2; i++) values.push(i, i);
      for (let i = values.length - 1; i > 0; i--) {
        const j = Math.floor(this.rng() * (i + 1));
        [values[i], values[j]] = [values[j], values[i]];
      }
      this.cards = values;
    }

    getState() {
      return this.state;
    }

    handleInput(input) {
      if (!input) return;
      if (this.ended) {
        if (input.type === 'confirm') restartGame();
        else if (input.type === 'cancel') closeEndedGame(this.state);
        return;
      }
      if (input.type === 'confirm') {
        if (this.state === 'ready') this.state = 'playing';
        this.flipCursor();
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
        if (this.state === 'ready') this.state = 'playing';
        const w = this.config.grid.width;
        const h = this.config.grid.height;
        const x = this.cursor % w;
        const y = Math.floor(this.cursor / w);
        const nx = clamp(x + Math.sign(input.dx || 0), 0, w - 1);
        const ny = clamp(y + Math.sign(input.dy || 0), 0, h - 1);
        this.cursor = ny * w + nx;
      }
    }

    update(dtMs) {
      if (this.state !== 'playing' || this.ended) return;
      if (this.lockMs > 0) {
        this.lockMs -= dtMs;
        if (this.lockMs <= 0) this.selected = [];
      }
    }

    flipCursor() {
      if (this.state !== 'playing' || this.lockMs > 0 || this.matched.has(this.cursor)) return;
      if (this.selected.includes(this.cursor)) return;
      this.selected.push(this.cursor);
      if (this.selected.length < 2) return;
      const [a, b] = this.selected;
      if (this.cards[a] === this.cards[b]) {
        this.matched.add(a);
        this.matched.add(b);
        this.score += 10;
        this.selected = [];
        if (this.matched.size >= this.cards.length) this.finish('win');
      } else {
        this.score = Math.max(0, this.score - 2);
        this.lockMs = 650;
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
      ctx.save();
      ctx.translate(metrics.x, metrics.y);
      fillBoardPanel(ctx, grid.width * cell, grid.height * cell);
      drawMemoryBackdrop(ctx, grid, cell);
      for (let i = 0; i < this.cards.length; i++) {
        const x = (i % grid.width) * cell;
        const y = Math.floor(i / grid.width) * cell;
        const open = this.matched.has(i) || this.selected.includes(i);
        drawMemoryCard(ctx, x, y, cell, open, this.matched.has(i), this.cards[i]);
        if (i === this.cursor) {
          ctx.strokeStyle = 'rgba(255,255,255,0.92)';
          ctx.lineWidth = 4;
          ctx.roundRect(x + 4, y + 4, cell - 8, cell - 8, 12);
          ctx.stroke();
        }
        if (open) {
          drawMemorySymbol(ctx, x + cell / 2, y + cell / 2, cell, this.cards[i]);
        }
      }
      ctx.restore();
      ctx.textAlign = 'start';
      ctx.textBaseline = 'alphabetic';
    }
  }

  class CatchEngine {
    constructor(config, rng) {
      this.config = config;
      this.rng = rng || Math.random;
      this.state = 'ready';
      this.score = 0;
      this.ended = false;
      const lane = this.laneColumns();
      this.playerX = Math.floor((lane.min + lane.max) / 2);
      this.spawnMs = 0;
      this.fallMs = 0;
      this.misses = 0;
      this.items = [];
    }

    getState() {
      return this.state;
    }

    handleInput(input) {
      if (!input) return;
      if (this.ended) {
        if (input.type === 'confirm') restartGame();
        else if (input.type === 'cancel') closeEndedGame(this.state);
        return;
      }
      if (input.type === 'confirm' && this.state === 'ready') {
        this.state = 'playing';
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
        if (this.state === 'ready') this.state = 'playing';
        const lane = this.laneColumns();
        this.playerX = clamp(this.playerX + Math.sign(input.dx || 0), lane.playerMin, lane.playerMax);
      }
    }

    update(dtMs) {
      if (this.state !== 'playing' || this.ended) return;
      this.spawnMs -= dtMs;
      this.fallMs += dtMs;
      if (this.spawnMs <= 0) {
        this.spawnMs = Math.max(240, this.config.player.speed_ms * 6 * this.config.rules.speed_ramp);
        const lane = this.laneColumns();
        for (let i = 0; i < this.config.rules.food_count; i++) {
          this.items.push({ x: lane.min + Math.floor(this.rng() * lane.width), y: 0 });
        }
      }
      const stepMs = Math.max(70, this.config.player.speed_ms);
      while (this.fallMs >= stepMs) {
        this.fallMs -= stepMs;
        this.step();
      }
    }

    step() {
      const bottom = this.config.grid.height - 1;
      const remaining = [];
      for (const item of this.items) {
        const next = { x: item.x, y: item.y + 1 };
        if (next.y >= bottom && Math.abs(next.x - this.playerX) <= 1) {
          this.score += 1;
          if (this.score >= this.config.rules.win_length) this.finish('win');
        } else if (next.y > bottom) {
          this.misses += 1;
          if (this.misses >= 5) this.finish('lose');
        } else {
          remaining.push(next);
        }
      }
      this.items = remaining;
    }

    finish(result) {
      if (this.ended) return;
      this.ended = true;
      this.state = result;
    }

    laneColumns() {
      const gridWidth = this.config.grid.width;
      const width = Math.max(5, Math.floor(gridWidth * 0.5));
      const min = Math.floor((gridWidth - width) / 2);
      const max = min + width - 1;
      return {
        min,
        max,
        width,
        playerMin: Math.min(max, min + 1),
        playerMax: Math.max(min, max - 1),
      };
    }

    render(ctx, metrics) {
      ctx.clearRect(0, 0, metrics.width, metrics.height);
      const grid = this.config.grid;
      const cell = metrics.cell;
      const lane = this.laneColumns();
      ctx.save();
      ctx.translate(metrics.x + lane.min * cell, metrics.y);
      fillBoardPanel(ctx, lane.width * cell, grid.height * cell);
      drawCatchLane(ctx, lane.width, grid.height, cell);
      for (const item of this.items) drawFood(ctx, { ...item, x: item.x - lane.min }, cell, this.config.theme.food);
      const y = grid.height - 1;
      const playerX = this.playerX - lane.min;
      drawCatchBasket(ctx, playerX, y, cell);
      ctx.restore();
    }
  }

  class LegacyBattleEngine {
    constructor(config) {
      this.config = config;
      this.battle = config.battle || defaultBattleConfig();
      this.state = 'ready';
      this.ended = false;
      this.score = 0;
      this.pet = {
        hp: this.battle.pet.hp,
        maxHp: this.battle.pet.hp,
        attack: this.battle.pet.attack,
        autoAttackMs: this.battle.pet.auto_attack_ms,
      };
      this.monster = {
        ...this.battle.monster,
        hp: this.battle.monster.hp,
        maxHp: this.battle.monster.hp,
        x: 0.62,
        y: 0.52,
        vx: 0.000045,
        hitFlashMs: 0,
        attackWarnMs: 0,
      };
      this.skills = (this.battle.skills || []).map((skill, index) => ({
        ...skill,
        slot: index + 1,
        cooldownLeftMs: 0,
      }));
      this.petAttackTimer = this.pet.autoAttackMs;
      this.monsterAttackTimer = this.monster.attack_interval_ms;
      this.attackWarnWindowMs = 700;
      this.interruptDelayMs = Math.max(900, Math.floor(this.monster.attack_interval_ms * 0.55));
      this.guardMs = 0;
      this.floaters = [];
      this.lastMetrics = null;
      this.startedNotified = false;
      this.inputCapture = null;
      this.inputCaptureCheckMs = 0;
      this.inputCaptureInFlight = false;
      this.inputCaptureSupported = Boolean(invoke);
    }

    getState() {
      return this.state;
    }

    handleInput(input) {
      if (!input) return;
      if (this.ended) {
        if (input.type === 'confirm' || input.type === 'attack_primary') restartGame();
        else if (input.type === 'cancel') closeEndedGame(this.state);
        return;
      }
      if (input.type === 'cancel') {
        this.finish('cancel');
        return;
      }
      if (input.type === 'pause' && (this.state === 'playing' || this.state === 'paused')) {
        this.state = this.state === 'playing' ? 'paused' : 'playing';
        return;
      }
      if (input.type === 'confirm' && this.state === 'ready') {
        this.state = 'playing';
        this.notifyStart();
        return;
      }
      if (input.type === 'attack_primary') {
        if (this.state === 'ready') this.state = 'playing';
        this.notifyStart();
        this.attackMonster(this.pet.attack, 'hit', { source: 'button' });
        return;
      }
      if (input.type === 'skill') {
        if (this.state === 'ready') this.state = 'playing';
        this.notifyStart();
        this.useSkill(input.slot);
        return;
      }
      if (input.type === 'guard') {
        if (this.state === 'ready') this.state = 'playing';
        this.notifyStart();
        this.guardMs = Math.max(this.guardMs, 700);
        this.addFloater('guard', 0.24, 0.64, '#8ecae6');
        emitBattlePet('guard', { source: 'guard', hpRatio: this.petHpRatio() });
        return;
      }
      if (input.type === 'direction') {
        const dx = Math.sign(input.dx || 0);
        if (dx !== 0) this.monster.x = clamp(this.monster.x + dx * 0.04, 0.30, 0.78);
      }
    }

    handlePointer(x, y) {
      if (this.ended) return false;
      if (this.state === 'ready') this.state = 'playing';
      this.notifyStart();
      if (this.hitTestMonster(x, y)) {
        this.attackMonster(this.pet.attack, 'tap', { source: 'pointer' });
        return true;
      }
      const skill = this.hitTestSkill(x, y);
      if (skill) {
        this.useSkill(skill.slot);
        return true;
      }
      return false;
    }

    notifyStart() {
      if (this.startedNotified) return;
      this.startedNotified = true;
      emitBattlePet('start');
    }

    update(dtMs) {
      this.updateInputCapture(dtMs);
      this.floaters.forEach((f) => {
        f.age += dtMs;
        f.y -= dtMs * 0.00008;
      });
      this.floaters = this.floaters.filter((f) => f.age < 900);
      this.skills.forEach((skill) => {
        skill.cooldownLeftMs = Math.max(0, skill.cooldownLeftMs - dtMs);
      });
      this.guardMs = Math.max(0, this.guardMs - dtMs);
      this.monster.hitFlashMs = Math.max(0, this.monster.hitFlashMs - dtMs);

      if (this.state !== 'playing' || this.ended) return;

      this.monster.x += this.monster.vx * dtMs;
      if (this.monster.x < 0.36 || this.monster.x > 0.76) this.monster.vx *= -1;
      this.monster.x = clamp(this.monster.x, 0.34, 0.78);

      this.petAttackTimer -= dtMs;
      if (this.petAttackTimer <= 0) {
        this.petAttackTimer += this.pet.autoAttackMs;
        this.attackMonster(Math.max(1, Math.floor(this.pet.attack * 0.7)), 'auto');
      }

      this.monsterAttackTimer -= dtMs;
      this.monster.attackWarnMs = this.monsterAttackTimer <= this.attackWarnWindowMs ? this.monsterAttackTimer : 0;
      if (this.monsterAttackTimer <= 0) {
        this.monsterAttackTimer += this.monster.attack_interval_ms;
        const guarded = this.guardMs > 0;
        const damage = guarded ? Math.max(1, Math.floor(this.monster.attack * 0.35)) : this.monster.attack;
        this.pet.hp = Math.max(0, this.pet.hp - damage);
        this.addFloater(`-${damage}`, 0.23, 0.55, guarded ? '#8ecae6' : '#ff6b6b');
        emitBattlePet('pet_hit', {
          source: guarded ? 'guarded_hit' : 'monster',
          damage,
          hpRatio: this.petHpRatio(),
        });
        if (this.pet.hp <= 0) this.finish('lose');
      }
    }

    attackMonster(amount, label, detail = {}) {
      if (this.ended || this.state === 'paused') return;
      this.monster.hp = Math.max(0, this.monster.hp - amount);
      this.monster.hitFlashMs = 130;
      this.addFloater(`-${amount}`, this.monster.x, this.monster.y - 0.12, label === 'auto' ? '#b8f2e6' : '#ffd166');
      if (this.monster.attackWarnMs > 0 && label !== 'auto') {
        this.interruptMonsterAttack({ ...detail, damage: amount });
      } else if (label !== 'auto') {
        emitBattlePet(label === 'skill' ? 'skill' : 'attack', {
          ...detail,
          damage: amount,
          hpRatio: this.monsterHpRatio(),
        });
      }
      if (this.monster.hp <= 0) {
        this.score = this.monster.reward_exp;
        this.finish('win');
      }
    }

    useSkill(slot) {
      const skill = this.skills.find((s) => s.slot === Number(slot));
      if (!skill || skill.cooldownLeftMs > 0 || this.state === 'paused') return;
      if (skill.damage > 0) this.attackMonster(skill.damage, 'skill', { source: 'skill', skillId: skill.id });
      if (skill.heal > 0) {
        this.pet.hp = Math.min(this.pet.maxHp, this.pet.hp + skill.heal);
        this.addFloater(`+${skill.heal}`, 0.23, 0.55, '#95d5b2');
      }
      skill.cooldownLeftMs = skill.cooldown_ms;
    }

    finish(result) {
      if (this.ended) return;
      this.ended = true;
      this.state = result;
      this.setInputCapture(true);
      if (result === 'win' || result === 'lose') {
        emitBattlePet(result, { hpRatio: result === 'win' ? this.monsterHpRatio() : this.petHpRatio() });
      }
    }

    interruptMonsterAttack(detail = {}) {
      this.monsterAttackTimer = Math.max(this.monsterAttackTimer, this.interruptDelayMs);
      this.monster.attackWarnMs = 0;
      this.addFloater('interrupt', this.monster.x, this.monster.y - 0.22, '#70d6ff');
      emitBattlePet('interrupt', {
        ...detail,
        hpRatio: this.monsterHpRatio(),
        interrupted: true,
      });
    }

    monsterHpRatio() {
      return this.monster.maxHp > 0 ? this.monster.hp / this.monster.maxHp : 0;
    }

    petHpRatio() {
      return this.pet.maxHp > 0 ? this.pet.hp / this.pet.maxHp : 0;
    }

    updateInputCapture(dtMs) {
      if (!this.inputCaptureSupported || this.inputCaptureInFlight) return;
      this.inputCaptureCheckMs -= dtMs;
      if (this.inputCaptureCheckMs > 0) return;
      this.inputCaptureCheckMs = 70;
      this.inputCaptureInFlight = true;
      invoke('cmd_game_cursor_position')
        .then((pos) => {
          const enabled = this.isInteractiveAt(pos?.x, pos?.y);
          this.setInputCapture(enabled);
        })
        .catch((e) => {
          this.inputCaptureSupported = false;
          log(`cmd_game_cursor_position disabled: ${e}`);
        })
        .finally(() => {
          this.inputCaptureInFlight = false;
        });
    }

    setInputCapture(enabled) {
      if (!this.inputCaptureSupported || this.inputCapture === enabled) return;
      this.inputCapture = enabled;
      setGameInputCapture(enabled);
    }

    isInteractiveAt(x, y) {
      if (!Number.isFinite(x) || !Number.isFinite(y)) return false;
      if (this.state === 'ready' || this.state === 'paused' || this.ended) {
        return this.hitTestOverlay(x, y);
      }
      if (this.state !== 'playing') return false;
      return this.hitTestMonster(x, y) || Boolean(this.hitTestSkill(x, y));
    }

    hitTestMonster(x, y) {
      if (!this.lastMetrics) return false;
      const m = this.lastMetrics;
      const r = monsterRect(m, this.monster);
      return x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h;
    }

    hitTestSkill(x, y) {
      if (!this.lastMetrics) return null;
      return this.skillRects(this.lastMetrics).find((entry) => (
        x >= entry.x && x <= entry.x + entry.w && y >= entry.y && y <= entry.y + entry.h
      )) || null;
    }

    hitTestOverlay(x, y) {
      if (!this.lastMetrics) return false;
      const m = this.lastMetrics;
      const w = Math.min(460, m.width * 0.82);
      const h = this.ended ? 220 : 180;
      const left = (m.width - w) / 2;
      const top = (m.height - h) / 2;
      return x >= left && x <= left + w && y >= top && y <= top + h;
    }

    skillRects(metrics) {
      const size = clamp(Math.floor(metrics.width * 0.055), 44, 64);
      const gap = 10;
      const y = metrics.height - size - 28;
      return this.skills.map((skill, index) => ({
        ...skill,
        x: Math.floor(metrics.width / 2 - (this.skills.length * size + (this.skills.length - 1) * gap) / 2 + index * (size + gap)),
        y,
        w: size,
        h: size,
      }));
    }

    addFloater(text, x, y, color) {
      this.floaters.push({ text, x, y, color, age: 0 });
    }

    render(ctx, metrics) {
      this.lastMetrics = metrics;
      ctx.clearRect(0, 0, metrics.width, metrics.height);
      drawBattleBackdrop(ctx, metrics);
      drawMonster(ctx, metrics, this.monster);
      drawBattleBars(ctx, metrics, this);
      drawSkillButtons(ctx, metrics, this.skillRects(metrics));
      drawFloaters(ctx, metrics, this.floaters);
    }
  }

  function defaultBattleConfig() {
    return {
      pet: { hp: 48, attack: 1, auto_attack_ms: 420 },
      monster: {
        id: 'intruder',
        name: '小史莱姆',
        hp: 10,
        attack: 4,
        attack_interval_ms: 1200,
        reward_exp: 20,
      },
      skills: [
        { id: 'heavy_hit', name: '重击', cooldown_ms: 3000, damage: 12, heal: 0 },
        { id: 'snack', name: '小鱼干', cooldown_ms: 6000, damage: 0, heal: 10 },
      ],
    };
  }

  class BattleEngine {
    constructor(config) {
      this.config = config;
      this.battle = config.battle || defaultBattleConfig();
      this.state = 'ready';
      this.ended = false;
      this.score = 0;
      this.pet = {
        hp: this.battle.pet.hp,
        maxHp: this.battle.pet.hp,
        attack: Math.max(1, this.battle.pet.attack),
        autoAttackMs: Math.max(260, this.battle.pet.auto_attack_ms),
      };
      this.skills = (this.battle.skills || []).map((skill, index) => ({
        ...skill,
        slot: index + 1,
        cooldownLeftMs: 0,
      }));
      this.ship = { x: 0.5, y: 0.84, flashMs: 0 };
      this.bullets = [];
      this.enemies = [];
      this.enemySpawnMs = 250;
      this.enemyWave = 0;
      this.shotCooldownMs = 0;
      this.autoShotMs = this.pet.autoAttackMs;
      this.targetScore = Number(config.rules?.win_length) || Number(this.battle.monster?.reward_exp) || 20;
      this.guardMs = 0;
      this.floaters = [];
      this.lastMetrics = null;
      this.startedNotified = false;
      this.inputCapture = null;
      this.inputCaptureCheckMs = 0;
      this.inputCaptureInFlight = false;
      this.inputCaptureSupported = Boolean(invoke);
    }

    getState() {
      return this.state;
    }

    handleInput(input) {
      if (!input) return;
      if (this.ended) {
        if (input.type === 'confirm' || input.type === 'attack_primary') restartGame();
        else if (input.type === 'cancel') closeEndedGame(this.state);
        return;
      }
      if (input.type === 'cancel') {
        this.finish('cancel');
        return;
      }
      if (input.type === 'pause' && (this.state === 'playing' || this.state === 'paused')) {
        this.state = this.state === 'playing' ? 'paused' : 'playing';
        return;
      }
      if (input.type === 'confirm' && this.state === 'ready') {
        this.state = 'playing';
        this.notifyStart();
        this.fireBullet('start');
        return;
      }
      if (input.type === 'attack_primary') {
        if (this.state === 'ready') this.state = 'playing';
        this.notifyStart();
        this.fireBullet('button');
        return;
      }
      if (input.type === 'skill') {
        if (this.state === 'ready') this.state = 'playing';
        this.notifyStart();
        this.useSkill(input.slot);
        return;
      }
      if (input.type === 'guard') {
        if (this.state === 'ready') this.state = 'playing';
        this.notifyStart();
        this.guardMs = Math.max(this.guardMs, 1000);
        this.addFloater('shield', this.ship.x, this.ship.y - 0.08, '#8ecae6');
        emitBattlePet('guard', { source: 'guard', hpRatio: this.petHpRatio() });
        return;
      }
      if (input.type === 'direction') {
        const dx = Math.sign(input.dx || 0);
        const dy = Math.sign(input.dy || 0);
        if (dx !== 0 || dy !== 0) {
          if (this.state === 'ready') this.state = 'playing';
          this.ship.x = clamp(this.ship.x + dx * 0.065, 0.08, 0.92);
          this.ship.y = clamp(this.ship.y + dy * 0.045, 0.62, 0.90);
          this.notifyStart();
        }
      }
    }

    handlePointer(x, y) {
      if (this.ended) return false;
      if (this.state === 'ready') this.state = 'playing';
      this.notifyStart();
      if (this.lastMetrics) {
        const lane = battleLane(this.lastMetrics);
        this.ship.x = clamp((x - lane.x) / lane.width, 0.08, 0.92);
        this.ship.y = clamp(y / this.lastMetrics.height, 0.62, 0.90);
      }
      this.fireBullet('pointer');
      return true;
    }

    notifyStart() {
      if (this.startedNotified) return;
      this.startedNotified = true;
      emitBattlePet('start');
    }

    update(dtMs) {
      this.updateInputCapture(dtMs);
      this.floaters.forEach((f) => {
        f.age += dtMs;
        f.y -= dtMs * 0.00008;
      });
      this.floaters = this.floaters.filter((f) => f.age < 900);
      this.skills.forEach((skill) => {
        skill.cooldownLeftMs = Math.max(0, skill.cooldownLeftMs - dtMs);
      });
      this.guardMs = Math.max(0, this.guardMs - dtMs);
      this.ship.flashMs = Math.max(0, this.ship.flashMs - dtMs);

      if (this.state !== 'playing' || this.ended) return;

      this.shotCooldownMs = Math.max(0, this.shotCooldownMs - dtMs);
      this.autoShotMs -= dtMs;
      if (this.autoShotMs <= 0) {
        this.autoShotMs += this.pet.autoAttackMs;
        this.fireBullet('auto', true);
      }
      this.enemySpawnMs -= dtMs;
      if (this.enemySpawnMs <= 0) this.spawnEnemy();
      this.updateBullets(dtMs);
      this.updateEnemies(dtMs);
      this.resolveBulletHits();
      this.resolveShipHits();
    }

    fireBullet(source = 'button', quiet = false) {
      if (this.ended || this.state === 'paused' || this.shotCooldownMs > 0) return;
      this.shotCooldownMs = source === 'auto' ? 0 : 150;
      this.bullets.push({
        x: this.ship.x,
        y: this.ship.y - 0.07,
        vy: -0.00135,
        damage: this.pet.attack,
      });
      if (!quiet) emitBattlePet('attack', { source, damage: this.pet.attack, hpRatio: this.monsterHpRatio() });
    }

    spawnEnemy() {
      this.enemyWave += 1;
      const hard = this.enemyWave % 7 === 0;
      const fast = this.enemyWave % 5 === 0;
      const hp = hard ? Math.max(2, Math.ceil(this.battle.monster.hp / 14)) : 1;
      const attack = hard ? this.battle.monster.attack + 1 : this.battle.monster.attack;
      this.enemies.push({
        x: 0.08 + battleRandom(this.enemyWave) * 0.84,
        y: -0.08,
        hp,
        maxHp: hp,
        attack,
        vy: fast ? 0.00035 : hard ? 0.00022 : 0.00027,
        wobble: (this.enemyWave % 2 === 0 ? 1 : -1) * 0.000045,
        kind: hard ? 'heavy' : fast ? 'fast' : 'normal',
        hitFlashMs: 0,
      });
      const ramp = Math.min(420, this.score * 8);
      this.enemySpawnMs = Math.max(520, this.battle.monster.attack_interval_ms - ramp);
    }

    updateBullets(dtMs) {
      for (const bullet of this.bullets) bullet.y += bullet.vy * dtMs;
      this.bullets = this.bullets.filter((bullet) => bullet.y > -0.12);
    }

    updateEnemies(dtMs) {
      for (const enemy of this.enemies) {
        enemy.y += enemy.vy * dtMs;
        enemy.x = clamp(enemy.x + Math.sin((enemy.y + this.enemyWave) * 9) * enemy.wobble * dtMs, 0.06, 0.94);
        enemy.hitFlashMs = Math.max(0, enemy.hitFlashMs - dtMs);
      }
      const remaining = [];
      for (const enemy of this.enemies) {
        if (enemy.y > 1.02) {
          this.addFloater('miss', enemy.x, 0.92, '#ff6b6b');
          this.damagePet(enemy.attack, 'leak');
        } else {
          remaining.push(enemy);
        }
      }
      this.enemies = remaining;
    }

    resolveBulletHits() {
      const bullets = [];
      for (const bullet of this.bullets) {
        const enemy = this.enemies.find((candidate) => distanceSq(bullet, candidate) < 0.0022);
        if (!enemy) {
          bullets.push(bullet);
          continue;
        }
        enemy.hp -= bullet.damage;
        enemy.hitFlashMs = 120;
        if (enemy.hp <= 0) this.defeatEnemy(enemy, bullet.damage);
      }
      this.bullets = bullets;
      this.enemies = this.enemies.filter((enemy) => enemy.hp > 0);
    }

    resolveShipHits() {
      const remaining = [];
      for (const enemy of this.enemies) {
        if (distanceSq(enemy, this.ship) < 0.0048) {
          this.damagePet(enemy.attack, 'collision');
        } else {
          remaining.push(enemy);
        }
      }
      this.enemies = remaining;
    }

    defeatEnemy(enemy, damage) {
      this.score += 1;
      this.addFloater('+1', enemy.x, enemy.y, '#ffd166');
      emitBattlePet('skill', { source: 'enemy_down', damage, hpRatio: this.monsterHpRatio() });
      if (this.score >= this.targetScore) this.finish('win');
    }

    damagePet(amount, source) {
      const guarded = this.guardMs > 0;
      const damage = guarded ? Math.max(1, Math.floor(amount * 0.35)) : amount;
      this.pet.hp = Math.max(0, this.pet.hp - damage);
      this.ship.flashMs = 180;
      const y = source === 'leak' ? 0.86 : this.ship.y - 0.10;
      this.addFloater(`HP -${damage}`, this.ship.x, y, guarded ? '#8ecae6' : '#ff6b6b');
      emitBattlePet('pet_hit', {
        source: guarded ? `guarded_${source}` : source,
        damage,
        hpRatio: this.petHpRatio(),
      });
      if (this.pet.hp <= 0) this.finish('lose');
    }

    useSkill(slot) {
      const skill = this.skills.find((s) => s.slot === Number(slot));
      if (!skill || skill.cooldownLeftMs > 0 || this.state === 'paused') return;
      if (skill.damage > 0) {
        const targets = this.enemies.slice(0, 8);
        for (const enemy of targets) {
          enemy.hp -= skill.damage;
          enemy.hitFlashMs = 160;
          if (enemy.hp <= 0) this.defeatEnemy(enemy, skill.damage);
        }
        this.enemies = this.enemies.filter((enemy) => enemy.hp > 0);
        this.addFloater('blast', this.ship.x, this.ship.y - 0.16, '#ffd166');
        emitBattlePet('skill', { source: 'skill', skillId: skill.id, damage: skill.damage, hpRatio: this.monsterHpRatio() });
      }
      if (skill.heal > 0) {
        this.pet.hp = Math.min(this.pet.maxHp, this.pet.hp + skill.heal);
        this.addFloater(`+${skill.heal}`, this.ship.x, this.ship.y - 0.10, '#95d5b2');
      }
      skill.cooldownLeftMs = skill.cooldown_ms;
    }

    finish(result) {
      if (this.ended) return;
      this.ended = true;
      this.state = result;
      this.setInputCapture(true);
      if (result === 'win' || result === 'lose') {
        emitBattlePet(result, { hpRatio: result === 'win' ? this.monsterHpRatio() : this.petHpRatio() });
      }
    }

    monsterHpRatio() {
      return this.targetScore > 0 ? clamp(1 - this.score / this.targetScore, 0, 1) : 0;
    }

    petHpRatio() {
      return this.pet.maxHp > 0 ? this.pet.hp / this.pet.maxHp : 0;
    }

    updateInputCapture(dtMs) {
      if (!this.inputCaptureSupported || this.inputCaptureInFlight) return;
      this.inputCaptureCheckMs -= dtMs;
      if (this.inputCaptureCheckMs > 0) return;
      this.inputCaptureCheckMs = 70;
      this.inputCaptureInFlight = true;
      invoke('cmd_game_cursor_position')
        .then((pos) => {
          const enabled = this.isInteractiveAt(pos?.x, pos?.y);
          this.setInputCapture(enabled);
        })
        .catch((e) => {
          this.inputCaptureSupported = false;
          log(`cmd_game_cursor_position disabled: ${e}`);
        })
        .finally(() => {
          this.inputCaptureInFlight = false;
        });
    }

    setInputCapture(enabled) {
      if (!this.inputCaptureSupported || this.inputCapture === enabled) return;
      this.inputCapture = enabled;
      setGameInputCapture(enabled);
    }

    isInteractiveAt(x, y) {
      if (!Number.isFinite(x) || !Number.isFinite(y) || !this.lastMetrics) return false;
      if (this.state === 'ready' || this.state === 'paused' || this.ended) {
        return this.hitTestOverlay(x, y);
      }
      if (this.state !== 'playing') return false;
      return y >= this.lastMetrics.height * 0.58;
    }

    hitTestSkill(x, y) {
      if (!this.lastMetrics) return null;
      return this.skillRects(this.lastMetrics).find((entry) => (
        x >= entry.x && x <= entry.x + entry.w && y >= entry.y && y <= entry.y + entry.h
      )) || null;
    }

    hitTestOverlay(x, y) {
      if (!this.lastMetrics) return false;
      const m = this.lastMetrics;
      const w = Math.min(460, m.width * 0.82);
      const h = this.ended ? 220 : 180;
      const left = (m.width - w) / 2;
      const top = (m.height - h) / 2;
      return x >= left && x <= left + w && y >= top && y <= top + h;
    }

    skillRects(metrics) {
      const size = clamp(Math.floor(metrics.width * 0.055), 44, 64);
      const gap = 10;
      const y = metrics.height - size - 28;
      return this.skills.map((skill, index) => ({
        ...skill,
        x: Math.floor(metrics.width / 2 - (this.skills.length * size + (this.skills.length - 1) * gap) / 2 + index * (size + gap)),
        y,
        w: size,
        h: size,
      }));
    }

    addFloater(text, x, y, color) {
      this.floaters.push({ text, x, y, color, age: 0 });
    }

    render(ctx, metrics) {
      this.lastMetrics = metrics;
      ctx.clearRect(0, 0, metrics.width, metrics.height);
      drawBattleBackdrop(ctx, metrics);
      drawBattleBullets(ctx, metrics, this.bullets);
      drawBattleEnemies(ctx, metrics, this.enemies);
      drawBattleShip(ctx, metrics, this.ship, this.guardMs);
      drawBattleBars(ctx, metrics, this);
      drawFloaters(ctx, metrics, this.floaters);
    }
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

  function fillBoardPanel(ctx, width, height) {
    const gradient = ctx.createLinearGradient(0, 0, width, height);
    gradient.addColorStop(0, 'rgba(10,16,22,0.46)');
    gradient.addColorStop(0.52, 'rgba(16,24,32,0.30)');
    gradient.addColorStop(1, 'rgba(8,12,18,0.48)');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);
  }

  function drawMemoryBackdrop(ctx, grid, cell) {
    const width = grid.width * cell;
    const height = grid.height * cell;
    ctx.save();
    ctx.strokeStyle = 'rgba(255,255,255,0.22)';
    ctx.lineWidth = 2;
    ctx.roundRect(2, 2, width - 4, height - 4, 16);
    ctx.stroke();
    ctx.fillStyle = 'rgba(112,214,255,0.055)';
    for (let y = 0; y < grid.height; y++) {
      for (let x = 0; x < grid.width; x++) {
        if ((x + y) % 2 === 0) ctx.fillRect(x * cell, y * cell, cell, cell);
      }
    }
    ctx.restore();
  }

  function drawMemoryCard(ctx, x, y, cell, open, matched, value) {
    const pad = Math.max(8, cell * 0.08);
    const r = Math.max(10, cell * 0.12);
    const cardX = x + pad;
    const cardY = y + pad;
    const cardW = cell - pad * 2;
    const cardH = cell - pad * 2;
    ctx.save();
    ctx.shadowColor = 'rgba(0,0,0,0.24)';
    ctx.shadowBlur = 10;
    ctx.shadowOffsetY = 4;
    const gradient = ctx.createLinearGradient(cardX, cardY, cardX, cardY + cardH);
    if (open) {
      gradient.addColorStop(0, matched ? '#b8f2e6' : '#ffd166');
      gradient.addColorStop(1, matched ? '#70d6ff' : '#ffafcc');
    } else {
      gradient.addColorStop(0, '#26313a');
      gradient.addColorStop(1, '#151b22');
    }
    ctx.fillStyle = gradient;
    ctx.beginPath();
    ctx.roundRect(cardX, cardY, cardW, cardH, r);
    ctx.fill();
    ctx.shadowColor = 'transparent';
    ctx.strokeStyle = open ? 'rgba(255,255,255,0.68)' : 'rgba(112,214,255,0.30)';
    ctx.lineWidth = 2;
    ctx.stroke();
    if (!open) {
      ctx.strokeStyle = 'rgba(255,255,255,0.16)';
      ctx.lineWidth = 3;
      ctx.beginPath();
      const inset = cardW * 0.24;
      ctx.arc(cardX + cardW / 2, cardY + cardH / 2, inset, 0, Math.PI * 2);
      ctx.stroke();
      ctx.fillStyle = 'rgba(255,255,255,0.18)';
      ctx.font = `800 ${Math.max(18, cell * 0.22)}px "Segoe UI", sans-serif`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText('?', cardX + cardW / 2, cardY + cardH / 2 + 1);
    } else {
      ctx.fillStyle = 'rgba(255,255,255,0.28)';
      ctx.beginPath();
      ctx.arc(cardX + cardW * 0.20, cardY + cardH * 0.20, Math.max(3, value + 3), 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.restore();
  }

  function drawMemorySymbol(ctx, x, y, cell, value) {
    const palette = ['#20242a', '#3a0ca3', '#006d77', '#9b2226', '#31572c', '#5f0f40', '#073b4c', '#6c584c'];
    const sides = 3 + (value % 5);
    const r = Math.max(14, cell * 0.20);
    ctx.save();
    ctx.fillStyle = palette[value % palette.length];
    ctx.beginPath();
    for (let i = 0; i < sides; i++) {
      const angle = -Math.PI / 2 + (Math.PI * 2 * i) / sides;
      const px = x + Math.cos(angle) * r;
      const py = y + Math.sin(angle) * r;
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = 'rgba(255,255,255,0.88)';
    ctx.beginPath();
    ctx.arc(x + r * 0.22, y - r * 0.26, Math.max(2, r * 0.16), 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function drawCatchLane(ctx, laneWidth, gridHeight, cell) {
    const width = laneWidth * cell;
    const height = gridHeight * cell;
    ctx.save();
    ctx.strokeStyle = 'rgba(255,255,255,0.36)';
    ctx.lineWidth = 2;
    ctx.roundRect(1, 1, width - 2, height - 2, 16);
    ctx.stroke();
    ctx.strokeStyle = 'rgba(255,209,102,0.28)';
    ctx.lineWidth = 1;
    for (let y = cell * 2; y < height - cell; y += cell * 2) {
      ctx.beginPath();
      ctx.moveTo(cell, y);
      ctx.lineTo(width - cell, y);
      ctx.stroke();
    }
    ctx.restore();
  }

  function drawCatchBasket(ctx, playerX, y, cell) {
    const x = (playerX - 1) * cell + 4;
    const top = y * cell + cell * 0.22;
    const w = cell * 3 - 8;
    const h = cell * 0.66;
    ctx.save();
    ctx.shadowColor = 'rgba(0,0,0,0.28)';
    ctx.shadowBlur = 8;
    ctx.fillStyle = '#ffd166';
    ctx.beginPath();
    ctx.moveTo(x + w * 0.10, top);
    ctx.lineTo(x + w * 0.90, top);
    ctx.lineTo(x + w * 0.76, top + h);
    ctx.quadraticCurveTo(x + w * 0.50, top + h * 1.08, x + w * 0.24, top + h);
    ctx.closePath();
    ctx.fill();
    ctx.shadowColor = 'transparent';
    ctx.strokeStyle = 'rgba(32,36,42,0.56)';
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.moveTo(x + w * 0.18, top + h * 0.18);
    ctx.quadraticCurveTo(x + w * 0.50, top - h * 0.42, x + w * 0.82, top + h * 0.18);
    ctx.stroke();
    ctx.strokeStyle = 'rgba(32,36,42,0.22)';
    ctx.lineWidth = 1.5;
    for (let i = 1; i < 4; i++) {
      const px = x + (w * i) / 4;
      ctx.beginPath();
      ctx.moveTo(px, top + h * 0.18);
      ctx.lineTo(px - w * 0.04, top + h * 0.86);
      ctx.stroke();
    }
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

  function distanceSq(a, b) {
    const dx = a.x - b.x;
    const dy = a.y - b.y;
    return dx * dx + dy * dy;
  }

  function battleRandom(seed) {
    const value = Math.sin(seed * 12.9898 + performance.now() * 0.001) * 43758.5453;
    return value - Math.floor(value);
  }

  function battleLane(metrics) {
    const width = Math.max(180, Math.floor(metrics.width * 0.5));
    return {
      x: Math.floor((metrics.width - width) / 2),
      width,
    };
  }

  function battleX(metrics, normalizedX) {
    const lane = battleLane(metrics);
    return lane.x + lane.width * normalizedX;
  }

  function drawBattleBullets(ctx, metrics, bullets) {
    ctx.save();
    for (const bullet of bullets) {
      const x = battleX(metrics, bullet.x);
      const y = metrics.height * bullet.y;
      const glow = ctx.createRadialGradient(x, y, 2, x, y, 18);
      glow.addColorStop(0, 'rgba(255,255,255,0.90)');
      glow.addColorStop(0.45, 'rgba(255,209,102,0.78)');
      glow.addColorStop(1, 'rgba(255,209,102,0)');
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.ellipse(x, y, 7, 20, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = '#fff3b0';
      ctx.beginPath();
      ctx.roundRect(x - 3, y - 16, 6, 20, 4);
      ctx.fill();
    }
    ctx.restore();
  }

  function drawBattleEnemies(ctx, metrics, enemies) {
    ctx.save();
    for (const enemy of enemies) {
      const lane = battleLane(metrics);
      const size = enemy.kind === 'heavy'
        ? clamp(Math.floor(lane.width * 0.07), 36, 62)
        : clamp(Math.floor(lane.width * 0.055), 28, 48);
      const x = Math.floor(battleX(metrics, enemy.x) - size / 2);
      const y = Math.floor(metrics.height * enemy.y - size / 2);
      const cx = x + size / 2;
      const cy = y + size / 2;
      ctx.fillStyle = enemy.hitFlashMs > 0 ? '#ffafcc' : enemy.kind === 'fast' ? '#8ecae6' : '#70d6ff';
      ctx.beginPath();
      ctx.ellipse(cx, cy, size * 0.48, size * 0.38, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.beginPath();
      ctx.moveTo(cx - size * 0.38, cy + size * 0.02);
      ctx.lineTo(cx - size * 0.66, cy + size * 0.34);
      ctx.lineTo(cx - size * 0.28, cy + size * 0.28);
      ctx.closePath();
      ctx.fill();
      ctx.beginPath();
      ctx.moveTo(cx + size * 0.38, cy + size * 0.02);
      ctx.lineTo(cx + size * 0.66, cy + size * 0.34);
      ctx.lineTo(cx + size * 0.28, cy + size * 0.28);
      ctx.closePath();
      ctx.fill();
      ctx.fillStyle = '#20242a';
      ctx.beginPath();
      ctx.arc(cx - size * 0.16, cy - size * 0.06, size * 0.055, 0, Math.PI * 2);
      ctx.arc(cx + size * 0.16, cy - size * 0.06, size * 0.055, 0, Math.PI * 2);
      ctx.fill();
      if (enemy.maxHp > 1) {
        ctx.fillStyle = 'rgba(255,255,255,0.72)';
        ctx.fillRect(x + 4, y - 8, size - 8, 5);
        ctx.fillStyle = '#ef476f';
        ctx.fillRect(x + 4, y - 8, (size - 8) * (enemy.hp / enemy.maxHp), 5);
      }
    }
    ctx.restore();
  }

  function drawBattleShip(ctx, metrics, ship, guardMs) {
    const lane = battleLane(metrics);
    const x = battleX(metrics, ship.x);
    const y = metrics.height * ship.y;
    const size = clamp(Math.floor(lane.width * 0.076), 38, 58);
    ctx.save();
    if (guardMs > 0) {
      ctx.strokeStyle = 'rgba(142, 202, 230, 0.72)';
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.arc(x, y, size * 0.72, 0, Math.PI * 2);
      ctx.stroke();
    }
    const body = ctx.createLinearGradient(x, y - size, x, y + size);
    body.addColorStop(0, ship.flashMs > 0 ? '#ffafcc' : '#fff3b0');
    body.addColorStop(0.62, '#ffd166');
    body.addColorStop(1, '#f77f00');
    ctx.fillStyle = body;
    ctx.beginPath();
    ctx.moveTo(x, y - size * 0.68);
    ctx.lineTo(x - size * 0.48, y + size * 0.45);
    ctx.lineTo(x, y + size * 0.20);
    ctx.lineTo(x + size * 0.48, y + size * 0.45);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = '#20242a';
    ctx.beginPath();
    ctx.ellipse(x, y - size * 0.12, size * 0.12, size * 0.16, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = 'rgba(255, 255, 255, 0.70)';
    ctx.beginPath();
    ctx.moveTo(x - size * 0.22, y + size * 0.50);
    ctx.lineTo(x - size * 0.12, y + size * 0.80);
    ctx.lineTo(x - size * 0.02, y + size * 0.50);
    ctx.closePath();
    ctx.moveTo(x + size * 0.02, y + size * 0.50);
    ctx.lineTo(x + size * 0.12, y + size * 0.80);
    ctx.lineTo(x + size * 0.22, y + size * 0.50);
    ctx.closePath();
    ctx.fill();
    ctx.restore();
  }

  function monsterRect(metrics, monster) {
    const size = clamp(Math.floor(metrics.width * 0.085), 64, 116);
    return {
      x: Math.floor(metrics.width * monster.x - size / 2),
      y: Math.floor(metrics.height * monster.y - size / 2),
      w: size,
      h: size,
    };
  }

  function drawBattleBackdrop(ctx, metrics) {
    const lane = battleLane(metrics);
    ctx.save();
    ctx.fillStyle = 'rgba(8, 12, 16, 0.18)';
    ctx.fillRect(0, 0, metrics.width, metrics.height);
    const gradient = ctx.createLinearGradient(lane.x, 0, lane.x + lane.width, 0);
    gradient.addColorStop(0, 'rgba(112, 214, 255, 0)');
    gradient.addColorStop(0.18, 'rgba(112, 214, 255, 0.10)');
    gradient.addColorStop(0.82, 'rgba(255, 209, 102, 0.10)');
    gradient.addColorStop(1, 'rgba(255, 209, 102, 0)');
    ctx.fillStyle = gradient;
    ctx.fillRect(lane.x, 0, lane.width, metrics.height);
    ctx.strokeStyle = 'rgba(255,255,255,0.10)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(lane.x, 0);
    ctx.lineTo(lane.x, metrics.height);
    ctx.moveTo(lane.x + lane.width, 0);
    ctx.lineTo(lane.x + lane.width, metrics.height);
    ctx.stroke();
    ctx.restore();
  }

  function drawMonster(ctx, metrics, monster) {
    const r = monsterRect(metrics, monster);
    const shake = monster.hitFlashMs > 0 ? Math.sin(monster.hitFlashMs * 0.6) * 4 : 0;
    ctx.save();
    ctx.translate(shake, 0);
    if (monster.attackWarnMs > 0) {
      ctx.strokeStyle = 'rgba(255, 107, 107, 0.78)';
      ctx.lineWidth = 5;
      ctx.beginPath();
      ctx.arc(r.x + r.w / 2, r.y + r.h / 2, r.w * 0.72, 0, Math.PI * 2);
      ctx.stroke();
    }
    ctx.fillStyle = monster.hitFlashMs > 0 ? '#ffafcc' : '#70d6ff';
    ctx.fillRect(r.x, r.y + r.h * 0.18, r.w, r.h * 0.72);
    ctx.fillStyle = '#20242a';
    ctx.fillRect(r.x + r.w * 0.25, r.y + r.h * 0.42, r.w * 0.12, r.h * 0.12);
    ctx.fillRect(r.x + r.w * 0.62, r.y + r.h * 0.42, r.w * 0.12, r.h * 0.12);
    ctx.fillStyle = 'rgba(255,255,255,0.78)';
    ctx.fillRect(r.x + r.w * 0.08, r.y - 16, r.w * 0.84, 7);
    ctx.fillStyle = '#ef476f';
    ctx.fillRect(r.x + r.w * 0.08, r.y - 16, r.w * 0.84 * (monster.hp / monster.maxHp), 7);
    ctx.restore();
  }

  function drawBattleBars(ctx, metrics, engine) {
    ctx.save();
    const hpRatio = engine.petHpRatio();
    const targetRatio = engine.targetScore > 0 ? clamp(engine.score / engine.targetScore, 0, 1) : 0;
    ctx.fillStyle = '#f7fbff';
    ctx.font = '700 16px "Segoe UI", "Microsoft YaHei", sans-serif';
    ctx.fillText(engine.battle.monster.name, 22, 36);
    ctx.font = '600 13px "Segoe UI", "Microsoft YaHei", sans-serif';
    ctx.fillText(`桌宠 HP ${engine.pet.hp}/${engine.pet.maxHp}`, 22, 58);
    drawHudBar(ctx, 96, 48, 150, 12, hpRatio, hpRatio < 0.35 ? '#ff6b6b' : '#95d5b2');
    ctx.fillText(`Targets ${engine.score}/${engine.targetScore}`, 22, 82);
    drawHudBar(ctx, 120, 72, 126, 10, targetRatio, '#ffd166');
    if (engine.guardMs > 0) {
      ctx.fillStyle = '#8ecae6';
      ctx.fillText('Guard', 22, 104);
    }
    ctx.restore();
  }

  function drawHudBar(ctx, x, y, w, h, ratio, fill) {
    ctx.save();
    ctx.fillStyle = 'rgba(8,12,16,0.52)';
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, h / 2);
    ctx.fill();
    ctx.strokeStyle = 'rgba(255,255,255,0.30)';
    ctx.lineWidth = 1.5;
    ctx.stroke();
    ctx.fillStyle = fill;
    ctx.beginPath();
    ctx.roundRect(x + 2, y + 2, Math.max(0, (w - 4) * ratio), h - 4, Math.max(1, (h - 4) / 2));
    ctx.fill();
    ctx.restore();
  }

  function drawSkillButtons(ctx, metrics, skills) {
    ctx.save();
    skills.forEach((skill) => {
      const ready = skill.cooldownLeftMs <= 0;
      ctx.fillStyle = ready ? 'rgba(255, 209, 102, 0.78)' : 'rgba(20, 24, 30, 0.68)';
      ctx.strokeStyle = ready ? 'rgba(255,255,255,0.55)' : 'rgba(255,255,255,0.16)';
      ctx.lineWidth = 2;
      ctx.fillRect(skill.x, skill.y, skill.w, skill.h);
      ctx.strokeRect(skill.x, skill.y, skill.w, skill.h);
      ctx.fillStyle = ready ? '#20242a' : '#f7fbff';
      ctx.font = '800 18px "Segoe UI", sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText(String(skill.slot), skill.x + skill.w / 2, skill.y + 24);
      ctx.font = '700 11px "Segoe UI", "Microsoft YaHei", sans-serif';
      ctx.fillText(skill.name.slice(0, 4), skill.x + skill.w / 2, skill.y + skill.h - 10);
      if (!ready) {
        ctx.fillStyle = 'rgba(0,0,0,0.32)';
        const h = skill.h * (skill.cooldownLeftMs / skill.cooldown_ms);
        ctx.fillRect(skill.x, skill.y + skill.h - h, skill.w, h);
      }
    });
    ctx.textAlign = 'start';
    ctx.restore();
  }

  function drawFloaters(ctx, metrics, floaters) {
    ctx.save();
    ctx.font = '800 18px "Segoe UI", sans-serif';
    ctx.textAlign = 'center';
    floaters.forEach((f) => {
      ctx.globalAlpha = clamp(1 - f.age / 900, 0, 1);
      ctx.fillStyle = f.color;
      ctx.fillText(f.text, metrics.width * f.x, metrics.height * f.y);
    });
    ctx.restore();
  }

  function resizeCanvas() {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.floor(window.innerWidth * dpr);
    canvas.height = Math.floor(window.innerHeight * dpr);
    canvas.style.width = `${window.innerWidth}px`;
    canvas.style.height = `${window.innerHeight}px`;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function metricsFor(config) {
    const marginX = clamp(Math.floor(window.innerWidth * 0.035), 28, 88);
    const marginY = clamp(Math.floor(window.innerHeight * 0.06), 34, 96);
    const maxCell = Math.min(
      Math.floor((window.innerWidth - marginX * 2) / config.grid.width),
      Math.floor((window.innerHeight - marginY * 2) / config.grid.height)
    );
    const cell = Math.max(8, maxCell);
    const w = config.grid.width * cell;
    const h = config.grid.height * cell;
    return {
      width: window.innerWidth,
      height: window.innerHeight,
      cell,
      x: Math.floor((window.innerWidth - w) / 2),
      y: Math.floor((window.innerHeight - h) / 2),
    };
  }

  function setOverlay(kind) {
    if (!engine) return;
    overlayActions.classList.add('hidden');
    if (kind === 'ready') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = engine.config.dialogue.start;
      if (engine instanceof BattleEngine) {
        overlayText.textContent = 'Move ship - A / Space fire - X/Y skills - L1 / Shift shield';
      } else if (engine instanceof MemoryEngine) {
        overlayText.textContent = 'Move cursor · Enter / A flip';
      } else if (engine instanceof CatchEngine) {
        overlayText.textContent = 'Left / Right to catch · miss 5 and lose';
      } else {
        overlayText.textContent = 'Arrow keys / WASD to start';
      }
    } else if (kind === 'paused') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = '暂停';
      overlayText.textContent = 'P 或 Start 继续';
    } else if (kind === 'win') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = engine.config.dialogue.win;
      overlayText.textContent = `奖励 ${engine.score} · Enter / A 再来一次 · Esc / B 退出`;
      overlayActions.classList.remove('hidden');
    } else if (kind === 'lose') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = engine.config.dialogue.lose;
      overlayText.textContent = `奖励 ${engine.score} · Enter / A 再来一次 · Esc / B 退出`;
      overlayActions.classList.remove('hidden');
    } else if (kind === 'cancel') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = '已退出';
      overlayText.textContent = `奖励 ${engine.score}`;
    } else {
      overlay.classList.add('hidden');
    }
  }

  function createEngine(config) {
    switch (config && config.game_type) {
      case 'battle':
        return new BattleEngine(config);
      case 'memory':
        return new MemoryEngine(config);
      case 'catch':
        return new CatchEngine(config);
      case 'snake':
      default:
        return new SnakeEngine(config);
    }
  }

  function restartGame() {
    if (!currentConfig) return;
    engine = createEngine(currentConfig);
    reported = false;
    closing = false;
    lastLoggedState = null;
    lastTime = performance.now();
    setOverlay('ready');
    log(`restart game title=${currentConfig.title}`);
  }

  async function closeEndedGame(result) {
    if (closing) return;
    closing = true;
    if (invoke) {
      try {
        await invoke('cmd_game_end', { result, score: engine.score });
      } catch (e) {
        log(`cmd_game_end failed: ${e}`);
      }
    }
  }

  function reportEnd(result) {
    if (reported) return;
    reported = true;
    setOverlay(result);
    log(`end screen shown result=${result} score=${engine.score}`);
    if (result === 'cancel') {
      closeEndedGame(result);
      return;
    }
  }

  function updateHud() {
    titleEl.textContent = engine.config.title;
    scoreEl.textContent = String(engine.score);
    lengthEl.textContent = engine instanceof BattleEngine
      ? `${engine.pet.hp}/${engine.pet.maxHp} HP - ${engine.score}/${engine.targetScore}`
      : engine instanceof MemoryEngine
        ? `${engine.matched.size}/${engine.cards.length}`
        : engine instanceof CatchEngine
          ? `${engine.misses}/5`
          : String(engine.snake.length);
  }

  function loop(now) {
    const dt = Math.min(100, now - lastTime);
    lastTime = now;
    if (engine) {
      engine.update(dt);
      engine.render(ctx, metricsFor(engine.config));
      updateHud();
      const state = engine.getState();
      if (state !== lastLoggedState) {
        log(`state changed ${lastLoggedState || '<none>'} -> ${state} score=${engine.score}`);
        lastLoggedState = state;
      }
      if (state === 'ready' || state === 'paused') {
        setOverlay(state);
      } else if (state === 'playing') {
        setOverlay('playing');
      } else if (state === 'win' || state === 'lose' || state === 'cancel') {
        reportEnd(state);
      }
    }
    requestAnimationFrame(loop);
  }

  function toInputFromKey(e) {
    if (engine instanceof BattleEngine) {
      switch (e.key) {
        case ' ':
        case 'j':
        case 'J':
          return { type: 'attack_primary' };
        case 'Enter':
          return { type: 'confirm' };
        case '1':
        case 'k':
        case 'K':
          return { type: 'skill', slot: 1 };
        case '2':
        case 'l':
        case 'L':
          return { type: 'skill', slot: 2 };
        case '3':
        case 'i':
        case 'I':
          return { type: 'skill', slot: 3 };
        case 'Shift':
          return { type: 'guard' };
        case 'ArrowLeft':
        case 'a':
        case 'A':
          return { type: 'direction', dx: -1, dy: 0 };
        case 'ArrowRight':
        case 'd':
        case 'D':
          return { type: 'direction', dx: 1, dy: 0 };
        case 'ArrowUp':
        case 'w':
        case 'W':
          return { type: 'direction', dx: 0, dy: -1 };
        case 'ArrowDown':
        case 's':
        case 'S':
          return { type: 'direction', dx: 0, dy: 1 };
        case 'Escape':
          return { type: 'cancel' };
        case 'p':
        case 'P':
          return { type: 'pause' };
        default:
          return null;
      }
    }
    switch (e.key) {
      case 'ArrowUp':
      case 'w':
      case 'W':
        if (engine instanceof CatchEngine) return null;
        return { type: 'direction', dx: 0, dy: -1 };
      case 'ArrowDown':
      case 's':
      case 'S':
        if (engine instanceof CatchEngine) return null;
        return { type: 'direction', dx: 0, dy: 1 };
      case 'ArrowLeft':
      case 'a':
      case 'A':
        return { type: 'direction', dx: -1, dy: 0 };
      case 'ArrowRight':
      case 'd':
      case 'D':
        return { type: 'direction', dx: 1, dy: 0 };
      case 'Enter':
      case ' ':
        if (engine instanceof SnakeEngine && !engine.ended) return { type: 'boost', active: true };
        return { type: 'confirm' };
      case 'Escape':
        return { type: 'cancel' };
      case 'p':
      case 'P':
        return { type: 'pause' };
      default:
        return null;
    }
  }

  async function initEvents() {
    log('init events begin');
    document.addEventListener('keydown', (e) => {
      const input = toInputFromKey(e);
      if (!input || !engine) return;
      e.preventDefault();
      engine.handleInput(input);
    });
    document.addEventListener('keyup', (e) => {
      if (!(engine instanceof SnakeEngine)) return;
      if (e.key !== 'Enter' && e.key !== ' ') return;
      e.preventDefault();
      engine.handleInput({ type: 'boost', active: false });
    });
    window.addEventListener('resize', resizeCanvas);
    canvas.addEventListener('pointerdown', (e) => {
      if (!(engine instanceof BattleEngine)) return;
      const rect = canvas.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      if (engine.handlePointer(x, y)) e.preventDefault();
    });
    restartBtn.addEventListener('click', restartGame);
    quitBtn.addEventListener('click', () => {
      if (engine) closeEndedGame(engine.getState());
    });
    if (listen) {
      await listen('game-input', (event) => {
        log(`game-input event ${JSON.stringify(event.payload)}`);
        if (engine) engine.handleInput(event.payload);
      });
      log('game-input listener registered');
    } else {
      log('event.listen unavailable; gamepad input disabled');
    }
  }

  async function init() {
    log('init start');
    resizeCanvas();
    let config;
    try {
      config = invoke ? await invoke('cmd_get_current_game') : null;
      log(`current game loaded title=${config && config.title}`);
    } catch (e) {
      log(`cmd_get_current_game failed: ${e}`);
    }
    config = config || {
      game_type: 'snake',
      title: '毛线球大作战',
      grid: { width: 30, height: 20, cell_size: 24 },
      player: { speed_ms: 140, initial_length: 3 },
      rules: { walls_kill: true, self_kill: true, food_count: 1, speed_ramp: 0.95, win_length: 80 },
      theme: { head: 'cat', body: 'yarn', food: 'mouse', trail_alpha: 0.55 },
      dialogue: { start: '喵！看我的！', win: '太厉害了喵~', lose: '呜...再来一次！' },
    };
    currentConfig = config;
    engine = createEngine(config);
    await initEvents();
    log(`engine ready title=${config.title}`);
    requestAnimationFrame(loop);
  }

  window.GameEngineTest = { SnakeEngine, MemoryEngine, CatchEngine, BattleEngine, createRng };
  init();
})();
