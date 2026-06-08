(function () {
  'use strict';

  const COLORS = {
    bg: '#0d1117',
    deskGlow: 'rgba(142, 202, 230, 0.10)',
    deskWarm: 'rgba(255, 209, 102, 0.045)',
    player: '#8ecae6',
    playerCore: '#f7fbff',
    playerNose: '#ffb7c8',
    playerScarf: '#ffe08a',
    playerShadow: 'rgba(56, 189, 248, 0.22)',
    enemy: '#ff6b6b',
    enemyHot: '#ffb86b',
    enemyFast: '#ff8fb3',
    enemyHeavy: '#e26dff',
    target: '#ffd166',
    memory: '#9bdcff',
    reminder: '#ffd166',
    agent: '#b7a8ff',
    treat: '#7ee0a1',
    targetInk: '#1d2430',
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

  function targetProfile(kind, weight) {
    const base = clamp(Number(weight || 1), 1, 5);
    switch (kind) {
      case 'memory_shard':
        return { value: 18 + base * 3, effect: 'focus', label: 'cooldown' };
      case 'reminder_note':
        return { value: 14 + base * 2, effect: 'alarm', label: 'alarm' };
      case 'agent_task':
        return { value: 24 + base * 4, effect: 'priority', label: 'priority' };
      default:
        return { value: 10 + base * 2, effect: 'snack', label: 'speed' };
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
      danger: 0,
      savedMs: 0,
      stolen: false,
      flashMs: 0,
      phase: index * 0.83,
      ...targetProfile(item.kind || 'treat', item.weight || 1),
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
      this.spawnMs = 520;
      this.enemySeq = 0;
      this.defeated = 0;
      this.stolen = 0;
      this.maxCombo = 0;
      this.guardedTargets = new Set();
      this.effects = [];
      this.enemies = [];
      this.trails = [];
      this.input = { dx: 0, dy: 0 };
      this.pulseCooldownMs = 0;
      this.sprintCooldownMs = 0;
      this.sprintMs = 0;
      this.wave = 1;
      this.waveNoticeMs = 0;
      this.lastWave = 1;
      this.clutchSaves = 0;
      this.lostValue = 0;
      this.savedValue = 0;
      this.savedTargets = new Set();
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

    readyText() {
      return 'A / Enter 守护，X / K 范围脉冲，Y / L 冲刺';
    }

    hudText() {
      const left = Math.max(0, this.targets.filter((target) => !target.stolen).length);
      const seconds = Math.max(0, Math.ceil((45000 - this.elapsedMs) / 1000));
      const combo = this.combo > 1 ? ` - x${this.combo}` : '';
      const pulse = this.pulseCooldownMs <= 0 ? 'pulse ready' : `pulse ${Math.ceil(this.pulseCooldownMs / 1000)}s`;
      return `wave ${this.wave}/4 - ${left} safe - ${this.defeated}/${this.config.rules.win_length}${combo} - ${seconds}s - ${pulse}`;
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
      if (input.type === 'skill') {
        if (this.state === 'ready') this.state = 'playing';
        this.useSkill(input.slot);
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

    handleKey(key) {
      switch (key) {
        case 'k':
        case 'K':
        case 'x':
        case 'X':
          this.handleInput({ type: 'skill', slot: 1 });
          return true;
        case 'l':
        case 'L':
        case 'y':
        case 'Y':
        case 'Shift':
          this.handleInput({ type: 'skill', slot: 2 });
          return true;
        default:
          return false;
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
      for (const target of this.targets) target.savedMs = Math.max(0, (target.savedMs || 0) - dtMs);
      for (const enemy of this.enemies) enemy.flashMs = Math.max(0, enemy.flashMs - dtMs);
      this.player.flashMs = Math.max(0, this.player.flashMs - dtMs);
      this.waveNoticeMs = Math.max(0, this.waveNoticeMs - dtMs);
      this.pulseCooldownMs = Math.max(0, this.pulseCooldownMs - dtMs);
      this.sprintCooldownMs = Math.max(0, this.sprintCooldownMs - dtMs);
      this.sprintMs = Math.max(0, this.sprintMs - dtMs);
      this.effects.forEach((effect) => {
        effect.age += dtMs;
        effect.y -= dtMs * 0.0018;
      });
      this.effects = this.effects.filter((effect) => effect.age < 720);
      this.trails.forEach((trail) => {
        trail.age += dtMs;
        trail.r += dtMs * 0.012;
      });
      this.trails = this.trails.filter((trail) => trail.age < 420);

      if (this.state !== 'playing' || this.ended) return;
      this.elapsedMs += dtMs;
      this.wave = clamp(1 + Math.floor(this.elapsedMs / 11000), 1, 4);
      if (this.wave !== this.lastWave) {
        this.lastWave = this.wave;
        this.waveNoticeMs = 1500;
        this.addEffect(`wave ${this.wave}`, this.player.x, this.player.y - 1.4, COLORS.playerScarf);
      }
      const speedBoost = this.sprintMs > 0 ? 1.65 : 1;
      const speed = this.player.speed * speedBoost * (dtMs / 16.67);
      this.player.x = clamp(this.player.x + this.input.dx * speed, 0.6, this.config.grid.width - 0.6);
      this.player.y = clamp(this.player.y + this.input.dy * speed, 0.6, this.config.grid.height - 0.6);
      if (Math.abs(this.input.dx) + Math.abs(this.input.dy) > 0 || this.sprintMs > 0) {
        this.trails.push({
          x: this.player.x,
          y: this.player.y,
          r: this.sprintMs > 0 ? 0.72 : 0.42,
          age: 0,
          color: this.sprintMs > 0 ? COLORS.playerScarf : COLORS.player,
        });
      }

      this.spawnMs -= dtMs;
      if (this.spawnMs <= 0) {
        this.spawnWave();
        const ramp = Math.pow(this.config.rules.speed_ramp || 0.96, this.defeated / 3 + this.wave * 0.75);
        this.spawnMs = Math.max(310, 1180 * ramp);
      }
      this.updateEnemies(dtMs);
      this.updateTargetDanger();
      this.handleSprintCollisions();
      if (this.defeated >= this.config.rules.win_length || this.elapsedMs >= 45000) {
        this.finish(this.targets.some((target) => !target.stolen) ? 'win' : 'lose');
      }
    }

    spawnWave() {
      const count = this.wave >= 4 ? 2 : 1;
      for (let i = 0; i < count; i += 1) this.spawnEnemy();
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
      const target = this.pickTarget(liveTargets);
      const variant = this.pickEnemyVariant();
      this.enemies.push({
        id: `enemy-${this.enemySeq++}`,
        variant,
        x: pos.x,
        y: pos.y,
        r: variant === 'brute' ? 16 : variant === 'skitter' ? 10 : 13,
        hp: variant === 'brute' ? 2 : 1,
        speed: this.enemySpeedFor(variant),
        targetId: target.id,
        flashMs: 0,
        warnMs: 420,
      });
      target.flashMs = Math.max(target.flashMs, 260);
      this.log(`invasion enemy spawned target=${target.kind}:${target.title} live=${this.enemies.length}`);
    }

    pickTarget(liveTargets) {
      const sorted = liveTargets.slice().sort((a, b) => {
        const priorityA = a.effect === 'priority' ? 9 : 0;
        const priorityB = b.effect === 'priority' ? 9 : 0;
        return (b.value + priorityB + b.danger * 12) - (a.value + priorityA + a.danger * 12);
      });
      const offset = this.enemySeq % Math.min(3, sorted.length);
      return sorted[offset] || liveTargets[0];
    }

    pickEnemyVariant() {
      if (this.enemySeq > 0 && this.enemySeq % 7 === 0) return 'brute';
      if (this.enemySeq > 2 && this.enemySeq % 3 === 0) return 'skitter';
      return 'crawler';
    }

    enemySpeedFor(variant) {
      const ramp = Math.min(0.001, this.defeated * 0.00004 + this.wave * 0.00008);
      if (variant === 'skitter') return 0.00245 + ramp;
      if (variant === 'brute') return 0.00115 + ramp * 0.65;
      return 0.00172 + ramp;
    }

    updateEnemies(dtMs) {
      const remaining = [];
      for (const enemy of this.enemies) {
        enemy.warnMs = Math.max(0, (enemy.warnMs || 0) - dtMs);
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
          this.lostValue += target.value || 0;
          this.combo = 0;
          this.addEffect('taken', target.x, target.y, COLORS.enemy);
          this.log(`invasion target stolen kind=${target.kind} title=${target.title} stolen=${this.stolen}`);
          if (target.effect === 'alarm') {
            this.spawnEnemy();
            this.addEffect('alarm', target.x, target.y - 0.8, COLORS.enemyHot);
          }
          if (this.stolen >= Math.max(3, Math.ceil(this.targets.length / 2))) this.finish('lose');
        } else {
          remaining.push(enemy);
        }
      }
      this.enemies = remaining;
    }

    updateTargetDanger() {
      for (const target of this.targets) {
        if (target.stolen) {
          target.danger = 0;
          continue;
        }
        let danger = 0;
        for (const enemy of this.enemies) {
          if (enemy.targetId !== target.id) continue;
          const dist = Math.hypot(enemy.x - target.x, enemy.y - target.y);
          danger = Math.max(danger, clamp(1 - dist / 6.5, 0, 1));
        }
        target.danger = danger;
      }
    }

    handleSprintCollisions() {
      if (this.sprintMs <= 0 || !this.enemies.length) return;
      const hits = this.enemies.filter((enemy) => Math.hypot(enemy.x - this.player.x, enemy.y - this.player.y) <= 1.2);
      for (const enemy of hits) this.damageEnemy(enemy, enemy.variant === 'brute' ? 1 : 2, 'dash');
      if (hits.length) this.sprintMs = Math.max(this.sprintMs, 260);
      if (this.defeated >= this.config.rules.win_length) this.finish('win');
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
      this.damageEnemy(best, 1, 'guard');
      if (this.defeated >= this.config.rules.win_length) this.finish('win');
    }

    damageEnemy(enemy, amount, source) {
      enemy.hp = Math.max(0, (enemy.hp || 1) - amount);
      enemy.flashMs = 180;
      if (enemy.hp > 0) {
        this.player.flashMs = 160;
        this.addEffect('blocked', enemy.x, enemy.y, COLORS.enemyHeavy);
        return false;
      }
      this.enemies = this.enemies.filter((item) => item !== enemy);
      this.combo += 1;
      this.maxCombo = Math.max(this.maxCombo, this.combo);
      this.defeated += 1;
      if (enemy.targetId) this.guardedTargets.add(enemy.targetId);
      const target = this.targets.find((item) => item.id === enemy.targetId);
      if (target) this.applyTargetSave(target);
      const gain = 10 + Math.min(30, this.combo * 3);
      this.score += gain;
      this.player.flashMs = 220;
      this.addEffect(`+${gain}${this.combo > 1 ? ` x${this.combo}` : ''}`, enemy.x, enemy.y, COLORS.target);
      this.log(`invasion ${source} success defeated=${this.defeated} combo=${this.combo} score=${this.score}`);
      return true;
    }

    applyTargetSave(target) {
      if (!target || target.stolen) return;
      const wasClutch = (target.danger || 0) >= 0.72;
      target.savedMs = wasClutch ? 900 : 420;
      this.savedTargets.add(target.id);
      this.savedValue += target.value || 0;
      if (wasClutch) {
        this.clutchSaves += 1;
        this.score += 12;
        this.addEffect('SAVE +12', target.x, target.y - 1, COLORS.playerScarf);
      }
      if (target.effect === 'focus') {
        this.pulseCooldownMs = Math.max(0, this.pulseCooldownMs - 1100);
      } else if (target.effect === 'snack') {
        this.sprintMs = Math.max(this.sprintMs, 650);
      } else if (target.effect === 'priority') {
        this.score += 6;
      }
    }

    useSkill(slot) {
      if (this.state !== 'playing') return;
      if (slot === 1) {
        this.usePulse();
      } else if (slot === 2) {
        this.useSprint();
      }
    }

    usePulse() {
      if (this.pulseCooldownMs > 0) {
        this.addEffect('pulse charging', this.player.x, this.player.y, COLORS.dim);
        return;
      }
      const radius = 3.2;
      const hits = this.enemies.filter((enemy) => Math.hypot(enemy.x - this.player.x, enemy.y - this.player.y) <= radius);
      if (!hits.length) {
        this.addEffect('pulse missed', this.player.x, this.player.y, COLORS.dim);
        this.pulseCooldownMs = 900;
        return;
      }
      for (const enemy of hits) this.damageEnemy(enemy, enemy.variant === 'brute' ? 1 : 2, 'pulse');
      this.pulseCooldownMs = 5800;
      this.player.flashMs = 360;
      this.effects.push({ text: 'PULSE', x: this.player.x, y: this.player.y, color: COLORS.playerCore, age: 0, ring: true, radius });
      if (this.defeated >= this.config.rules.win_length) this.finish('win');
    }

    useSprint() {
      if (this.sprintCooldownMs > 0) {
        this.addEffect('sprint charging', this.player.x, this.player.y, COLORS.dim);
        return;
      }
      this.sprintMs = 1650;
      this.sprintCooldownMs = 4600;
      this.player.flashMs = 420;
      this.addEffect('dash', this.player.x, this.player.y, COLORS.playerCore);
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
      this.log(`invasion finish result=${result} score=${this.score} defeated=${this.defeated} stolen=${this.stolen} max_combo=${this.maxCombo}`);
    }

    endText(result) {
      const kept = this.targets.filter((target) => !target.stolen).length;
      const savedNames = this.targets
        .filter((target) => this.savedTargets.has(target.id))
        .slice(0, 3)
        .map((target) => target.title)
        .join(' / ');
      const prefix = result === 'win' ? '守住了桌面秩序' : result === 'lose' ? '桌面被抢走了一部分' : '已撤退';
      const names = savedNames ? `，重点守住：${savedNames}` : '';
      return `${prefix}。保住 ${kept}/${this.targets.length} 个目标，最后一刻救场 ${this.clutchSaves} 次${names}。Enter / A 再来一局，Esc / B 退出`;
    }

    endDetails() {
      return {
        defeated: this.defeated,
        stolen: this.stolen,
        max_combo: this.maxCombo,
        guarded_targets: this.guardedTargets.size,
        elapsed_ms: Math.round(this.elapsedMs),
        flawless: this.state === 'win' && this.stolen === 0,
        wave: this.wave,
        clutch_saves: this.clutchSaves,
        saved_value: this.savedValue,
        lost_value: this.lostValue,
      };
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
      this.renderDesktopSurface(ctx, w, h, metrics.cell);
      ctx.restore();

      for (const target of this.targets) this.renderTarget(ctx, target, toPx, metrics.cell);
      this.renderThreatLines(ctx, toPx);
      for (const enemy of this.enemies) this.renderEnemy(ctx, enemy, toPx, metrics.cell);
      this.renderTrails(ctx, toPx, metrics.cell);
      this.renderPlayer(ctx, toPx, metrics.cell);
      this.renderSkillHud(ctx, metrics);
      this.renderWaveNotice(ctx, metrics);
      this.renderEffects(ctx, toPx, metrics.cell);
    }

    renderDesktopSurface(ctx, width, height, cell) {
      const pxWidth = width * cell;
      const pxHeight = height * cell;
      const glow = ctx.createRadialGradient(
        pxWidth * 0.5,
        pxHeight * 0.48,
        cell * 1.2,
        pxWidth * 0.5,
        pxHeight * 0.48,
        pxWidth * 0.62
      );
      glow.addColorStop(0, COLORS.deskGlow);
      glow.addColorStop(0.48, COLORS.deskWarm);
      glow.addColorStop(1, 'rgba(0, 0, 0, 0)');
      ctx.fillStyle = glow;
      ctx.fillRect(0, 0, pxWidth, pxHeight);

      ctx.save();
      ctx.globalAlpha = 0.13;
      ctx.fillStyle = 'rgba(255,255,255,0.36)';
      for (let i = 0; i < 32; i += 1) {
        const x = ((i * 97) % Math.max(1, pxWidth));
        const y = ((i * 53 + 41) % Math.max(1, pxHeight));
        const r = 0.8 + (i % 3) * 0.34;
        ctx.beginPath();
        ctx.arc(x, y, r, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.restore();
    }

    renderTarget(ctx, target, toPx, cell) {
      const p = toPx(target.x, target.y);
      p.y += Math.sin(this.elapsedMs * 0.0022 + target.phase) * 2.3;
      ctx.save();
      ctx.globalAlpha = target.stolen ? 0.38 : 1;
      const color = this.targetColor(target.kind);
      ctx.fillStyle = 'rgba(0,0,0,0.22)';
      ctx.beginPath();
      ctx.ellipse(p.x, p.y + target.r + 7, target.r * 0.9, 5, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.shadowColor = target.stolen ? 'transparent' : `${color}55`;
      ctx.shadowBlur = target.flashMs > 0 ? 22 : 10;
      this.drawTargetShape(ctx, target, p, color);
      ctx.shadowBlur = 0;
      ctx.fillStyle = COLORS.text;
      ctx.font = '700 10px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(target.title, p.x, p.y + target.r + 12);
      if (target.flashMs > 0) {
        ctx.strokeStyle = COLORS.enemy;
        ctx.lineWidth = 3;
        ctx.strokeRect(p.x - target.r - 3, p.y - target.r - 3, target.r * 2 + 6, target.r * 2 + 6);
      }
      if (target.danger > 0.04 && !target.stolen) {
        ctx.strokeStyle = target.danger > 0.72 ? COLORS.enemyHot : 'rgba(255, 107, 107, 0.58)';
        ctx.lineWidth = target.danger > 0.72 ? 4 : 3;
        ctx.beginPath();
        ctx.arc(p.x, p.y, target.r + 10, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * target.danger);
        ctx.stroke();
      }
      if (target.savedMs > 0) {
        ctx.strokeStyle = COLORS.playerScarf;
        ctx.lineWidth = 3;
        ctx.globalAlpha = Math.min(ctx.globalAlpha, target.savedMs / 900);
        ctx.beginPath();
        ctx.arc(p.x, p.y, target.r + 15, 0, Math.PI * 2);
        ctx.stroke();
      }
      ctx.restore();
    }

    targetColor(kind) {
      if (kind === 'memory_shard') return COLORS.memory;
      if (kind === 'reminder_note') return COLORS.reminder;
      if (kind === 'agent_task') return COLORS.agent;
      return COLORS.treat;
    }

    drawTargetShape(ctx, target, p, color) {
      const r = target.r;
      ctx.lineJoin = 'round';
      ctx.fillStyle = target.stolen ? COLORS.lost : color;
      ctx.strokeStyle = target.stolen ? 'rgba(255,255,255,0.12)' : 'rgba(255,255,255,0.40)';
      ctx.lineWidth = 2;
      if (target.kind === 'memory_shard') {
        ctx.beginPath();
        ctx.moveTo(p.x, p.y - r);
        ctx.lineTo(p.x + r * 0.9, p.y - r * 0.14);
        ctx.lineTo(p.x + r * 0.48, p.y + r);
        ctx.lineTo(p.x - r * 0.48, p.y + r);
        ctx.lineTo(p.x - r * 0.9, p.y - r * 0.14);
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
        const shine = ctx.createLinearGradient(p.x - r, p.y - r, p.x + r, p.y + r);
        shine.addColorStop(0, 'rgba(255,255,255,0.42)');
        shine.addColorStop(0.36, 'rgba(255,255,255,0.08)');
        shine.addColorStop(1, 'rgba(255,255,255,0)');
        ctx.fillStyle = shine;
        ctx.beginPath();
        ctx.moveTo(p.x - r * 0.48, p.y - r * 0.44);
        ctx.lineTo(p.x, p.y - r * 0.82);
        ctx.lineTo(p.x + r * 0.32, p.y - r * 0.24);
        ctx.closePath();
        ctx.fill();
        ctx.strokeStyle = 'rgba(29,36,48,0.38)';
        for (const offset of [-0.34, 0, 0.34]) {
          ctx.beginPath();
          ctx.moveTo(p.x + r * offset, p.y - r * 0.42);
          ctx.lineTo(p.x + r * offset * 0.5, p.y + r * 0.46);
          ctx.stroke();
        }
      } else if (target.kind === 'reminder_note') {
        ctx.beginPath();
        ctx.roundRect(p.x - r, p.y - r, r * 2, r * 2, 6);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = 'rgba(255,255,255,0.24)';
        ctx.beginPath();
        ctx.roundRect(p.x - r * 0.72, p.y - r * 0.76, r * 1.12, 5, 3);
        ctx.fill();
        ctx.fillStyle = 'rgba(29,36,48,0.18)';
        ctx.beginPath();
        ctx.moveTo(p.x + r * 0.36, p.y - r);
        ctx.lineTo(p.x + r, p.y - r * 0.36);
        ctx.lineTo(p.x + r * 0.36, p.y - r * 0.36);
        ctx.closePath();
        ctx.fill();
        ctx.strokeStyle = 'rgba(29,36,48,0.30)';
        for (let i = 0; i < 3; i += 1) {
          ctx.beginPath();
          ctx.moveTo(p.x - r * 0.52, p.y - r * 0.22 + i * 7);
          ctx.lineTo(p.x + r * 0.42, p.y - r * 0.22 + i * 7);
          ctx.stroke();
        }
      } else if (target.kind === 'agent_task') {
        ctx.beginPath();
        ctx.roundRect(p.x - r * 1.08, p.y - r * 0.78, r * 2.16, r * 1.56, 7);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = 'rgba(255,255,255,0.16)';
        ctx.beginPath();
        ctx.roundRect(p.x - r * 0.82, p.y - r * 0.58, r * 1.64, r * 0.28, 4);
        ctx.fill();
        ctx.fillStyle = 'rgba(18,22,29,0.36)';
        ctx.fillRect(p.x - r * 0.82, p.y - r * 0.38, r * 1.64, r * 0.42);
        ctx.fillStyle = COLORS.targetInk;
        ctx.font = '900 13px ui-monospace, Consolas, monospace';
        ctx.textAlign = 'center';
        ctx.fillText('>_', p.x, p.y + r * 0.35);
      } else {
        ctx.beginPath();
        ctx.ellipse(p.x, p.y, r * 1.08, r * 0.62, -0.12, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = 'rgba(255,255,255,0.26)';
        ctx.beginPath();
        ctx.ellipse(p.x + r * 0.22, p.y - r * 0.22, r * 0.52, r * 0.18, -0.15, 0, Math.PI * 2);
        ctx.fill();
        ctx.beginPath();
        ctx.moveTo(p.x - r * 1.08, p.y);
        ctx.lineTo(p.x - r * 1.56, p.y - r * 0.46);
        ctx.lineTo(p.x - r * 1.56, p.y + r * 0.46);
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = COLORS.targetInk;
        ctx.beginPath();
        ctx.arc(p.x + r * 0.42, p.y - r * 0.12, 2, 0, Math.PI * 2);
        ctx.fill();
      }
      if (target.kind !== 'agent_task') {
        ctx.fillStyle = target.stolen ? 'rgba(255,255,255,0.42)' : COLORS.targetInk;
        ctx.font = '900 14px system-ui, sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(labelFor(target.kind), p.x, p.y + 1);
      }
    }

    renderEnemy(ctx, enemy, toPx, cell) {
      const p = toPx(enemy.x, enemy.y);
      p.y += Math.sin(this.elapsedMs * 0.006 + enemy.x) * (enemy.variant === 'skitter' ? 2.2 : 1.2);
      ctx.save();
      ctx.fillStyle = 'rgba(0,0,0,0.24)';
      ctx.beginPath();
      ctx.ellipse(p.x, p.y + enemy.r + 7, enemy.r * 0.9, 4, 0, 0, Math.PI * 2);
      ctx.fill();
      if (enemy.warnMs > 0) {
        ctx.globalAlpha = 0.24 + 0.18 * Math.sin(enemy.warnMs * 0.06);
        ctx.strokeStyle = COLORS.enemy;
        ctx.lineWidth = 4;
        ctx.beginPath();
        ctx.arc(p.x, p.y, enemy.r + 10, 0, Math.PI * 2);
        ctx.stroke();
        ctx.globalAlpha = 1;
      }
      const pulse = 0.5 + 0.5 * Math.sin((this.elapsedMs + enemy.x * 80) * 0.018);
      ctx.shadowColor = 'rgba(255, 107, 107, 0.34)';
      ctx.shadowBlur = 12 + pulse * 8;
      this.drawEnemyBody(ctx, enemy, p);
      ctx.shadowBlur = 0;
      if ((enemy.hp || 1) > 1) {
        ctx.fillStyle = COLORS.text;
        ctx.font = '900 10px system-ui, sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(String(enemy.hp), p.x, p.y - enemy.r - 8);
      }
      ctx.restore();
    }

    drawEnemyBody(ctx, enemy, p) {
      const r = enemy.r;
      ctx.fillStyle = this.enemyColor(enemy);
      ctx.strokeStyle = 'rgba(255,255,255,0.28)';
      ctx.lineWidth = 2;
      ctx.lineJoin = 'round';
      if (enemy.variant === 'brute') {
        ctx.beginPath();
        ctx.roundRect(p.x - r * 0.95, p.y - r * 0.82, r * 1.9, r * 1.64, 5);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = 'rgba(255,255,255,0.18)';
        ctx.beginPath();
        ctx.roundRect(p.x - r * 0.78, p.y - r * 0.66, r * 1.56, r * 0.34, 4);
        ctx.fill();
        ctx.strokeStyle = 'rgba(77, 25, 86, 0.42)';
        ctx.beginPath();
        ctx.moveTo(p.x - r * 0.3, p.y - r * 0.74);
        ctx.lineTo(p.x - r * 0.3, p.y + r * 0.76);
        ctx.moveTo(p.x + r * 0.3, p.y - r * 0.74);
        ctx.lineTo(p.x + r * 0.3, p.y + r * 0.76);
        ctx.stroke();
        ctx.fillStyle = 'rgba(255,255,255,0.15)';
        ctx.fillRect(p.x - r * 0.58, p.y - r * 0.54, r * 1.16, 5);
        ctx.fillStyle = 'rgba(0,0,0,0.48)';
        ctx.fillRect(p.x - r * 0.46, p.y - 2, r * 0.92, 4);
        ctx.fillStyle = '#2b1430';
        ctx.beginPath();
        ctx.arc(p.x - r * 0.34, p.y - r * 0.16, 2, 0, Math.PI * 2);
        ctx.arc(p.x + r * 0.34, p.y - r * 0.16, 2, 0, Math.PI * 2);
        ctx.fill();
        ctx.strokeStyle = 'rgba(0,0,0,0.34)';
        for (const side of [-1, 1]) {
          ctx.beginPath();
          ctx.moveTo(p.x + side * r * 0.78, p.y + r * 0.2);
          ctx.lineTo(p.x + side * r * 1.16, p.y + r * 0.62);
          ctx.stroke();
        }
      } else if (enemy.variant === 'skitter') {
        ctx.beginPath();
        ctx.moveTo(p.x, p.y - r * 1.12);
        ctx.lineTo(p.x + r * 0.9, p.y + r * 0.76);
        ctx.lineTo(p.x, p.y + r * 0.42);
        ctx.lineTo(p.x - r * 0.9, p.y + r * 0.76);
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = 'rgba(255,255,255,0.20)';
        ctx.beginPath();
        ctx.moveTo(p.x, p.y - r * 0.86);
        ctx.lineTo(p.x + r * 0.28, p.y + r * 0.22);
        ctx.lineTo(p.x, p.y + r * 0.08);
        ctx.lineTo(p.x - r * 0.28, p.y + r * 0.22);
        ctx.closePath();
        ctx.fill();
        ctx.strokeStyle = 'rgba(0,0,0,0.34)';
        ctx.lineWidth = 2;
        for (const side of [-1, 1]) {
          ctx.beginPath();
          ctx.moveTo(p.x + side * r * 0.28, p.y + r * 0.2);
          ctx.lineTo(p.x + side * r * 1.22, p.y + r * 0.16);
          ctx.stroke();
        }
        ctx.fillStyle = 'rgba(0,0,0,0.46)';
        ctx.fillRect(p.x - 4, p.y - 2, 8, 4);
        ctx.fillStyle = '#351721';
        ctx.beginPath();
        ctx.arc(p.x - 3, p.y - 4, 1.5, 0, Math.PI * 2);
        ctx.arc(p.x + 3, p.y - 4, 1.5, 0, Math.PI * 2);
        ctx.fill();
      } else {
        ctx.beginPath();
        ctx.arc(p.x, p.y, r, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = 'rgba(255,255,255,0.20)';
        ctx.beginPath();
        ctx.arc(p.x - r * 0.18, p.y - r * 0.28, r * 0.44, 0, Math.PI * 2);
        ctx.fill();
        ctx.fillStyle = 'rgba(255,255,255,0.16)';
        ctx.beginPath();
        ctx.arc(p.x - r * 0.28, p.y - r * 0.32, r * 0.22, 0, Math.PI * 2);
        ctx.arc(p.x + r * 0.28, p.y - r * 0.32, r * 0.22, 0, Math.PI * 2);
        ctx.fill();
        ctx.strokeStyle = 'rgba(0,0,0,0.34)';
        ctx.lineWidth = 2;
        for (const side of [-1, 1]) {
          ctx.beginPath();
          ctx.moveTo(p.x + side * r * 0.56, p.y + r * 0.1);
          ctx.lineTo(p.x + side * r * 1.22, p.y + r * 0.42);
          ctx.stroke();
          ctx.beginPath();
          ctx.moveTo(p.x + side * r * 0.42, p.y + r * 0.54);
          ctx.lineTo(p.x + side * r * 1.04, p.y + r * 0.82);
          ctx.stroke();
        }
        ctx.fillStyle = 'rgba(0,0,0,0.46)';
        ctx.fillRect(p.x - 5, p.y - 2, 10, 4);
        ctx.fillStyle = '#321719';
        ctx.beginPath();
        ctx.arc(p.x - r * 0.28, p.y - r * 0.22, 1.7, 0, Math.PI * 2);
        ctx.arc(p.x + r * 0.28, p.y - r * 0.22, 1.7, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    enemyColor(enemy) {
      if (enemy.warnMs > 0) return COLORS.enemyHot;
      if (enemy.variant === 'skitter') return COLORS.enemyFast;
      if (enemy.variant === 'brute') return COLORS.enemyHeavy;
      return COLORS.enemy;
    }

    renderThreatLines(ctx, toPx) {
      ctx.save();
      ctx.lineWidth = 2;
      for (const enemy of this.enemies) {
        const target = this.targets.find((item) => item.id === enemy.targetId && !item.stolen);
        if (!target) continue;
        const e = toPx(enemy.x, enemy.y);
        const t = toPx(target.x, target.y);
        ctx.globalAlpha = enemy.warnMs > 0 ? 0.42 : 0.2;
        ctx.strokeStyle = COLORS.enemy;
        ctx.setLineDash([6, 8]);
        ctx.beginPath();
        ctx.moveTo(e.x, e.y);
        ctx.lineTo(t.x, t.y);
        ctx.stroke();
      }
      ctx.setLineDash([]);
      ctx.restore();
    }

    renderPlayer(ctx, toPx, cell) {
      const p = toPx(this.player.x, this.player.y);
      p.y += Math.sin(this.elapsedMs * 0.0048) * 1.8;
      ctx.save();
      ctx.globalAlpha = 0.18;
      ctx.strokeStyle = COLORS.player;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(p.x, p.y, cell * 1.65, 0, Math.PI * 2);
      ctx.stroke();
      ctx.globalAlpha = 1;
      ctx.shadowColor = COLORS.playerShadow;
      ctx.shadowBlur = 18;
      const bodyColor = this.player.flashMs > 0 ? COLORS.target : COLORS.player;
      ctx.fillStyle = 'rgba(0,0,0,0.28)';
      ctx.beginPath();
      ctx.ellipse(p.x + 2, p.y + 35, 22, 6, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = bodyColor;
      ctx.beginPath();
      ctx.ellipse(p.x, p.y + 15, 14, 18, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = 'rgba(255,255,255,0.26)';
      ctx.lineWidth = 2;
      ctx.stroke();
      const flutter = Math.sin(this.elapsedMs * 0.011) * 4 + (this.input.dx || 0) * -3;
      ctx.strokeStyle = COLORS.playerCore;
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.moveTo(p.x - 15, p.y + 8);
      ctx.lineTo(p.x + 15, p.y + 22);
      ctx.stroke();
      ctx.fillStyle = COLORS.playerScarf;
      ctx.beginPath();
      ctx.roundRect(p.x - 13, p.y + 5, 26, 6, 3);
      ctx.fill();
      ctx.beginPath();
      ctx.moveTo(p.x + 7, p.y + 9);
      ctx.lineTo(p.x + 22, p.y + 17 + flutter);
      ctx.lineTo(p.x + 12, p.y + 21 + flutter * 0.5);
      ctx.closePath();
      ctx.fill();
      ctx.fillStyle = bodyColor;
      ctx.beginPath();
      ctx.arc(p.x, p.y, this.player.r, 0, Math.PI * 2);
      ctx.fill();
      ctx.beginPath();
      ctx.moveTo(p.x - 12, p.y - 11);
      ctx.lineTo(p.x - 6, p.y - 25);
      ctx.lineTo(p.x + 1, p.y - 11);
      ctx.closePath();
      ctx.fill();
      ctx.beginPath();
      ctx.moveTo(p.x + 12, p.y - 11);
      ctx.lineTo(p.x + 6, p.y - 25);
      ctx.lineTo(p.x - 1, p.y - 11);
      ctx.closePath();
      ctx.fill();
      ctx.fillStyle = 'rgba(255,255,255,0.24)';
      ctx.beginPath();
      ctx.ellipse(p.x - 5, p.y - 8, 9, 5, -0.22, 0, Math.PI * 2);
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.strokeStyle = 'rgba(255,255,255,0.28)';
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(p.x, p.y, this.player.r, 0, Math.PI * 2);
      ctx.stroke();
      ctx.strokeStyle = bodyColor;
      ctx.lineWidth = 5;
      ctx.beginPath();
      ctx.arc(p.x + 12, p.y + 15, 18, (-0.4 + flutter * 0.006) * Math.PI, (0.26 + flutter * 0.004) * Math.PI);
      ctx.stroke();
      ctx.fillStyle = COLORS.playerCore;
      ctx.beginPath();
      ctx.arc(p.x - 6, p.y - 3, 2.5, 0, Math.PI * 2);
      ctx.arc(p.x + 6, p.y - 3, 2.5, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = 'rgba(18,22,29,0.36)';
      ctx.beginPath();
      ctx.arc(p.x - 5.5, p.y - 3, 1, 0, Math.PI * 2);
      ctx.arc(p.x + 5.5, p.y - 3, 1, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = COLORS.playerNose;
      ctx.beginPath();
      ctx.moveTo(p.x, p.y + 1);
      ctx.lineTo(p.x - 3, p.y + 5);
      ctx.lineTo(p.x + 3, p.y + 5);
      ctx.closePath();
      ctx.fill();
      ctx.strokeStyle = COLORS.playerCore;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(p.x, p.y + 2, 6, 0.16 * Math.PI, 0.84 * Math.PI);
      ctx.stroke();
      ctx.fillStyle = COLORS.playerCore;
      ctx.beginPath();
      ctx.arc(p.x - 8, p.y + 18, 3, 0, Math.PI * 2);
      ctx.arc(p.x + 8, p.y + 18, 3, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }

    renderTrails(ctx, toPx, cell) {
      ctx.save();
      for (const trail of this.trails) {
        const p = toPx(trail.x, trail.y);
        const alpha = 1 - trail.age / 420;
        ctx.globalAlpha = alpha * 0.22;
        ctx.fillStyle = trail.color;
        ctx.beginPath();
        ctx.ellipse(p.x, p.y + 22, cell * trail.r, cell * trail.r * 0.38, 0, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.restore();
    }

    renderSkillHud(ctx, metrics) {
      const x = metrics.offsetX + 16;
      const y = metrics.offsetY + metrics.cell * this.config.grid.height - 46;
      const skills = [
        { label: 'X pulse', ready: this.pulseCooldownMs <= 0, cd: this.pulseCooldownMs },
        { label: 'Y dash', ready: this.sprintCooldownMs <= 0, cd: this.sprintCooldownMs },
      ];
      ctx.save();
      ctx.font = '800 12px system-ui, sans-serif';
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';
      skills.forEach((skill, index) => {
        const bx = x + index * 104;
        ctx.fillStyle = skill.ready ? 'rgba(255, 209, 102, 0.18)' : 'rgba(18, 22, 29, 0.64)';
        ctx.strokeStyle = skill.ready ? 'rgba(255, 209, 102, 0.52)' : 'rgba(255,255,255,0.12)';
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.roundRect(bx, y, 92, 30, 8);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = skill.ready ? COLORS.text : COLORS.dim;
        ctx.fillText(skill.ready ? skill.label : `${skill.label} ${Math.ceil(skill.cd / 1000)}s`, bx + 10, y + 15);
      });
      ctx.restore();
    }

    renderWaveNotice(ctx, metrics) {
      if (this.waveNoticeMs <= 0) return;
      const alpha = Math.min(1, this.waveNoticeMs / 450);
      const cx = metrics.offsetX + (metrics.cell * this.config.grid.width) / 2;
      const cy = metrics.offsetY + 34;
      ctx.save();
      ctx.globalAlpha = alpha;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillStyle = 'rgba(12, 16, 22, 0.62)';
      ctx.strokeStyle = 'rgba(255, 209, 102, 0.46)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.roundRect(cx - 72, cy - 17, 144, 34, 10);
      ctx.fill();
      ctx.stroke();
      ctx.fillStyle = COLORS.text;
      ctx.font = '900 15px system-ui, sans-serif';
      ctx.fillText(`wave ${this.wave}`, cx, cy);
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
        if (effect.ring) {
          ctx.strokeStyle = effect.color;
          ctx.lineWidth = 3;
          ctx.beginPath();
          ctx.arc(p.x, p.y, cell * effect.radius * (0.2 + effect.age / 720), 0, Math.PI * 2);
          ctx.stroke();
        } else {
          ctx.fillStyle = effect.color;
          ctx.fillText(effect.text, p.x, p.y);
        }
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
