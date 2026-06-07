(function () {
  'use strict';

  const COLORS = {
    bg: '#101318',
    grid: 'rgba(255,255,255,0.06)',
    player: '#8ecae6',
    playerCore: '#e0fbfc',
    enemy: '#ff6b6b',
    target: '#ffd166',
    lost: '#5d6470',
    text: '#f6f7f9',
    dim: 'rgba(246,247,249,0.62)',
  };

  function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
  }

  function fallbackProjection() {
    return {
      version: 1,
      items: [
        { id: 'fallback-0', kind: 'treat', title: 'energy treat', weight: 2 },
        { id: 'fallback-1', kind: 'memory_shard', title: 'memory shard', weight: 3 },
        { id: 'fallback-2', kind: 'reminder_note', title: 'reminder note', weight: 2 },
        { id: 'fallback-3', kind: 'agent_task', title: 'agent task', weight: 3 },
        { id: 'fallback-4', kind: 'treat', title: 'focus snack', weight: 1 },
      ],
    };
  }

  function labelFor(kind) {
    switch (kind) {
      case 'memory_shard':
        return 'M';
      case 'reminder_note':
        return 'R';
      case 'agent_task':
        return 'A';
      default:
        return 'T';
    }
  }

  function makeTarget(item, index, count, width, height) {
    const angle = (Math.PI * 2 * index) / Math.max(1, count) - Math.PI / 2;
    const radiusX = width * 0.28;
    const radiusY = height * 0.26;
    return {
      id: item.id || `target-${index}`,
      kind: item.kind || 'treat',
      title: String(item.title || 'target').slice(0, 24),
      weight: clamp(Number(item.weight || 1), 1, 5),
      x: width * 0.5 + Math.cos(angle) * radiusX,
      y: height * 0.5 + Math.sin(angle) * radiusY,
      r: 17 + clamp(Number(item.weight || 1), 1, 5) * 2,
      stolen: false,
      flashMs: 0,
    };
  }

  class InvasionEngine {
    constructor(config, deps = {}) {
      this.config = config;
      this.invoke = deps.invoke;
      this.log = deps.log || (() => {});
      this.restartGame = deps.restartGame || (() => {});
      this.closeEndedGame = deps.closeEndedGame || (() => {});
      this.state = 'ready';
      this.score = 0;
      this.combo = 0;
      this.ended = false;
      this.elapsedMs = 0;
      this.spawnMs = 900;
      this.enemySeq = 0;
      this.defeated = 0;
      this.stolen = 0;
      this.effects = [];
      this.enemies = [];
      this.input = { dx: 0, dy: 0 };
      this.player = { x: 0, y: 0, r: 18, speed: 0.22, flashMs: 0 };
      this.loadProjection(fallbackProjection());
      this.fetchProjection();
    }

    loadProjection(projection) {
      const items = Array.isArray(projection && projection.items) && projection.items.length
        ? projection.items.slice(0, this.config.rules.food_count || 5)
        : fallbackProjection().items;
      const width = this.config.grid.width;
      const height = this.config.grid.height;
      this.targets = items.map((item, index) => makeTarget(item, index, items.length, width, height));
      this.player.x = width / 2;
      this.player.y = height / 2;
    }

    async fetchProjection() {
      if (!this.invoke) return;
      try {
        const projection = await this.invoke('cmd_get_game_projection');
        this.loadProjection(projection);
        this.log(`invasion projection loaded items=${this.targets.length}`);
      } catch (error) {
        this.log(`cmd_get_game_projection failed: ${error}`);
      }
    }

    getState() {
      return this.state;
    }

    hudText() {
      const left = Math.max(0, this.targets.filter((target) => !target.stolen).length);
      const seconds = Math.max(0, Math.ceil((45000 - this.elapsedMs) / 1000));
      return `${left} guarded - ${this.defeated}/${this.config.rules.win_length} - ${seconds}s`;
    }

    handleInput(input) {
      if (!input) return;
      if (this.ended) {
        if (input.type === 'confirm' || input.type === 'attack_primary') this.restartGame();
        else if (input.type === 'cancel') this.closeEndedGame(this.state);
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
      if (input.type === 'confirm' || input.type === 'attack_primary') {
        if (this.state === 'ready') this.state = 'playing';
        this.guard();
        return;
      }
      if (input.type === 'direction') {
        if (this.state === 'ready') this.state = 'playing';
        this.input.dx = Math.sign(input.dx || 0);
        this.input.dy = Math.sign(input.dy || 0);
      }
    }

    handleKeyUp(key) {
      if (['ArrowLeft', 'ArrowRight', 'a', 'A', 'd', 'D'].includes(key)) this.input.dx = 0;
      if (['ArrowUp', 'ArrowDown', 'w', 'W', 's', 'S'].includes(key)) this.input.dy = 0;
    }

    handlePointer(x, y) {
      if (this.ended) return false;
      if (this.state === 'ready') this.state = 'playing';
      const metrics = this.lastMetrics;
      if (!metrics) return false;
      this.player.x = clamp((x - metrics.offsetX) / metrics.cell, 0, this.config.grid.width);
      this.player.y = clamp((y - metrics.offsetY) / metrics.cell, 0, this.config.grid.height);
      this.guard();
      return true;
    }

    update(dtMs) {
      for (const target of this.targets) target.flashMs = Math.max(0, target.flashMs - dtMs);
      for (const enemy of this.enemies) enemy.flashMs = Math.max(0, enemy.flashMs - dtMs);
      this.player.flashMs = Math.max(0, this.player.flashMs - dtMs);
      this.effects.forEach((effect) => {
        effect.age += dtMs;
        effect.y -= dtMs * 0.0018;
      });
      this.effects = this.effects.filter((effect) => effect.age < 720);

      if (this.state !== 'playing' || this.ended) return;
      this.elapsedMs += dtMs;
      const speed = this.player.speed * (dtMs / 16.67);
      this.player.x = clamp(this.player.x + this.input.dx * speed, 0.6, this.config.grid.width - 0.6);
      this.player.y = clamp(this.player.y + this.input.dy * speed, 0.6, this.config.grid.height - 0.6);

      this.spawnMs -= dtMs;
      if (this.spawnMs <= 0) {
        this.spawnEnemy();
        const ramp = Math.pow(this.config.rules.speed_ramp || 0.96, this.defeated / 3);
        this.spawnMs = Math.max(360, 1050 * ramp);
      }
      this.updateEnemies(dtMs);
      if (this.defeated >= this.config.rules.win_length || this.elapsedMs >= 45000) {
        this.finish(this.targets.some((target) => !target.stolen) ? 'win' : 'lose');
      }
    }

    spawnEnemy() {
      const liveTargets = this.targets.filter((target) => !target.stolen);
      if (!liveTargets.length) {
        this.finish('lose');
        return;
      }
      const edge = this.enemySeq % 4;
      const w = this.config.grid.width;
      const h = this.config.grid.height;
      const pos = [
        { x: 0, y: 1 + Math.random() * (h - 2) },
        { x: w, y: 1 + Math.random() * (h - 2) },
        { x: 1 + Math.random() * (w - 2), y: 0 },
        { x: 1 + Math.random() * (w - 2), y: h },
      ][edge];
      const target = liveTargets[this.enemySeq % liveTargets.length];
      this.enemies.push({
        id: `enemy-${this.enemySeq++}`,
        x: pos.x,
        y: pos.y,
        r: 13,
        speed: 0.0017 + Math.min(0.0012, this.defeated * 0.00005),
        targetId: target.id,
        flashMs: 0,
      });
    }

    updateEnemies(dtMs) {
      const remaining = [];
      for (const enemy of this.enemies) {
        const target = this.targets.find((item) => item.id === enemy.targetId && !item.stolen)
          || this.targets.find((item) => !item.stolen);
        if (!target) {
          this.finish('lose');
          return;
        }
        const dx = target.x - enemy.x;
        const dy = target.y - enemy.y;
        const dist = Math.max(0.001, Math.hypot(dx, dy));
        enemy.x += (dx / dist) * enemy.speed * dtMs;
        enemy.y += (dy / dist) * enemy.speed * dtMs;
        if (dist < 0.45) {
          target.stolen = true;
          target.flashMs = 500;
          this.stolen += 1;
          this.combo = 0;
          this.addEffect('taken', target.x, target.y, COLORS.enemy);
          if (this.stolen >= Math.max(3, Math.ceil(this.targets.length / 2))) this.finish('lose');
        } else {
          remaining.push(enemy);
        }
      }
      this.enemies = remaining;
    }

    guard() {
      if (this.state !== 'playing') return;
      let best = null;
      let bestDist = Infinity;
      for (const enemy of this.enemies) {
        const dist = Math.hypot(enemy.x - this.player.x, enemy.y - this.player.y);
        if (dist < bestDist) {
          best = enemy;
          bestDist = dist;
        }
      }
      if (!best || bestDist > 1.65) {
        this.player.flashMs = 180;
        this.addEffect('guard', this.player.x, this.player.y, COLORS.dim);
        return;
      }
      this.enemies = this.enemies.filter((enemy) => enemy !== best);
      this.combo += 1;
      this.defeated += 1;
      const gain = 10 + Math.min(30, this.combo * 3);
      this.score += gain;
      this.player.flashMs = 220;
      this.addEffect(`+${gain}${this.combo > 1 ? ` x${this.combo}` : ''}`, best.x, best.y, COLORS.target);
      if (this.defeated >= this.config.rules.win_length) this.finish('win');
    }

    addEffect(text, x, y, color) {
      this.effects.push({ text, x, y, color, age: 0 });
    }

    finish(result) {
      if (this.ended) return;
      this.ended = true;
      this.state = result;
      this.input.dx = 0;
      this.input.dy = 0;
    }

    render(ctx, metrics) {
      this.lastMetrics = metrics;
      const w = this.config.grid.width;
      const h = this.config.grid.height;
      const toPx = (x, y) => ({
        x: metrics.offsetX + x * metrics.cell,
        y: metrics.offsetY + y * metrics.cell,
      });
      ctx.save();
      ctx.fillStyle = COLORS.bg;
      ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
      ctx.translate(metrics.offsetX, metrics.offsetY);
      ctx.strokeStyle = COLORS.grid;
      ctx.lineWidth = 1;
      for (let x = 0; x <= w; x += 2) {
        ctx.beginPath();
        ctx.moveTo(x * metrics.cell, 0);
        ctx.lineTo(x * metrics.cell, h * metrics.cell);
        ctx.stroke();
      }
      for (let y = 0; y <= h; y += 2) {
        ctx.beginPath();
        ctx.moveTo(0, y * metrics.cell);
        ctx.lineTo(w * metrics.cell, y * metrics.cell);
        ctx.stroke();
      }
      ctx.restore();

      for (const target of this.targets) this.renderTarget(ctx, target, toPx, metrics.cell);
      for (const enemy of this.enemies) this.renderEnemy(ctx, enemy, toPx, metrics.cell);
      this.renderPlayer(ctx, toPx, metrics.cell);
      this.renderEffects(ctx, toPx, metrics.cell);
    }

    renderTarget(ctx, target, toPx, cell) {
      const p = toPx(target.x, target.y);
      ctx.save();
      ctx.globalAlpha = target.stolen ? 0.38 : 1;
      ctx.fillStyle = target.stolen ? COLORS.lost : COLORS.target;
      ctx.beginPath();
      ctx.roundRect(p.x - target.r, p.y - target.r, target.r * 2, target.r * 2, 6);
      ctx.fill();
      ctx.fillStyle = COLORS.bg;
      ctx.font = '700 14px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(labelFor(target.kind), p.x, p.y - 2);
      ctx.fillStyle = COLORS.text;
      ctx.font = '600 10px system-ui, sans-serif';
      ctx.fillText(target.title, p.x, p.y + target.r + 12);
      if (target.flashMs > 0) {
        ctx.strokeStyle = COLORS.enemy;
        ctx.lineWidth = 3;
        ctx.strokeRect(p.x - target.r - 3, p.y - target.r - 3, target.r * 2 + 6, target.r * 2 + 6);
      }
      ctx.restore();
    }

    renderEnemy(ctx, enemy, toPx, cell) {
      const p = toPx(enemy.x, enemy.y);
      ctx.save();
      ctx.fillStyle = COLORS.enemy;
      ctx.beginPath();
      ctx.arc(p.x, p.y, enemy.r, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = 'rgba(0,0,0,0.35)';
      ctx.fillRect(p.x - 5, p.y - 2, 10, 4);
      ctx.restore();
    }

    renderPlayer(ctx, toPx, cell) {
      const p = toPx(this.player.x, this.player.y);
      ctx.save();
      ctx.fillStyle = this.player.flashMs > 0 ? COLORS.target : COLORS.player;
      ctx.beginPath();
      ctx.arc(p.x, p.y, this.player.r, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = COLORS.playerCore;
      ctx.beginPath();
      ctx.arc(p.x, p.y, this.player.r * 0.45, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }

    renderEffects(ctx, toPx, cell) {
      ctx.save();
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.font = '700 14px system-ui, sans-serif';
      for (const effect of this.effects) {
        const p = toPx(effect.x, effect.y);
        ctx.globalAlpha = 1 - effect.age / 720;
        ctx.fillStyle = effect.color;
        ctx.fillText(effect.text, p.x, p.y);
      }
      ctx.restore();
    }
  }

  window.BitCatGames = window.BitCatGames || {};
  window.BitCatGames.invasion = function createInvasionEngine(config, deps) {
    return new InvasionEngine(config, deps);
  };
  window.BitCatInvasionTest = { InvasionEngine, fallbackProjection };
})();
