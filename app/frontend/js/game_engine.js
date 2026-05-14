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

  function clamp(n, min, max) {
    return Math.max(min, Math.min(max, n));
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
      this.snake = [];
      this.food = { x: 0, y: 0 };
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
      if (this.state !== 'playing' || this.ended) return;
      this.tickMs += dtMs;
      while (this.tickMs >= this.stepMs && this.state === 'playing') {
        this.tickMs -= this.stepMs;
        this.step();
      }
    }

    step() {
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
      this.food = free[Math.floor(this.rng() * free.length) % free.length];
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
      ctx.fillStyle = 'rgba(12, 18, 24, 0.18)';
      ctx.fillRect(0, 0, grid.width * cell, grid.height * cell);

      ctx.strokeStyle = 'rgba(255,255,255,0.08)';
      ctx.lineWidth = Math.max(1, Math.floor(cell / 14));
      for (let x = 0; x <= grid.width; x++) {
        ctx.beginPath();
        ctx.moveTo(x * cell, 0);
        ctx.lineTo(x * cell, grid.height * cell);
        ctx.stroke();
      }
      for (let y = 0; y <= grid.height; y++) {
        ctx.beginPath();
        ctx.moveTo(0, y * cell);
        ctx.lineTo(grid.width * cell, y * cell);
        ctx.stroke();
      }

      drawFood(ctx, this.food, cell, this.config.theme.food);
      for (let i = this.snake.length - 1; i >= 0; i--) {
        drawSnakePart(ctx, this.snake[i], cell, i === 0, this.config.theme);
      }
      ctx.restore();
    }
  }

  function drawSnakePart(ctx, p, cell, head, theme) {
    const pad = Math.max(2, Math.floor(cell * 0.13));
    const x = p.x * cell + pad;
    const y = p.y * cell + pad;
    const s = cell - pad * 2;
    ctx.fillStyle = head ? '#ffd166' : theme.body === 'dot' ? '#70d6ff' : '#b8f2e6';
    ctx.fillRect(x, y, s, s);
    ctx.fillStyle = 'rgba(20,20,24,0.88)';
    if (head) {
      const eye = Math.max(2, Math.floor(cell * 0.12));
      ctx.fillRect(x + s * 0.25, y + s * 0.25, eye, eye);
      ctx.fillRect(x + s * 0.65, y + s * 0.25, eye, eye);
    }
  }

  function drawFood(ctx, p, cell, kind) {
    const pad = Math.max(3, Math.floor(cell * 0.2));
    const x = p.x * cell + pad;
    const y = p.y * cell + pad;
    const s = cell - pad * 2;
    ctx.fillStyle = kind === 'fish' ? '#8ecae6' : kind === 'butterfly' ? '#ffafcc' : '#ef476f';
    ctx.fillRect(x, y, s, s);
    ctx.fillStyle = 'rgba(255,255,255,0.72)';
    ctx.fillRect(x + Math.floor(s * 0.55), y + Math.floor(s * 0.22), Math.max(2, s / 6), Math.max(2, s / 6));
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
      overlayText.textContent = '方向键 / WASD 开始';
    } else if (kind === 'paused') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = '暂停';
      overlayText.textContent = 'P 或 Start 继续';
    } else if (kind === 'win') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = engine.config.dialogue.win;
      overlayText.textContent = `得分 ${engine.score} · Enter / A 再来一次 · Esc / B 退出`;
      overlayActions.classList.remove('hidden');
    } else if (kind === 'lose') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = engine.config.dialogue.lose;
      overlayText.textContent = `得分 ${engine.score} · Enter / A 再来一次 · Esc / B 退出`;
      overlayActions.classList.remove('hidden');
    } else if (kind === 'cancel') {
      overlay.classList.remove('hidden');
      overlayTitle.textContent = '已退出';
      overlayText.textContent = `得分 ${engine.score}`;
    } else {
      overlay.classList.add('hidden');
    }
  }

  function restartGame() {
    if (!currentConfig) return;
    engine = new SnakeEngine(currentConfig);
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
    lengthEl.textContent = String(engine.snake.length);
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
        log(`state changed ${lastLoggedState || '<none>'} -> ${state} score=${engine.score} len=${engine.snake.length}`);
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
    switch (e.key) {
      case 'ArrowUp':
      case 'w':
      case 'W':
        return { type: 'direction', dx: 0, dy: -1 };
      case 'ArrowDown':
      case 's':
      case 'S':
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
    window.addEventListener('resize', resizeCanvas);
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
      title: '毛线球大作战',
      grid: { width: 30, height: 20, cell_size: 24 },
      player: { speed_ms: 140, initial_length: 3 },
      rules: { walls_kill: true, self_kill: true, food_count: 1, speed_ramp: 0.95, win_length: 20 },
      theme: { head: 'cat', body: 'yarn', food: 'mouse', trail_alpha: 0.55 },
      dialogue: { start: '喵！看我的！', win: '太厉害了喵~', lose: '呜...再来一次！' },
    };
    currentConfig = config;
    engine = new SnakeEngine(config);
    await initEvents();
    log(`engine ready title=${config.title}`);
    requestAnimationFrame(loop);
  }

  window.GameEngineTest = { SnakeEngine, createRng };
  init();
})();
