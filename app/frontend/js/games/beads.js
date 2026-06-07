(function () {
  window.BitCatGames = window.BitCatGames || {};

  const EMPTY = -1;
  const PALETTE = [
    { id: 'cream', name: 'Cream', color: '#f8f0cf' },
    { id: 'charcoal', name: 'Ink', color: '#252433' },
    { id: 'pink', name: 'Pink', color: '#ff8fb3' },
    { id: 'coral', name: 'Coral', color: '#ff6f61' },
    { id: 'gold', name: 'Gold', color: '#ffd166' },
    { id: 'mint', name: 'Mint', color: '#84dcc6' },
    { id: 'blue', name: 'Blue', color: '#4dabf7' },
    { id: 'violet', name: 'Violet', color: '#9d7cff' },
  ];

  const PATTERNS = [
    {
      name: 'BitCat',
      rows: [
        '................',
        '....11....11....',
        '...1001..1001...',
        '..100001100001..',
        '..100000000001..',
        '.10020000002001.',
        '.10000000000001.',
        '.10003033030001.',
        '.10000033000001.',
        '..100000000001..',
        '..110400004011..',
        '....10000001....',
        '.....111111.....',
        '................',
        '................',
        '................',
      ],
    },
    {
      name: 'Paw Pop',
      rows: [
        '................',
        '................',
        '....22....22....',
        '...2222..2222...',
        '...2222..2222...',
        '................',
        '..22........22..',
        '.2222......2222.',
        '.2222..22..2222.',
        '......2222......',
        '.....222222.....',
        '....22222222....',
        '....22222222....',
        '.....222222.....',
        '................',
        '................',
      ],
    },
    {
      name: 'Star Jar',
      rows: [
        '................',
        '.......4........',
        '.......4........',
        '.....44444......',
        '......444.......',
        '.....44444......',
        '....4..4..4.....',
        '................',
        '...66666666.....',
        '..6555555566....',
        '..6555555566....',
        '..6555555566....',
        '..6555555566....',
        '...66666666.....',
        '................',
        '................',
      ],
    },
  ];

  function clamp(n, min, max) {
    return Math.max(min, Math.min(max, n));
  }

  function decodePattern(pattern, width, height) {
    const rows = [];
    for (let y = 0; y < height; y++) {
      const source = pattern.rows[y] || '';
      const row = [];
      for (let x = 0; x < width; x++) {
        const ch = source[x] || '.';
        row.push(ch === '.' ? EMPTY : clamp(Number(ch), 0, PALETTE.length - 1));
      }
      rows.push(row);
    }
    return rows;
  }

  function makeBoard(width, height) {
    return Array.from({ length: height }, () => Array(width).fill(EMPTY));
  }

  class BeadsEngine {
    constructor(config, host) {
      this.config = config;
      this.host = host || {};
      this.state = 'ready';
      this.score = 0;
      this.ended = false;
      this.cursor = { x: 0, y: 0 };
      this.paletteIndex = 2;
      this.patternIndex = 0;
      this.target = decodePattern(PATTERNS[0], config.grid.width, config.grid.height);
      this.board = makeBoard(config.grid.width, config.grid.height);
      this.history = [];
      this.effects = [];
      this.animMs = 0;
      this.lastCompletion = 0;
      this.mistakes = 0;
      this.placed = 0;
    }

    getState() {
      return this.state;
    }

    readyText() {
      return '方向键移动，Enter / A 放豆，Tab / X 换色，Backspace 撤销';
    }

    endText(result) {
      if (result === 'win') {
        return `完成 ${PATTERNS[this.patternIndex].name}，得分 ${this.score}。Enter 再拼一次，Esc 退出`;
      }
      return `完成度 ${Math.round(this.lastCompletion * 100)}%，得分 ${this.score}`;
    }

    hudText() {
      const completion = Math.round(this.completion().ratio * 100);
      return `${PATTERNS[this.patternIndex].name} · ${completion}% · ${PALETTE[this.paletteIndex].name}`;
    }

    handleInput(input) {
      if (!input) return;
      if (this.ended) {
        if (input.type === 'confirm') this.host.restartGame && this.host.restartGame();
        else if (input.type === 'cancel') this.host.closeEndedGame && this.host.closeEndedGame(this.state);
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
      if (input.type === 'direction') {
        if (this.state === 'ready') this.state = 'playing';
        this.cursor.x = clamp(this.cursor.x + Math.sign(input.dx || 0), 0, this.config.grid.width - 1);
        this.cursor.y = clamp(this.cursor.y + Math.sign(input.dy || 0), 0, this.config.grid.height - 1);
        return;
      }
      if (input.type === 'confirm') {
        if (this.state === 'ready') this.state = 'playing';
        this.placeSelected();
        return;
      }
      if (input.type === 'cycle') {
        if (this.state === 'ready') this.state = 'playing';
        const dir = Math.sign(input.dir || 1) || 1;
        this.paletteIndex = (this.paletteIndex + dir + PALETTE.length) % PALETTE.length;
        this.addEffect(PALETTE[this.paletteIndex].name, this.cursor.x, this.cursor.y, PALETTE[this.paletteIndex].color);
        return;
      }
      if (input.type === 'undo') {
        this.undo();
      }
    }

    handleKey(key) {
      if (this.ended) return false;
      if (key === 'Tab' || key === 'x' || key === 'X') {
        if (this.state === 'ready') this.state = 'playing';
        this.paletteIndex = (this.paletteIndex + 1) % PALETTE.length;
        this.addEffect(PALETTE[this.paletteIndex].name, this.cursor.x, this.cursor.y, PALETTE[this.paletteIndex].color);
        return true;
      }
      if (key === 'z' || key === 'Z' || key === 'Backspace') {
        this.undo();
        return true;
      }
      if (key === 'Delete' || key === 'e' || key === 'E') {
        this.erase();
        return true;
      }
      if (key >= '1' && key <= String(PALETTE.length)) {
        this.paletteIndex = Number(key) - 1;
        return true;
      }
      return false;
    }

    handlePointer(x, y) {
      if (this.state === 'ready') this.state = 'playing';
      const metrics = this.lastMetrics;
      if (!metrics) return true;
      const cell = this.cellFromPixel(x, y, metrics);
      if (cell) {
        this.cursor = cell;
        this.placeSelected();
        return true;
      }
      const palette = this.paletteFromPixel(x, y, metrics);
      if (palette !== null) {
        this.paletteIndex = palette;
        return true;
      }
      const pattern = this.patternFromPixel(x, y, metrics);
      if (pattern !== null) {
        this.loadPattern(pattern);
        return true;
      }
      return true;
    }

    update(dtMs) {
      this.animMs += dtMs;
      for (const effect of this.effects) {
        effect.age += dtMs;
        effect.y -= dtMs * 0.0012;
      }
      this.effects = this.effects.filter((effect) => effect.age < 780);
    }

    render(ctx, metrics) {
      this.lastMetrics = metrics;
      ctx.clearRect(0, 0, metrics.width, metrics.height);
      drawBackground(ctx, metrics);
      const layout = this.layout(metrics);
      drawTargetPreview(ctx, layout.preview, this.target);
      drawBoard(ctx, layout.board, this);
      drawPalette(ctx, layout.palette, this.paletteIndex);
      drawPatternTabs(ctx, layout.patterns, this.patternIndex);
      drawBeadEffects(ctx, layout.board, this.effects, this.config.grid);
    }

    placeSelected() {
      if (this.state !== 'playing') return;
      const old = this.board[this.cursor.y][this.cursor.x];
      if (old === this.paletteIndex) return;
      this.history.push({ x: this.cursor.x, y: this.cursor.y, old });
      this.history = this.history.slice(-80);
      this.board[this.cursor.y][this.cursor.x] = this.paletteIndex;
      this.placed += old === EMPTY ? 1 : 0;
      const target = this.target[this.cursor.y][this.cursor.x];
      const correct = target === this.paletteIndex;
      if (!correct) this.mistakes += 1;
      this.addEffect(correct ? '+ bead' : 'swap', this.cursor.x, this.cursor.y, correct ? '#95d5b2' : '#ffd166');
      this.updateScore();
      if (this.completion().ratio >= 1) this.finish('win');
    }

    erase() {
      const old = this.board[this.cursor.y][this.cursor.x];
      if (old === EMPTY) return;
      this.history.push({ x: this.cursor.x, y: this.cursor.y, old });
      this.board[this.cursor.y][this.cursor.x] = EMPTY;
      this.updateScore();
      this.addEffect('erase', this.cursor.x, this.cursor.y, '#f8f0cf');
    }

    undo() {
      const last = this.history.pop();
      if (!last) return;
      this.board[last.y][last.x] = last.old;
      this.cursor = { x: last.x, y: last.y };
      this.updateScore();
      this.addEffect('undo', last.x, last.y, '#8ecae6');
    }

    loadPattern(index) {
      this.patternIndex = index;
      this.target = decodePattern(PATTERNS[index], this.config.grid.width, this.config.grid.height);
      this.board = makeBoard(this.config.grid.width, this.config.grid.height);
      this.history = [];
      this.effects = [];
      this.score = 0;
      this.ended = false;
      this.state = 'playing';
      this.mistakes = 0;
      this.placed = 0;
    }

    completion() {
      let required = 0;
      let correct = 0;
      let wrong = 0;
      for (let y = 0; y < this.config.grid.height; y++) {
        for (let x = 0; x < this.config.grid.width; x++) {
          const target = this.target[y][x];
          const bead = this.board[y][x];
          if (target !== EMPTY) {
            required += 1;
            if (bead === target) correct += 1;
            else if (bead !== EMPTY) wrong += 1;
          } else if (bead !== EMPTY) {
            wrong += 1;
          }
        }
      }
      return { required, correct, wrong, ratio: required ? correct / required : 1 };
    }

    updateScore() {
      const progress = this.completion();
      this.lastCompletion = progress.ratio;
      this.score = Math.max(0, Math.round(progress.correct * 12 + progress.ratio * 80 - progress.wrong * 5));
    }

    finish(result) {
      if (this.ended) return;
      this.updateScore();
      if (result === 'win') {
        const progress = this.completion();
        this.score += Math.max(0, 120 - progress.wrong * 4 - Math.max(0, this.history.length - progress.required) * 2);
      }
      this.ended = true;
      this.state = result;
    }

    addEffect(text, x, y, color) {
      this.effects.push({ text, x, y, color, age: 0 });
    }

    layout(metrics) {
      const grid = this.config.grid;
      const availableW = metrics.width - 112;
      const availableH = metrics.height - 88;
      const cell = Math.floor(Math.min(availableW / (grid.width + 9), availableH / grid.height, 44));
      const boardW = grid.width * cell;
      const boardH = grid.height * cell;
      const railW = Math.max(224, Math.floor(metrics.width * 0.19));
      const gap = Math.max(22, Math.floor(cell * 0.8));
      const boardX = Math.round((metrics.width - boardW - railW - gap - cell * 4.1) / 2 + cell * 2.1);
      const boardY = Math.round((metrics.height - boardH) / 2 + 10);
      const rightX = boardX + boardW + gap;
      const patternPanelH = 38 + PATTERNS.length * 40 + 14;
      const paletteRows = Math.ceil(PALETTE.length / 2);
      const paletteCell = Math.max(28, Math.floor(cell * 0.72));
      const palettePanelH = 38 + paletteRows * (paletteCell + 12) + 18;
      const palettePanelY = boardY + patternPanelH + 22;
      return {
        board: { x: boardX, y: boardY, cell, w: boardW, h: boardH },
        preview: {
          x: Math.max(22, boardX - cell * 4.2),
          y: boardY,
          cell: Math.max(5, Math.floor(cell / 4)),
          w: cell * 3.7,
        },
        patterns: {
          x: rightX,
          y: boardY,
          w: railW,
          h: patternPanelH,
        },
        palette: {
          x: rightX,
          y: palettePanelY,
          w: railW,
          h: palettePanelH,
          cell: paletteCell,
        },
      };
    }

    cellFromPixel(x, y, metrics) {
      const board = this.layout(metrics).board;
      if (x < board.x || y < board.y || x >= board.x + board.w || y >= board.y + board.h) return null;
      return {
        x: clamp(Math.floor((x - board.x) / board.cell), 0, this.config.grid.width - 1),
        y: clamp(Math.floor((y - board.y) / board.cell), 0, this.config.grid.height - 1),
      };
    }

    paletteFromPixel(x, y, metrics) {
      const p = this.layout(metrics).palette;
      for (let i = 0; i < PALETTE.length; i++) {
        const px = p.x + (i % 2) * (p.cell + 12);
        const py = p.y + Math.floor(i / 2) * (p.cell + 12);
        if (x >= px && y >= py && x <= px + p.cell && y <= py + p.cell) return i;
      }
      return null;
    }

    patternFromPixel(x, y, metrics) {
      const tabs = this.layout(metrics).patterns;
      for (let i = 0; i < PATTERNS.length; i++) {
        const py = tabs.y + 38 + i * 40;
        if (x >= tabs.x + 12 && y >= py && x <= tabs.x + tabs.w - 12 && y <= py + 32) return i;
      }
      return null;
    }
  }

  function drawBackground(ctx, metrics) {
    const g = ctx.createLinearGradient(0, 0, metrics.width, metrics.height);
    g.addColorStop(0, 'rgba(10, 17, 24, 0.72)');
    g.addColorStop(0.62, 'rgba(20, 27, 32, 0.58)');
    g.addColorStop(1, 'rgba(9, 12, 18, 0.72)');
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, metrics.width, metrics.height);
  }

  function drawPanel(ctx, x, y, w, h) {
    ctx.save();
    ctx.fillStyle = 'rgba(15, 19, 25, 0.72)';
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.16)';
    ctx.lineWidth = 1;
    ctx.roundRect(x, y, w, h, 8);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  }

  function drawBoard(ctx, rect, engine) {
    const grid = engine.config.grid;
    drawPanel(ctx, rect.x - 10, rect.y - 10, rect.w + 20, rect.h + 20);
    for (let y = 0; y < grid.height; y++) {
      for (let x = 0; x < grid.width; x++) {
        const target = engine.target[y][x];
        const bead = engine.board[y][x];
        const cx = rect.x + x * rect.cell + rect.cell / 2;
        const cy = rect.y + y * rect.cell + rect.cell / 2;
        ctx.fillStyle = target === EMPTY ? 'rgba(255,255,255,0.035)' : colorWithAlpha(PALETTE[target].color, 0.20);
        ctx.fillRect(rect.x + x * rect.cell + 1, rect.y + y * rect.cell + 1, rect.cell - 2, rect.cell - 2);
        if (bead !== EMPTY) {
          drawBead(ctx, cx, cy, rect.cell * 0.39, PALETTE[bead].color);
        } else if (target !== EMPTY) {
          ctx.beginPath();
          ctx.arc(cx, cy, rect.cell * 0.17, 0, Math.PI * 2);
          ctx.fillStyle = colorWithAlpha(PALETTE[target].color, 0.35);
          ctx.fill();
        }
      }
    }
    const pulse = 0.5 + Math.sin(engine.animMs / 120) * 0.5;
    ctx.strokeStyle = `rgba(255,255,255,${0.72 + pulse * 0.2})`;
    ctx.lineWidth = 3;
    ctx.roundRect(
      rect.x + engine.cursor.x * rect.cell + 3,
      rect.y + engine.cursor.y * rect.cell + 3,
      rect.cell - 6,
      rect.cell - 6,
      6
    );
    ctx.stroke();
  }

  function drawBead(ctx, x, y, r, color) {
    const g = ctx.createRadialGradient(x - r * 0.35, y - r * 0.38, r * 0.08, x, y, r);
    g.addColorStop(0, 'rgba(255,255,255,0.86)');
    g.addColorStop(0.24, color);
    g.addColorStop(1, shade(color, -0.34));
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fillStyle = g;
    ctx.fill();
    ctx.strokeStyle = 'rgba(0,0,0,0.20)';
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(x - r * 0.24, y - r * 0.28, r * 0.18, 0, Math.PI * 2);
    ctx.fillStyle = 'rgba(255,255,255,0.42)';
    ctx.fill();
  }

  function drawTargetPreview(ctx, rect, target) {
    const cell = rect.cell;
    drawPanel(ctx, rect.x - 10, rect.y - 10, rect.w + 20, cell * target.length + 42);
    ctx.fillStyle = '#f7fbff';
    ctx.font = '700 13px "Segoe UI", "Microsoft YaHei", sans-serif';
    ctx.fillText('Target', rect.x, rect.y - 20);
    for (let y = 0; y < target.length; y++) {
      for (let x = 0; x < target[y].length; x++) {
        const bead = target[y][x];
        if (bead === EMPTY) continue;
        ctx.fillStyle = PALETTE[bead].color;
        ctx.fillRect(rect.x + x * cell, rect.y + y * cell, cell + 0.5, cell + 0.5);
      }
    }
  }

  function drawPalette(ctx, rect, selected) {
    drawPanel(ctx, rect.x, rect.y, rect.w, rect.h);
    ctx.fillStyle = '#f7fbff';
    ctx.font = '700 13px "Segoe UI", "Microsoft YaHei", sans-serif';
    ctx.fillText('Colors', rect.x + 14, rect.y + 22);
    for (let i = 0; i < PALETTE.length; i++) {
      const x = rect.x + 14 + (i % 2) * (rect.cell + 12);
      const y = rect.y + 38 + Math.floor(i / 2) * (rect.cell + 12);
      drawBead(ctx, x + rect.cell / 2, y + rect.cell / 2, rect.cell * 0.42, PALETTE[i].color);
      if (i === selected) {
        ctx.strokeStyle = 'rgba(255,255,255,0.92)';
        ctx.lineWidth = 3;
        ctx.roundRect(x - 3, y - 3, rect.cell + 6, rect.cell + 6, 8);
        ctx.stroke();
      }
    }
  }

  function drawPatternTabs(ctx, rect, selected) {
    drawPanel(ctx, rect.x, rect.y, rect.w, rect.h);
    ctx.fillStyle = '#f7fbff';
    ctx.font = '700 13px "Segoe UI", "Microsoft YaHei", sans-serif';
    ctx.fillText('Patterns', rect.x + 14, rect.y + 22);
    ctx.font = '700 12px "Segoe UI", "Microsoft YaHei", sans-serif';
    for (let i = 0; i < PATTERNS.length; i++) {
      const y = rect.y + 38 + i * 40;
      ctx.fillStyle = i === selected ? 'rgba(255, 209, 102, 0.30)' : 'rgba(255,255,255,0.08)';
      ctx.strokeStyle = i === selected ? 'rgba(255, 209, 102, 0.78)' : 'rgba(255,255,255,0.14)';
      ctx.roundRect(rect.x + 12, y, rect.w - 24, 32, 7);
      ctx.fill();
      ctx.stroke();
      ctx.fillStyle = '#f7fbff';
      ctx.fillText(PATTERNS[i].name, rect.x + 24, y + 20);
    }
  }

  function drawBeadEffects(ctx, board, effects, grid) {
    ctx.save();
    ctx.font = '800 12px "Segoe UI", "Microsoft YaHei", sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (const effect of effects) {
      const alpha = clamp(1 - effect.age / 780, 0, 1);
      ctx.fillStyle = colorWithAlpha(effect.color, alpha);
      ctx.fillText(
        effect.text,
        board.x + clamp(effect.x, 0, grid.width - 1) * board.cell + board.cell / 2,
        board.y + clamp(effect.y, 0, grid.height - 1) * board.cell + board.cell / 2 - effect.age * 0.035
      );
    }
    ctx.restore();
  }

  function colorWithAlpha(hex, alpha) {
    const rgb = hexToRgb(hex);
    return `rgba(${rgb.r},${rgb.g},${rgb.b},${alpha})`;
  }

  function shade(hex, amount) {
    const rgb = hexToRgb(hex);
    const scale = 1 + amount;
    const r = clamp(Math.round(rgb.r * scale), 0, 255).toString(16).padStart(2, '0');
    const g = clamp(Math.round(rgb.g * scale), 0, 255).toString(16).padStart(2, '0');
    const b = clamp(Math.round(rgb.b * scale), 0, 255).toString(16).padStart(2, '0');
    return `#${r}${g}${b}`;
  }

  function hexToRgb(hex) {
    const clean = hex.replace('#', '');
    return {
      r: parseInt(clean.slice(0, 2), 16),
      g: parseInt(clean.slice(2, 4), 16),
      b: parseInt(clean.slice(4, 6), 16),
    };
  }

  window.BitCatGames.beads = function createBeadsEngine(config, host) {
    return new BeadsEngine(config, host);
  };

  window.BitCatBeadsTest = { BeadsEngine, decodePattern, PATTERNS, PALETTE };
})();
