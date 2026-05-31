import * as THREE from 'three';
import { GLTFLoader } from '../vendor/three/examples/jsm/loaders/GLTFLoader.js';

(function () {
  const GRAVITY = -36;
  const FLOOR_Y = 0;
  const STAGE_HALF_WIDTH = 5.45;
  const STAGE_SOFT_EDGE = 4.85;
  const ASSIST_HALF_WIDTH = 5.95;
  const DEFAULT_HP = 100;
  const HITSTOP_MS = 70;
  const MODEL_BASE_URL = '/assets/arena/';
  const ARENA_BACKGROUND_URL = `${MODEL_BASE_URL}backgrounds/temple-arena.png`;

  function clamp(n, min, max) {
    return Math.max(min, Math.min(max, n));
  }

  function signOrFacing(value, fallback) {
    const sign = Math.sign(value || 0);
    return sign === 0 ? fallback : sign;
  }

  function boxesOverlap(a, b) {
    return (
      Math.abs(a.x - b.x) * 2 < a.w + b.w &&
      Math.abs(a.y - b.y) * 2 < a.h + b.h &&
      Math.abs(a.z - b.z) * 2 < a.d + b.d
    );
  }

  function attackData(kind) {
    if (kind === 'light2') {
      return {
        kind,
        duration: 300,
        startup: 58,
        active: 112,
        recovery: 130,
        damage: 9,
        knockback: 4.5,
        hitstun: 310,
        width: 1.16,
        height: 1.0,
        depth: 1.35,
        offsetX: 0.96,
        offsetY: 1.04,
        vfx: 'slash_up',
      };
    }
    if (kind === 'light3') {
      return {
        kind,
        duration: 420,
        startup: 78,
        active: 130,
        recovery: 212,
        damage: 12,
        knockback: 7.1,
        hitstun: 390,
        width: 1.44,
        height: 1.18,
        depth: 1.4,
        offsetX: 1.12,
        offsetY: 1.08,
        lift: 4.2,
        vfx: 'slash_down',
      };
    }
    if (kind === 'dash') {
      return {
        kind,
        duration: 520,
        startup: 45,
        active: 230,
        recovery: 245,
        damage: 14,
        knockback: 8.4,
        hitstun: 430,
        width: 1.48,
        height: 1.0,
        depth: 1.5,
        offsetX: 1.04,
        offsetY: 0.98,
        lift: 2.8,
        vfx: 'dash_slash',
      };
    }
    if (kind === 'spell') {
      return {
        kind,
        duration: 680,
        startup: 165,
        active: 250,
        recovery: 265,
        damage: 18,
        knockback: 5.8,
        hitstun: 520,
        width: 2.35,
        height: 1.45,
        depth: 2.1,
        offsetX: 1.28,
        offsetY: 1.12,
        lift: 5.8,
        vfx: 'spell_burst',
      };
    }
    if (kind === 'heavy') {
      return {
        kind: 'heavy',
        duration: 470,
        startup: 130,
        active: 145,
        recovery: 195,
        damage: 16,
        knockback: 6.8,
        hitstun: 420,
        width: 1.32,
        height: 1.1,
        depth: 1.5,
        offsetX: 1.02,
        offsetY: 1.05,
        lift: 2.2,
        vfx: 'slash_down',
      };
    }
    if (kind === 'assist') {
      return {
        kind,
        duration: 280,
        startup: 40,
        active: 160,
        recovery: 80,
        damage: 10,
        knockback: 5.2,
        hitstun: 310,
        width: 1.35,
        height: 1.05,
        depth: 1.6,
        offsetX: 0.72,
        offsetY: 1.0,
        lift: 2.8,
        vfx: 'assist',
      };
    }
    return {
      kind: kind === 'light1' ? 'light1' : 'light',
      duration: 280,
      startup: 65,
      active: 105,
      recovery: 110,
      damage: 8,
      knockback: 3.8,
      hitstun: 280,
      width: 1.06,
      height: 0.95,
      depth: 1.35,
      offsetX: 0.86,
      offsetY: 1.02,
      lift: 1.8,
      vfx: 'slash',
    };
  }

  class InputBuffer {
    constructor() {
      this.hold = new Set();
      this.queue = [];
      this.windowMs = 170;
    }

    setHeld(name, held) {
      if (held) this.hold.add(name);
      else this.hold.delete(name);
    }

    push(name, now) {
      this.queue.push({ name, time: now });
      this.queue = this.queue.slice(-12);
    }

    consume(names, now) {
      const allowed = Array.isArray(names) ? names : [names];
      for (let i = this.queue.length - 1; i >= 0; i--) {
        const item = this.queue[i];
        if (now - item.time > this.windowMs) continue;
        if (!allowed.includes(item.name)) continue;
        this.queue.splice(i, 1);
        return item.name;
      }
      return null;
    }

    axis() {
      return {
        x: (this.hold.has('right') ? 1 : 0) - (this.hold.has('left') ? 1 : 0),
        y: (this.hold.has('up') ? 1 : 0) - (this.hold.has('down') ? 1 : 0),
      };
    }

    clearStale(now) {
      this.queue = this.queue.filter((item) => now - item.time <= this.windowMs);
    }
  }

  class Fighter {
    constructor(id, opts = {}) {
      this.id = id;
      this.name = opts.name || id;
      this.isPlayer = Boolean(opts.isPlayer);
      this.pos = { x: opts.x || 0, y: FLOOR_Y, z: opts.z || 0 };
      this.vel = { x: 0, y: 0, z: 0 };
      this.facing = opts.facing || 1;
      this.hp = opts.hp || DEFAULT_HP;
      this.maxHp = this.hp;
      this.damageScale = opts.damageScale ?? 1;
      this.state = 'idle';
      this.stateMs = 0;
      this.attack = null;
      this.attackHitIds = new Set();
      this.hitstunMs = 0;
      this.blockstunMs = 0;
      this.combo = 0;
      this.comboTimer = 0;
      this.assistCooldownMs = 0;
      this.rounds = 0;
      this.mesh = null;
      this.bodyMaterial = null;
      this.input = new InputBuffer();
      this.onGround = true;
      this.guardHeld = false;
      this.lightChain = 0;
      this.lightChainTimer = 0;
      this.skillCooldowns = { dash: 0, spell: 0 };
      this.defeated = false;
    }

    resetForRound(x, facing) {
      this.pos.x = x;
      this.pos.y = FLOOR_Y;
      this.pos.z = 0;
      this.vel.x = 0;
      this.vel.y = 0;
      this.vel.z = 0;
      this.facing = facing;
      this.hp = this.maxHp;
      this.state = 'idle';
      this.stateMs = 0;
      this.attack = null;
      this.attackHitIds.clear();
      this.hitstunMs = 0;
      this.blockstunMs = 0;
      this.combo = 0;
      this.comboTimer = 0;
      this.assistCooldownMs = 0;
      this.onGround = true;
      this.guardHeld = false;
      this.lightChain = 0;
      this.lightChainTimer = 0;
      this.skillCooldowns = { dash: 0, spell: 0 };
      this.defeated = false;
    }

    hurtbox() {
      return {
        x: this.pos.x,
        y: this.pos.y + 0.95,
        z: this.pos.z,
        w: 0.72,
        h: 1.9,
        d: 0.92,
      };
    }

    hitbox() {
      if (!this.attack || !this.isAttackActive()) return null;
      const data = this.attack.data;
      return {
        x: this.pos.x + this.facing * data.offsetX,
        y: this.pos.y + data.offsetY,
        z: this.pos.z,
        w: data.width,
        h: data.height,
        d: data.depth,
        owner: this,
        data,
      };
    }

    isAttackActive() {
      if (!this.attack) return false;
      const elapsed = this.stateMs;
      return elapsed >= this.attack.data.startup && elapsed <= this.attack.data.startup + this.attack.data.active;
    }

    canAct() {
      return !['attack', 'hurt', 'blockstun', 'dead', 'win'].includes(this.state);
    }

    isBlockingAgainst(attacker) {
      if (!this.guardHeld || this.onGround === false) return false;
      const towardAttacker = Math.sign(attacker.pos.x - this.pos.x) || this.facing;
      return this.facing === towardAttacker;
    }

    startAttack(kind, now) {
      if (!this.canAct()) return false;
      let resolvedKind = kind;
      if (kind === 'light') {
        this.lightChain = this.lightChainTimer > 0 ? (this.lightChain % 3) + 1 : 1;
        this.lightChainTimer = 760;
        resolvedKind = `light${this.lightChain}`;
      }
      if ((kind === 'dash' || kind === 'spell') && this.skillCooldowns[kind] > 0) return false;
      const data = attackData(resolvedKind);
      this.state = 'attack';
      this.stateMs = 0;
      this.attack = { data, startedAt: now };
      this.attackHitIds.clear();
      if (kind === 'dash') {
        this.skillCooldowns.dash = 2600;
        this.vel.x = this.facing * 9.4;
      } else if (kind === 'spell') {
        this.skillCooldowns.spell = 5200;
        this.vel.x *= 0.25;
      }
      return true;
    }

    jump() {
      if (!this.canAct() || !this.onGround) return false;
      this.vel.y = 10.6;
      this.onGround = false;
      this.state = 'jump';
      this.stateMs = 0;
      return true;
    }

    applyHit(attacker, hitbox) {
      if (this.state === 'dead' || this.state === 'win') return { blocked: false, applied: false };
      const blocked = this.isBlockingAgainst(attacker);
      const rawDamage = Math.max(1, Math.round(hitbox.data.damage * (attacker.damageScale ?? 1)));
      const damage = blocked ? Math.max(1, Math.floor(rawDamage * 0.2)) : rawDamage;
      this.hp = Math.max(0, this.hp - damage);
      this.vel.x = attacker.facing * (blocked ? hitbox.data.knockback * 0.32 : hitbox.data.knockback);
      this.vel.y = blocked ? Math.max(this.vel.y, 0) : Math.max(this.vel.y, hitbox.data.lift ?? 2.2);
      this.onGround = blocked ? this.onGround : false;
      if (this.hp <= 0) {
        this.state = 'dead';
        this.stateMs = 0;
        this.defeated = true;
      } else if (blocked) {
        this.state = 'blockstun';
        this.blockstunMs = 150;
        this.stateMs = 0;
      } else {
        this.state = 'hurt';
        this.hitstunMs = hitbox.data.hitstun;
        this.stateMs = 0;
      }
      return { blocked, applied: true, damage };
    }

    update(dt, now, opponent) {
      this.stateMs += dt;
      this.assistCooldownMs = Math.max(0, this.assistCooldownMs - dt);
      this.lightChainTimer = Math.max(0, this.lightChainTimer - dt);
      if (this.lightChainTimer === 0) this.lightChain = 0;
      this.skillCooldowns.dash = Math.max(0, this.skillCooldowns.dash - dt);
      this.skillCooldowns.spell = Math.max(0, this.skillCooldowns.spell - dt);
      if (this.comboTimer > 0) {
        this.comboTimer = Math.max(0, this.comboTimer - dt);
        if (this.comboTimer === 0) this.combo = 0;
      }
      this.input.clearStale(now);

      if (opponent && opponent.pos.x !== this.pos.x) {
        this.facing = signOrFacing(opponent.pos.x - this.pos.x, this.facing);
      }

      if (this.state === 'hurt') {
        this.hitstunMs -= dt;
        if (this.hitstunMs <= 0 && this.hp > 0) this.state = this.onGround ? 'idle' : 'jump';
      } else if (this.state === 'blockstun') {
        this.blockstunMs -= dt;
        if (this.blockstunMs <= 0 && this.hp > 0) this.state = 'idle';
      } else if (this.state === 'attack') {
        if (this.stateMs >= this.attack.data.duration) {
          this.attack = null;
          this.state = this.onGround ? 'idle' : 'jump';
          this.stateMs = 0;
        }
      } else if (this.state !== 'dead' && this.state !== 'win') {
        const axis = this.input.axis();
        this.guardHeld = axis.y < 0 || this.input.hold.has('guard');
        if (this.canAct()) {
          const attack = this.input.consume(['spell', 'dash', 'heavy', 'light'], now);
          if (attack) this.startAttack(attack, now);
          else if (this.input.consume('jump', now)) this.jump();
          if (this.canAct()) {
            const speed = this.guardHeld ? 2.7 : 6.4;
            this.vel.x = axis.x * speed;
            if (Math.abs(this.vel.x) > 0.01 && this.onGround) this.state = 'run';
            else if (this.onGround) this.state = this.guardHeld ? 'guard' : 'idle';
          }
        }
      }

      this.vel.y += GRAVITY * (dt / 1000);
      this.pos.x += this.vel.x * (dt / 1000);
      this.pos.y += this.vel.y * (dt / 1000);
      this.vel.x *= this.onGround ? 0.84 : 0.97;
      const clampedX = clamp(this.pos.x, -STAGE_HALF_WIDTH, STAGE_HALF_WIDTH);
      if (clampedX !== this.pos.x) this.vel.x = 0;
      this.pos.x = clampedX;
      if (this.pos.y <= FLOOR_Y) {
        this.pos.y = FLOOR_Y;
        this.vel.y = 0;
        if (!this.onGround && this.state === 'jump') this.state = 'idle';
        this.onGround = true;
      } else {
        this.onGround = false;
      }
    }
  }

  class FighterAI {
    constructor(fighter, opts = {}) {
      this.fighter = fighter;
      this.reactionMs = opts.reactionMs || 300;
      this.timer = 0;
      this.attackCooldownMs = 0;
      this.intent = 'idle';
      this.rng = opts.rng || Math.random;
    }

    update(dt, now, opponent) {
      this.attackCooldownMs = Math.max(0, this.attackCooldownMs - dt);
      this.timer -= dt;
      if (this.timer > 0) return;
      this.timer = this.reactionMs + this.rng() * 110;
      const f = this.fighter;
      const distance = Math.abs(opponent.pos.x - f.pos.x);
      if (Math.abs(f.pos.x) > STAGE_SOFT_EDGE) {
        f.input.setHeld(f.pos.x > 0 ? 'left' : 'right', true);
        this.intent = 'center';
        return;
      }
      if (distance > 6.6) {
        this.intent = 'watch';
        return;
      }
      const opponentAttacking = opponent.state === 'attack' && opponent.stateMs < 210;
      f.input.hold.clear();
      if (opponentAttacking && distance < 1.8) {
        f.input.setHeld('down', true);
        this.intent = 'block';
        return;
      }
      if (!opponent.onGround && distance < 2.4 && this.attackCooldownMs <= 0 && this.rng() < 0.55) {
        f.input.push('heavy', now);
        this.attackCooldownMs = 820;
        this.intent = 'anti_air';
        return;
      }
      if (distance > 3.4 && distance < 6.2 && this.attackCooldownMs <= 0 && this.rng() < 0.25) {
        f.input.push('dash', now);
        this.attackCooldownMs = 1200;
        this.intent = 'dash';
        return;
      }
      if (distance < 3.2 && this.attackCooldownMs <= 0 && this.rng() < 0.18) {
        f.input.push('spell', now);
        this.attackCooldownMs = 1800;
        this.intent = 'spell';
        return;
      }
      if (distance > 1.32) {
        f.input.setHeld(opponent.pos.x > f.pos.x ? 'right' : 'left', true);
        this.intent = 'approach';
        return;
      }
      if (this.attackCooldownMs <= 0 && this.rng() < 0.54) {
        f.input.push(this.rng() < 0.28 ? 'heavy' : 'light', now);
        this.attackCooldownMs = 560 + this.rng() * 260;
        this.intent = 'poke';
      } else {
        f.input.setHeld('down', true);
        this.intent = 'guard';
      }
    }
  }

  class BitCatAssist {
    constructor(owner) {
      this.owner = owner;
      this.activeMs = 0;
      this.cooldownMs = 0;
      this.pos = { x: owner.pos.x - 0.8, y: 0.55, z: 0 };
      this.facing = owner.facing;
      this.hitIds = new Set();
      this.mesh = null;
    }

    trigger(target) {
      if (this.cooldownMs > 0 || this.activeMs > 0) return false;
      this.activeMs = 460;
      this.cooldownMs = 7200;
      this.facing = signOrFacing(target.pos.x - this.owner.pos.x, this.owner.facing);
      this.pos.x = this.owner.pos.x + this.facing * 0.4;
      this.pos.y = 0.62;
      this.hitIds.clear();
      return true;
    }

    hitbox() {
      if (this.activeMs <= 0) return null;
      return {
        x: this.pos.x + this.facing * 0.42,
        y: 0.84,
        z: 0,
        w: 0.92,
        h: 0.92,
        d: 1.1,
        owner: this.owner,
        data: attackData('assist'),
        assist: this,
      };
    }

    update(dt) {
      this.cooldownMs = Math.max(0, this.cooldownMs - dt);
      if (this.activeMs > 0) {
        this.activeMs = Math.max(0, this.activeMs - dt);
        this.pos.x += this.facing * 7.2 * (dt / 1000);
      } else {
        this.pos.x += (this.owner.pos.x - this.facing * 0.95 - this.pos.x) * 0.14;
        this.pos.y += (0.62 - this.pos.y) * 0.14;
        this.facing = this.owner.facing;
      }
      this.pos.x = clamp(this.pos.x, -ASSIST_HALF_WIDTH, ASSIST_HALF_WIDTH);
    }
  }

  class ArenaCombat {
    constructor(player, enemy, assist = null) {
      this.player = player;
      this.enemy = enemy;
      this.assist = assist;
      this.hitstopMs = 0;
      this.lastHit = null;
      this.lastHitSeq = 0;
    }

    update(dt) {
      if (this.hitstopMs > 0) {
        this.hitstopMs = Math.max(0, this.hitstopMs - dt);
        return;
      }
      this.checkHit(this.player, this.enemy);
      this.checkHit(this.enemy, this.player);
      if (this.assist) this.checkAssist(this.assist, this.enemy);
    }

    checkHit(attacker, defender) {
      const hitbox = attacker.hitbox();
      if (!hitbox || attacker.attackHitIds.has(defender.id)) return false;
      if (!boxesOverlap(hitbox, defender.hurtbox())) return false;
      attacker.attackHitIds.add(defender.id);
      const hit = defender.applyHit(attacker, hitbox);
      if (hit.applied && !hit.blocked) {
        attacker.combo += 1;
        attacker.comboTimer = 900;
      }
      this.hitstopMs = HITSTOP_MS;
      this.lastHit = { attacker: attacker.id, defender: defender.id, data: hitbox.data, ...hit };
      this.lastHitSeq += 1;
      return true;
    }

    checkAssist(assist, defender) {
      const hitbox = assist.hitbox();
      if (!hitbox || assist.hitIds.has(defender.id)) return false;
      if (!boxesOverlap(hitbox, defender.hurtbox())) return false;
      assist.hitIds.add(defender.id);
      const hit = defender.applyHit(assist.owner, hitbox);
      if (hit.applied && !hit.blocked) {
        assist.owner.combo += 1;
        assist.owner.comboTimer = 900;
      }
      this.hitstopMs = HITSTOP_MS;
      this.lastHit = { attacker: 'bitcat', defender: defender.id, data: hitbox.data, ...hit };
      this.lastHitSeq += 1;
      return true;
    }
  }

  function createStandardMaterial(color, opts = {}) {
    return new THREE.MeshStandardMaterial({
      color,
      roughness: opts.roughness ?? 0.68,
      metalness: opts.metalness ?? 0.06,
      transparent: opts.opacity !== undefined,
      opacity: opts.opacity ?? 1,
    });
  }

  function createStylizedMaterial(color, opts = {}) {
    const sourceMap = opts.map || null;
    const material = new THREE.MeshStandardMaterial({
      color,
      map: sourceMap,
      roughness: opts.roughness ?? 0.54,
      metalness: opts.metalness ?? 0.08,
      transparent: opts.opacity !== undefined,
      opacity: opts.opacity ?? 1,
    });
    return material;
  }

  function createPart(geometry, material) {
    const mesh = new THREE.Mesh(geometry, material);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    return mesh;
  }

  function createBoxMesh(color, size = [0.8, 1.8, 0.8]) {
    const material = createStylizedMaterial(color);
    const mesh = createPart(new THREE.BoxGeometry(size[0], size[1], size[2]), material);
    mesh.userData.material = material;
    return mesh;
  }

  function createDamageTextSprite(text, color = '#fff3b0') {
    const canvas = document.createElement('canvas');
    canvas.width = 192;
    canvas.height = 96;
    const ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.font = '700 44px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.lineWidth = 8;
    ctx.strokeStyle = 'rgba(15, 23, 42, 0.9)';
    ctx.strokeText(text, 96, 48);
    ctx.fillStyle = color;
    ctx.fillText(text, 96, 48);
    const texture = new THREE.CanvasTexture(canvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    const material = new THREE.SpriteMaterial({ map: texture, transparent: true, depthWrite: false });
    const sprite = new THREE.Sprite(material);
    sprite.scale.set(0.95, 0.48, 1);
    sprite.userData.dispose = () => {
      texture.dispose();
      material.dispose();
    };
    return sprite;
  }

  function createTextSprite(text, opts = {}) {
    const canvas = document.createElement('canvas');
    canvas.width = opts.width || 256;
    canvas.height = opts.height || 128;
    const ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.font = opts.font || '800 72px "Microsoft YaHei", "Segoe UI", sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.lineWidth = opts.strokeWidth ?? 10;
    ctx.strokeStyle = opts.stroke || 'rgba(12, 10, 18, 0.86)';
    ctx.fillStyle = opts.color || '#ffd166';
    ctx.shadowColor = opts.shadow || 'rgba(255, 209, 102, 0.55)';
    ctx.shadowBlur = opts.shadowBlur ?? 18;
    ctx.strokeText(text, canvas.width / 2, canvas.height / 2);
    ctx.fillText(text, canvas.width / 2, canvas.height / 2);
    const texture = new THREE.CanvasTexture(canvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    const material = new THREE.SpriteMaterial({
      map: texture,
      transparent: true,
      depthWrite: false,
      opacity: opts.opacity ?? 1,
    });
    const sprite = new THREE.Sprite(material);
    sprite.scale.set(opts.scaleX || 1.4, opts.scaleY || 0.7, 1);
    sprite.userData.dispose = () => {
      texture.dispose();
      material.dispose();
    };
    return sprite;
  }

  function createSlashTexture() {
    const canvas = document.createElement('canvas');
    canvas.width = 512;
    canvas.height = 128;
    const ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const glow = ctx.createLinearGradient(24, 0, canvas.width - 28, 0);
    glow.addColorStop(0, 'rgba(42, 214, 255, 0)');
    glow.addColorStop(0.18, 'rgba(42, 214, 255, 0.18)');
    glow.addColorStop(0.52, 'rgba(255, 255, 255, 0.62)');
    glow.addColorStop(0.84, 'rgba(42, 214, 255, 0.20)');
    glow.addColorStop(1, 'rgba(42, 214, 255, 0)');
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.strokeStyle = glow;
    ctx.shadowColor = 'rgba(42, 214, 255, 0.72)';
    ctx.shadowBlur = 18;
    ctx.lineWidth = 42;
    ctx.beginPath();
    ctx.moveTo(34, 96);
    ctx.quadraticCurveTo(250, 12, 484, 34);
    ctx.stroke();
    ctx.shadowBlur = 6;
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.72)';
    ctx.lineWidth = 10;
    ctx.beginPath();
    ctx.moveTo(84, 84);
    ctx.quadraticCurveTo(268, 28, 452, 42);
    ctx.stroke();
    const texture = new THREE.CanvasTexture(canvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    return texture;
  }

  function disposeObject(object) {
    object.traverse((node) => {
      if (node.userData.dispose) node.userData.dispose();
      if (node.geometry) node.geometry.dispose();
      if (node.material) {
        const materials = Array.isArray(node.material) ? node.material : [node.material];
        for (const material of materials) material.dispose?.();
      }
    });
  }

  function createArenaFloorTexture() {
    const canvas = document.createElement('canvas');
    canvas.width = 1024;
    canvas.height = 512;
    const ctx = canvas.getContext('2d');
    const bg = ctx.createLinearGradient(0, 0, 0, canvas.height);
    bg.addColorStop(0, '#18283b');
    bg.addColorStop(0.48, '#101722');
    bg.addColorStop(1, '#070a10');
    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    const leftGlow = ctx.createRadialGradient(160, 190, 20, 180, 230, 470);
    leftGlow.addColorStop(0, 'rgba(42, 183, 255, 0.38)');
    leftGlow.addColorStop(1, 'rgba(42, 183, 255, 0)');
    ctx.fillStyle = leftGlow;
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    const rightGlow = ctx.createRadialGradient(850, 200, 20, 820, 230, 470);
    rightGlow.addColorStop(0, 'rgba(255, 46, 214, 0.34)');
    rightGlow.addColorStop(1, 'rgba(255, 46, 214, 0)');
    ctx.fillStyle = rightGlow;
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    ctx.strokeStyle = 'rgba(255, 209, 102, 0.42)';
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.moveTo(canvas.width / 2, 26);
    ctx.lineTo(canvas.width / 2, canvas.height - 12);
    ctx.stroke();

    ctx.strokeStyle = 'rgba(255, 255, 255, 0.12)';
    ctx.lineWidth = 2;
    for (let i = 0; i < 7; i++) {
      const y = 80 + i * 58;
      ctx.beginPath();
      ctx.moveTo(30, y);
      ctx.lineTo(canvas.width - 30, y);
      ctx.stroke();
    }

    ctx.strokeStyle = 'rgba(255, 209, 102, 0.26)';
    ctx.lineWidth = 5;
    ctx.beginPath();
    ctx.moveTo(canvas.width / 2 - 128, canvas.height - 84);
    ctx.lineTo(canvas.width / 2, canvas.height - 132);
    ctx.lineTo(canvas.width / 2 + 128, canvas.height - 84);
    ctx.lineTo(canvas.width / 2 + 84, canvas.height - 34);
    ctx.lineTo(canvas.width / 2 - 84, canvas.height - 34);
    ctx.closePath();
    ctx.stroke();

    const texture = new THREE.CanvasTexture(canvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = THREE.ClampToEdgeWrapping;
    texture.wrapT = THREE.ClampToEdgeWrapping;
    return texture;
  }

  function createSlashTrail(color = 0x9be7ff) {
    const group = new THREE.Group();
    const outer = new THREE.Mesh(
      new THREE.PlaneGeometry(1.55, 0.30),
      new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity: 0.34,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        side: THREE.DoubleSide,
      })
    );
    const core = new THREE.Mesh(
      new THREE.PlaneGeometry(1.12, 0.075),
      new THREE.MeshBasicMaterial({
        color: 0xffffff,
        transparent: true,
        opacity: 0.74,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        side: THREE.DoubleSide,
      })
    );
    core.position.z = 0.01;
    group.add(outer, core);
    group.visible = false;
    group.userData.outer = outer;
    group.userData.core = core;
    return group;
  }

  function createFighterAura(color = 0x58c7ff) {
    const group = new THREE.Group();
    const ring = new THREE.Mesh(
      new THREE.RingGeometry(0.46, 0.52, 64),
      new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity: 0.24,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        side: THREE.DoubleSide,
      })
    );
    ring.rotation.x = -Math.PI / 2;
    const glow = new THREE.Mesh(
      new THREE.PlaneGeometry(1.15, 1.15),
      new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity: 0.09,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        side: THREE.DoubleSide,
      })
    );
    glow.rotation.x = -Math.PI / 2;
    glow.position.y = 0.004;
    group.add(glow, ring);
    group.userData.ring = ring;
    group.userData.glow = glow;
    return group;
  }

  function createArenaCombatEffects() {
    const group = new THREE.Group();
    const slashTexture = createSlashTexture();
    const slash = new THREE.Mesh(
      new THREE.PlaneGeometry(2.25, 0.48),
      new THREE.MeshBasicMaterial({
        map: slashTexture,
        color: 0x36d6ff,
        transparent: true,
        opacity: 0.20,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        side: THREE.DoubleSide,
      })
    );
    slash.rotation.z = -0.16;

    const shield = new THREE.Group();
    const shieldRing = new THREE.Mesh(
      new THREE.TorusGeometry(0.56, 0.018, 12, 96),
      new THREE.MeshBasicMaterial({
        color: 0xff39f6,
        transparent: true,
        opacity: 0.32,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
      })
    );
    const shieldCore = new THREE.Mesh(
      new THREE.CircleGeometry(0.52, 72),
      new THREE.MeshBasicMaterial({
        color: 0xff39f6,
        transparent: true,
        opacity: 0.045,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        side: THREE.DoubleSide,
      })
    );
    const glyph = createTextSprite('咒', {
      width: 128,
      height: 128,
      font: '900 74px "Microsoft YaHei", serif',
      color: '#ff9cff',
      stroke: 'rgba(38, 0, 48, 0.92)',
      strokeWidth: 7,
      opacity: 0.48,
      scaleX: 0.42,
      scaleY: 0.42,
    });
    glyph.position.z = 0.018;
    shield.add(shieldCore, shieldRing, glyph);

    group.add(slash, shield);
    group.userData.slash = slash;
    group.userData.shield = shield;
    group.userData.shieldRing = shieldRing;
    group.userData.shieldCore = shieldCore;
    group.userData.glyph = glyph;
    group.userData.dispose = () => {
      slashTexture.dispose();
    };
    return group;
  }

  function createHumanoidMesh(palette) {
    const root = new THREE.Group();
    const materials = {
      primary: createStylizedMaterial(palette.primary),
      accent: createStylizedMaterial(palette.accent),
      skin: createStylizedMaterial(palette.skin),
      dark: createStylizedMaterial(palette.dark),
      glow: createStylizedMaterial(palette.glow),
      scarf: createStylizedMaterial(palette.scarf || 0xf43f5e, { roughness: 0.72 }),
    };
    const parts = {};

    parts.hips = createPart(new THREE.BoxGeometry(0.62, 0.28, 0.42), materials.dark);
    parts.hips.position.set(0, 0.72, 0);
    root.add(parts.hips);

    parts.torso = createPart(new THREE.BoxGeometry(0.74, 0.92, 0.44), materials.primary);
    parts.torso.position.set(0, 1.14, 0);
    root.add(parts.torso);

    parts.chestGlow = createPart(new THREE.BoxGeometry(0.42, 0.08, 0.035), materials.glow);
    parts.chestGlow.position.set(0, 1.27, 0.238);
    root.add(parts.chestGlow);

    parts.neck = createPart(new THREE.BoxGeometry(0.22, 0.16, 0.2), materials.skin);
    parts.neck.position.set(0, 1.66, 0);
    root.add(parts.neck);

    parts.head = createPart(new THREE.SphereGeometry(0.32, 10, 8), materials.skin);
    parts.head.scale.set(0.92, 1.08, 0.9);
    parts.head.position.set(0, 1.94, 0.02);
    root.add(parts.head);

    parts.hair = createPart(new THREE.BoxGeometry(0.46, 0.14, 0.34), materials.dark);
    parts.hair.position.set(0, 2.19, 0.0);
    root.add(parts.hair);

    parts.face = createPart(new THREE.BoxGeometry(0.2, 0.055, 0.035), materials.dark);
    parts.face.position.set(0, 1.96, 0.295);
    root.add(parts.face);

    parts.leftEar = createPart(new THREE.ConeGeometry(0.13, 0.34, 3), materials.dark);
    parts.leftEar.position.set(-0.18, 2.34, 0.02);
    parts.leftEar.rotation.z = 0.26;
    root.add(parts.leftEar);

    parts.rightEar = createPart(new THREE.ConeGeometry(0.13, 0.34, 3), materials.dark);
    parts.rightEar.position.set(0.18, 2.34, 0.02);
    parts.rightEar.rotation.z = -0.26;
    root.add(parts.rightEar);

    parts.visor = createPart(new THREE.BoxGeometry(0.34, 0.055, 0.045), materials.glow);
    parts.visor.position.set(0, 2.02, 0.32);
    root.add(parts.visor);

    parts.scarf = new THREE.Group();
    parts.scarf.position.set(-0.24, 1.58, 0.18);
    const scarfA = createPart(new THREE.BoxGeometry(0.13, 0.36, 0.055), materials.scarf);
    scarfA.position.set(0, -0.18, 0);
    const scarfB = createPart(new THREE.BoxGeometry(0.10, 0.28, 0.05), materials.scarf);
    scarfB.position.set(-0.04, -0.48, 0.02);
    parts.scarf.add(scarfA, scarfB);
    root.add(parts.scarf);

    parts.backpack = createPart(new THREE.BoxGeometry(0.44, 0.18, 0.52), materials.dark);
    parts.backpack.position.set(0, 1.18, -0.31);
    root.add(parts.backpack);

    function limbGroup(name, side) {
      const group = new THREE.Group();
      group.position.set(side * 0.52, 1.48, 0);
      const upper = createPart(new THREE.BoxGeometry(0.2, 0.58, 0.22), materials.accent);
      upper.position.set(0, -0.29, 0);
      const lower = createPart(new THREE.BoxGeometry(0.18, 0.54, 0.2), materials.skin);
      lower.position.set(0, -0.83, 0);
      const fist = createPart(new THREE.BoxGeometry(0.25, 0.2, 0.24), materials.glow);
      fist.position.set(0, -1.17, 0.04);
      group.add(upper, lower, fist);
      root.add(group);
      parts[`${name}Arm`] = group;
      parts[`${name}Fist`] = fist;
    }

    function legGroup(name, side) {
      const group = new THREE.Group();
      group.position.set(side * 0.22, 0.68, 0);
      const thigh = createPart(new THREE.BoxGeometry(0.24, 0.58, 0.24), materials.primary);
      thigh.position.set(0, -0.29, 0);
      const shin = createPart(new THREE.BoxGeometry(0.22, 0.5, 0.22), materials.dark);
      shin.position.set(0, -0.82, 0);
      const foot = createPart(new THREE.BoxGeometry(0.34, 0.16, 0.5), materials.glow);
      foot.position.set(side * 0.02, -1.12, 0.12);
      group.add(thigh, shin, foot);
      root.add(group);
      parts[`${name}Leg`] = group;
      parts[`${name}Foot`] = foot;
    }

    limbGroup('left', -1);
    limbGroup('right', 1);
    legGroup('left', -1);
    legGroup('right', 1);

    parts.tail = new THREE.Group();
    parts.tail.position.set(0, 0.88, -0.22);
    for (let i = 0; i < 3; i++) {
      const segment = createPart(new THREE.CapsuleGeometry(0.055 - i * 0.008, 0.22, 4, 10), materials.dark);
      segment.position.set(0.08 * i, 0.13 + i * 0.16, -0.03 * i);
      segment.rotation.z = 0.42 + i * 0.22;
      segment.rotation.x = 0.48;
      parts.tail.add(segment);
    }
    root.add(parts.tail);

    parts.weapon = new THREE.Group();
    const blade = createPart(new THREE.BoxGeometry(0.12, 0.88, 0.08), materials.glow);
    blade.position.set(0, -0.38, 0.04);
    const guard = createPart(new THREE.BoxGeometry(0.46, 0.08, 0.08), materials.dark);
    guard.position.set(0, 0.08, 0.04);
    parts.weapon.add(blade, guard);
    parts.weapon.position.set(0.7, 0.64, 0.1);
    parts.weapon.rotation.z = -0.62;
    parts.rightArm.add(parts.weapon);

    root.userData.parts = parts;
    root.userData.materials = materials;
    root.userData.basePalette = palette;
    return root;
  }

  function resetHumanoidPose(parts) {
    if (!parts) return;
    for (const part of Object.values(parts)) {
      part.rotation.set(0, 0, 0);
      part.scale.set(1, 1, 1);
    }
    parts.torso.position.y = 1.14;
    parts.head.position.y = 1.94;
    parts.leftArm.position.set(-0.52, 1.48, 0);
    parts.rightArm.position.set(0.52, 1.48, 0);
    parts.leftLeg.position.set(-0.22, 0.68, 0);
    parts.rightLeg.position.set(0.22, 0.68, 0);
    parts.leftFist.position.z = 0.04;
    parts.rightFist.position.z = 0.04;
    parts.leftEar.rotation.set(0, 0, 0.26);
    parts.rightEar.rotation.set(0, 0, -0.26);
    parts.scarf.rotation.set(0, 0, 0);
    parts.tail.rotation.set(0, 0, 0);
    if (parts.weapon) {
      parts.weapon.position.set(0.7, 0.64, 0.1);
      parts.weapon.rotation.set(0, 0, -0.62);
      parts.weapon.scale.set(1, 1, 1);
    }
  }

  function applyHumanoidPose(fighter, now) {
    const parts = fighter.mesh?.userData.parts;
    const materials = fighter.mesh?.userData.materials;
    if (!parts || !materials) return false;
    resetHumanoidPose(parts);

    const t = now * 0.008;
    const moving = Math.abs(fighter.vel.x) > 0.12 && fighter.onGround && fighter.canAct();
    const walk = moving ? Math.sin(t) : 0;
    const airborne = !fighter.onGround;
    const attackProgress = fighter.attack ? clamp(fighter.stateMs / fighter.attack.data.duration, 0, 1) : 0;

    parts.torso.position.y += moving ? Math.abs(walk) * 0.035 : 0;
    parts.head.position.y += moving ? Math.abs(walk) * 0.025 : 0;
    parts.leftEar.rotation.z += moving ? walk * 0.08 : Math.sin(t * 0.45) * 0.025;
    parts.rightEar.rotation.z -= moving ? walk * 0.08 : Math.sin(t * 0.45) * 0.025;
    parts.tail.rotation.z = moving ? -walk * 0.18 : Math.sin(t * 0.38) * 0.12;
    parts.scarf.rotation.z = moving ? walk * 0.12 : Math.sin(t * 0.42) * 0.06;
    parts.leftArm.rotation.x = moving ? walk * 0.42 : -0.08;
    parts.rightArm.rotation.x = moving ? -walk * 0.42 : -0.08;
    parts.leftLeg.rotation.x = moving ? -walk * 0.46 : 0.05;
    parts.rightLeg.rotation.x = moving ? walk * 0.46 : -0.05;

    if (airborne) {
      parts.leftArm.rotation.x = -0.85;
      parts.rightArm.rotation.x = -0.72;
      parts.leftLeg.rotation.x = 0.32;
      parts.rightLeg.rotation.x = -0.22;
      parts.torso.rotation.x = -0.08;
    }

    if (fighter.guardHeld || fighter.state === 'blockstun') {
      parts.leftArm.rotation.x = -1.18;
      parts.rightArm.rotation.x = -1.18;
      parts.leftArm.rotation.z = -0.26;
      parts.rightArm.rotation.z = 0.26;
      parts.leftArm.position.z = 0.16;
      parts.rightArm.position.z = 0.16;
      parts.torso.rotation.x = 0.12;
    }

    if (fighter.state === 'attack') {
      const windup = attackProgress < 0.34;
      const strike = fighter.isAttackActive();
      const recover = attackProgress > 0.72;
      const strikeT = clamp((attackProgress - 0.20) / 0.42, 0, 1);
      const snap = Math.sin(strikeT * Math.PI);
      const weaponArm = parts.rightArm;
      const offArm = parts.leftArm;
      weaponArm.rotation.x = windup ? 0.92 : strike ? -1.74 + snap * -0.34 : recover ? -0.42 : -1.06;
      weaponArm.rotation.z = windup ? -0.54 : strike ? 0.66 + snap * 0.42 : -0.18;
      weaponArm.rotation.y = strike ? -0.34 * fighter.facing : 0;
      weaponArm.position.z = strike ? 0.42 + snap * 0.18 : 0.12;
      weaponArm.position.x += strike ? fighter.facing * 0.06 : 0;
      offArm.rotation.x = windup ? -0.18 : strike ? 0.34 : 0.14;
      offArm.rotation.z = fighter.facing > 0 ? -0.22 : 0.22;
      parts.torso.rotation.y = fighter.facing > 0 ? -0.12 : 0.12;
      parts.torso.rotation.x = strike ? -0.08 : 0.04;
      parts.head.rotation.y = fighter.facing > 0 ? -0.08 : 0.08;
      parts.tail.rotation.z = strike ? -fighter.facing * 0.45 : fighter.facing * 0.16;
      parts.scarf.rotation.z = strike ? fighter.facing * 0.38 : -fighter.facing * 0.12;
      if (parts.weapon) {
        const kind = fighter.attack?.data.kind || '';
        const heavy = kind === 'spell' || kind === 'dash' || kind === 'heavy';
        parts.weapon.position.set(0.72 + snap * 0.10, 0.60 - snap * 0.08, 0.10 + snap * 0.16);
        parts.weapon.scale.y = heavy ? 1.62 : 1.20;
        parts.weapon.scale.x = heavy ? 1.18 : 1.06;
        parts.weapon.rotation.x = windup ? -0.38 : strike ? 0.54 + snap * 0.28 : -0.16;
        parts.weapon.rotation.y = strike ? -fighter.facing * 0.42 : 0;
        parts.weapon.rotation.z = windup ? -1.42 : strike ? 0.92 + snap * 0.72 : -0.36;
      }
    }

    if (fighter.state === 'hurt') {
      parts.torso.rotation.x = 0.34;
      parts.head.rotation.x = 0.28;
      parts.leftArm.rotation.x = 0.62;
      parts.rightArm.rotation.x = 0.62;
      parts.leftEar.rotation.z = -0.08;
      parts.rightEar.rotation.z = 0.08;
      parts.tail.rotation.z = fighter.facing * 0.42;
    }

    if (fighter.state === 'dead') {
      fighter.mesh.rotation.z = fighter.facing > 0 ? -1.22 : 1.22;
      parts.leftArm.rotation.x = 0.8;
      parts.rightArm.rotation.x = 0.8;
    } else {
      fighter.mesh.rotation.z = 0;
    }

    if (fighter.state === 'win') {
      parts.leftArm.rotation.x = -2.2;
      parts.rightArm.rotation.x = -2.2;
      parts.head.position.y += Math.sin(t * 0.8) * 0.04;
    }

    const base = fighter.mesh.userData.basePalette;
    materials.primary.color.setHex(fighter.state === 'hurt' ? 0xffffff : base.primary);
    materials.accent.color.setHex(fighter.state === 'attack' && fighter.isAttackActive() ? 0xfff3b0 : base.accent);
    materials.glow.color.setHex(fighter.state === 'blockstun' ? 0x9be7ff : base.glow);
    return true;
  }

  class ModelAnimator {
    constructor(root, clips = []) {
      this.root = root;
      this.sourceClips = clips;
      this.mixer = clips.length ? new THREE.AnimationMixer(root) : null;
      this.actions = new Map();
      this.current = null;
      this.currentKey = null;
      this.clipNames = clips.map((clip) => clip.name);
      for (const clip of this.mergeNodeClips(clips)) {
        const key = this.keyForClip(clip.name);
        if (!key || this.actions.has(key) || !this.mixer) continue;
        this.actions.set(key, this.mixer.clipAction(clip));
      }
    }

    mergeNodeClips(clips) {
      const buckets = new Map();
      for (const clip of clips) {
        const key = this.keyForClip(clip.name);
        if (!key) continue;
        if (!buckets.has(key)) buckets.set(key, []);
        buckets.get(key).push(clip);
      }
      const merged = [];
      for (const [key, group] of buckets) {
        const tracks = group.flatMap((clip) => clip.tracks || []);
        const duration = group.reduce((max, clip) => Math.max(max, clip.duration || 0), 0);
        if (tracks.length) merged.push(new THREE.AnimationClip(key, duration, tracks));
      }
      return merged;
    }

    keyForClip(name) {
      const n = String(name || '').toLowerCase();
      if (n.includes('idle')) return 'idle';
      if (n.includes('run') || n.includes('walk')) return 'run';
      if (n.includes('jump')) return 'jump';
      if (n.includes('heavy') || n.includes('kick')) return 'heavy';
      if (n.includes('light') || n.includes('punch') || n.includes('attack')) return 'light';
      if (n.includes('guard') || n.includes('block')) return 'guard';
      if (n.includes('hurt') || n.includes('hit')) return 'hurt';
      if (n.includes('dead') || n.includes('ko')) return 'dead';
      if (n.includes('win') || n.includes('victory')) return 'win';
      return null;
    }

    keyForFighter(fighter) {
      if (fighter.state === 'dead') return 'dead';
      if (fighter.state === 'win') return 'win';
      if (fighter.state === 'hurt' || fighter.state === 'blockstun') return fighter.state === 'blockstun' ? 'guard' : 'hurt';
      if (fighter.state === 'attack') return fighter.attack?.data.kind === 'heavy' ? 'heavy' : 'light';
      if (fighter.guardHeld) return 'guard';
      if (!fighter.onGround) return 'jump';
      if (Math.abs(fighter.vel.x) > 0.12 && fighter.canAct()) return 'run';
      return 'idle';
    }

    play(key) {
      if (!this.mixer) return false;
      const action = this.actions.get(key) || this.actions.get('idle');
      if (!action) return false;
      if (this.current === action) return true;
      const isAttack = key === 'light' || key === 'heavy';
      const wasAttack = this.currentKey === 'light' || this.currentKey === 'heavy';
      action.enabled = true;
      if (isAttack || key === 'hurt') action.reset();
      const fade = isAttack || wasAttack || key === 'hurt' ? 0.035 : 0.10;
      action.fadeIn(fade);
      if (this.current) this.current.fadeOut(fade);
      this.current = action;
      this.currentKey = key;
      action.play();
      return true;
    }

    update(fighter, dtMs) {
      const key = this.keyForFighter(fighter);
      this.play(key);
      if (this.mixer) this.mixer.update(Math.max(0, dtMs) / 1000);
    }
  }

  class ArenaModelLoader {
    constructor() {
      this.loader = new GLTFLoader();
      this.cache = new Map();
    }

    load(path) {
      if (!path) return Promise.resolve(null);
      if (!this.cache.has(path)) {
        this.cache.set(path, new Promise((resolve) => {
          this.loader.load(
            path,
            (gltf) => resolve(gltf),
            undefined,
            (error) => {
              if (typeof console !== 'undefined') {
                console.warn('arena model load failed', path, error?.message || error);
              }
              resolve(null);
            }
          );
        }));
      }
      return this.cache.get(path);
    }
  }

  function normalizeImportedModel(root) {
    const box = new THREE.Box3().setFromObject(root);
    const size = new THREE.Vector3();
    const center = new THREE.Vector3();
    box.getSize(size);
    box.getCenter(center);
    const height = size.y || 1;
    const scale = clamp(2.72 / height, 0.15, 5.2);
    const stylizedScale = new THREE.Vector3(1.16, 0.84, 1.08);
    root.scale.multiplyScalar(scale);
    root.scale.multiply(stylizedScale);
    root.position.x -= center.x * scale;
    root.position.z -= center.z * scale;
    root.position.y -= box.min.y * scale;
    root.traverse((node) => {
      if (!node.isMesh) return;
      node.castShadow = true;
      node.receiveShadow = true;
      if (node.material) {
        const materials = Array.isArray(node.material) ? node.material : [node.material];
        for (const material of materials) {
          if ('roughness' in material) material.roughness = Math.min(material.roughness ?? 0.48, 0.42);
          if ('metalness' in material) material.metalness = Math.max(material.metalness ?? 0.04, 0.05);
          if ('emissiveIntensity' in material) material.emissiveIntensity = Math.max(material.emissiveIntensity || 0, 0.02);
          if (material.color) material.userData.baseColor = material.color.clone();
          if (material.emissive) material.userData.baseEmissive = material.emissive.clone();
          if ('emissiveIntensity' in material) material.userData.baseEmissiveIntensity = material.emissiveIntensity || 0;
          material.needsUpdate = true;
        }
      }
    });
  }

  function collectImportedRigNodes(root) {
    const rig = { weapons: [], offhands: [], rightArms: [], leftArms: [], reactive: [] };
    root.traverse((node) => {
      if (!node || !node.name) return;
      const name = node.name.toLowerCase();
      const hasTransform = node.rotation && node.position && node.scale;
      if (!hasTransform) return;
      if (!node.userData.arenaBaseTransform) {
        node.userData.arenaBaseTransform = {
          position: node.position.clone(),
          rotation: node.rotation.clone(),
          scale: node.scale.clone(),
        };
      }
      if (name === 'weapon' || name.endsWith('_weapon') || name.includes('weaponpivot')) rig.weapons.push(node);
      else if (name === 'offhanddagger' || name.includes('offhand') || name.includes('wardpivot')) rig.offhands.push(node);
      else if (name.includes('r_shoulder') || name.includes('r_elbow')) rig.rightArms.push(node);
      else if (name.includes('l_shoulder') || name.includes('l_elbow')) rig.leftArms.push(node);
      if (
        name.includes('scarf') ||
        name.includes('cape') ||
        name.includes('tail') ||
        name.includes('charm') ||
        name.includes('talisman')
      ) {
        rig.reactive.push(node);
      }
    });
    return rig;
  }

  class ArenaRenderer {
    constructor(canvas, engine) {
      document.getElementById('arenaCanvas')?.remove();
      this.canvas = document.createElement('canvas');
      this.canvas.id = 'arenaCanvas';
      this.canvas.style.position = 'fixed';
      this.canvas.style.inset = '0';
      this.canvas.style.width = '100vw';
      this.canvas.style.height = '100vh';
      this.canvas.style.pointerEvents = 'none';
      this.canvas.style.zIndex = '0';
      canvas.style.opacity = '0';
      canvas.parentElement.insertBefore(this.canvas, canvas);
      this.engine = engine;
      this.renderer = new THREE.WebGLRenderer({
        canvas: this.canvas,
        alpha: false,
        antialias: true,
        powerPreference: 'high-performance',
      });
      this.renderer.setClearColor(0x05070d, 1);
      this.renderer.outputColorSpace = THREE.SRGBColorSpace;
      this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
      this.renderer.toneMappingExposure = 1.26;
      this.renderer.shadowMap.enabled = true;
      this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
      this.scene = new THREE.Scene();
      this.camera = new THREE.PerspectiveCamera(40, 16 / 9, 0.1, 100);
      this.camera.position.set(0, 2.28, 6.55);
      this.camera.lookAt(0, 0.98, 0.18);
      this.debugBoxes = [];
      this.effects = [];
      this.seenHitSeq = 0;
      this.stageAccents = [];
      this.fighterAuras = new Map();
      this.combatPoseEffects = null;
      this.displayZ = new Map();
      this.modelLoader = new ArenaModelLoader();
      this.textureLoader = new THREE.TextureLoader();
      this.buildScene();
    }

    buildScene() {
      this.scene.fog = new THREE.FogExp2(0x05070d, 0.035);
      const ambient = new THREE.HemisphereLight(0xb8e6ff, 0x2b102b, 0.84);
      this.scene.add(ambient);
      const key = new THREE.DirectionalLight(0xffffff, 3.35);
      key.position.set(-3.4, 5.6, 3.6);
      key.castShadow = true;
      key.shadow.mapSize.set(2048, 2048);
      key.shadow.camera.near = 1;
      key.shadow.camera.far = 18;
      key.shadow.camera.left = -8;
      key.shadow.camera.right = 8;
      key.shadow.camera.top = 8;
      key.shadow.camera.bottom = -4;
      this.scene.add(key);
      const rim = new THREE.DirectionalLight(0x9be7ff, 1.9);
      rim.position.set(4.4, 3.6, -4.6);
      this.scene.add(rim);
      const blueLanternLight = new THREE.PointLight(0x19b9ff, 3.15, 9.5);
      blueLanternLight.position.set(-4.9, 1.75, 1.05);
      const magentaLanternLight = new THREE.PointLight(0xff2bd6, 3.0, 9.5);
      magentaLanternLight.position.set(4.9, 1.75, 1.05);
      const centerWarmLight = new THREE.PointLight(0xffb84d, 1.15, 10);
      centerWarmLight.position.set(0, 2.35, 2.7);
      this.scene.add(blueLanternLight, magentaLanternLight, centerWarmLight);
      this.createImageBackdrop();
      this.createArenaBackdrop();
      const floorTexture = createArenaFloorTexture();
      const floor = new THREE.Mesh(
        new THREE.PlaneGeometry(13.4, 4.15),
        new THREE.MeshBasicMaterial({
          map: floorTexture,
          transparent: true,
          opacity: 0.74,
          depthWrite: false,
          side: THREE.DoubleSide,
        })
      );
      floor.rotation.x = -Math.PI / 2;
      floor.position.set(0, 0.004, 0.95);
      floor.userData.dispose = () => floorTexture.dispose();
      this.scene.add(floor);
      const contactShadow = new THREE.Mesh(
        new THREE.PlaneGeometry(7.6, 1.24),
        new THREE.MeshBasicMaterial({ color: 0x020617, transparent: true, opacity: 0.34, depthWrite: false })
      );
      contactShadow.rotation.x = -Math.PI / 2;
      contactShadow.position.set(0, 0.014, 0.30);
      this.scene.add(contactShadow);
      this.engine.assist.mesh = createBoxMesh(0xffd166, [0.52, 0.52, 0.52]);
      this.scene.add(this.engine.assist.mesh);
      this.loadOptionalModel(this.engine.player, this.engine.config?.config?.player_model || `${MODEL_BASE_URL}player.glb`);
      this.loadOptionalModel(this.engine.enemy, this.engine.config?.config?.enemy_model || `${MODEL_BASE_URL}enemy.glb`);
      this.ensureFighterAura(this.engine.player, 0x4cc9f0);
      this.ensureFighterAura(this.engine.enemy, 0xf72585);
      this.combatPoseEffects = createArenaCombatEffects();
      this.scene.add(this.combatPoseEffects);
    }

    createImageBackdrop() {
      const texture = this.textureLoader.load(ARENA_BACKGROUND_URL);
      texture.colorSpace = THREE.SRGBColorSpace;
      texture.anisotropy = Math.min(8, this.renderer.capabilities.getMaxAnisotropy?.() || 1);
      const backdrop = new THREE.Mesh(
        new THREE.PlaneGeometry(18.9, 9.45),
        new THREE.MeshBasicMaterial({
          map: texture,
          fog: false,
          depthWrite: false,
        })
      );
      backdrop.position.set(0, 1.30, -3.72);
      this.scene.add(backdrop);
      this.imageBackdrop = backdrop;
    }

    createArenaBackdrop() {
      const gold = new THREE.MeshStandardMaterial({
        color: 0xb98737,
        roughness: 0.36,
        metalness: 0.38,
        emissive: 0x2a1705,
        emissiveIntensity: 0.2,
        transparent: true,
        opacity: 0.72,
      });
      const darkGold = new THREE.MeshStandardMaterial({ color: 0x33220f, roughness: 0.55, metalness: 0.25 });
      const centerRing = new THREE.Mesh(new THREE.TorusGeometry(0.56, 0.026, 12, 96), gold);
      centerRing.position.set(0, 1.82, -2.02);
      this.scene.add(centerRing);
      const soul = createTextSprite('魂', {
        width: 256,
        height: 256,
        font: '900 128px "Microsoft YaHei", serif',
        color: '#ffc864',
        stroke: 'rgba(24, 14, 6, 0.9)',
        opacity: 0.58,
        scaleX: 0.66,
        scaleY: 0.66,
      });
      soul.position.set(0, 1.83, -1.96);
      this.scene.add(soul);
      this.stageAccents.push(soul);

      for (const side of [-1, 1]) {
        const color = side < 0 ? 0x1bb8ff : 0xff2bcf;
        const lanternMat = new THREE.MeshStandardMaterial({
          color,
          emissive: color,
          emissiveIntensity: 0.75,
          roughness: 0.42,
          metalness: 0.05,
          transparent: true,
          opacity: 0.92,
        });
        const frameMat = side < 0 ? gold : darkGold;
        const x = side * 6.25;
        const lantern = new THREE.Group();
        const body = new THREE.Mesh(new THREE.CylinderGeometry(0.25, 0.31, 0.78, 8), lanternMat);
        body.scale.x = 0.82;
        const capTop = new THREE.Mesh(new THREE.CylinderGeometry(0.36, 0.32, 0.07, 8), frameMat);
        capTop.position.y = 0.43;
        const capLow = new THREE.Mesh(new THREE.CylinderGeometry(0.32, 0.36, 0.07, 8), frameMat);
        capLow.position.y = -0.43;
        lantern.add(body, capTop, capLow);
        const glyph = createTextSprite(side < 0 ? '封' : '煞', {
          width: 128,
          height: 192,
          font: '900 82px "Microsoft YaHei", serif',
          color: side < 0 ? '#082b44' : '#3b0730',
          stroke: 'rgba(255,255,255,0.28)',
          strokeWidth: 4,
          shadowBlur: 0,
          scaleX: 0.32,
          scaleY: 0.48,
        });
        glyph.position.set(0, 0, 0.31);
        lantern.add(glyph);
        lantern.position.set(x, 1.54, -1.93);
        this.scene.add(lantern);
        this.stageAccents.push(lantern);

        for (let i = 0; i < 3; i++) {
          const tag = createTextSprite(i === 1 ? '妖' : '灵', {
            width: 96,
            height: 192,
            font: '800 70px "Microsoft YaHei", serif',
            color: '#f8d37b',
            stroke: 'rgba(28, 16, 8, 0.85)',
            strokeWidth: 6,
            scaleX: 0.22,
            scaleY: 0.48,
            opacity: 0.52,
          });
          tag.position.set(side * (1.92 + i * 0.38), 2.05 - i * 0.18, -1.95);
          tag.rotation.z = side * (0.08 + i * 0.02);
          this.scene.add(tag);
          this.stageAccents.push(tag);
        }
      }
    }

    ensureFighterAura(fighter, color) {
      if (this.fighterAuras.has(fighter.id)) return this.fighterAuras.get(fighter.id);
      const aura = createFighterAura(color);
      aura.position.set(fighter.pos.x, 0.028, fighter.pos.z);
      this.scene.add(aura);
      this.fighterAuras.set(fighter.id, aura);
      return aura;
    }

    async loadOptionalModel(fighter, path) {
      const gltf = await this.modelLoader.load(path);
      if (!gltf || !this.scene) {
        if (this.engine.host?.log) this.engine.host.log(`arena model unavailable ${path}`);
        return;
      }
      const modelRoot = gltf.scene.clone(true);
      normalizeImportedModel(modelRoot);
      const holder = new THREE.Group();
      holder.add(modelRoot);
      holder.scale.setScalar(1.12);
      holder.userData.animator = new ModelAnimator(holder, gltf.animations || []);
      holder.userData.modelPath = path;
      holder.userData.rigNodes = collectImportedRigNodes(holder);
      const trail = createSlashTrail(fighter.isPlayer ? 0x7dd3fc : 0xf0abfc);
      holder.add(trail);
      holder.userData.slashTrail = trail;
      if (this.engine.host?.log) {
        this.engine.host.log(`arena model loaded ${path} clips=${holder.userData.animator.clipNames.join(',') || '<none>'}`);
      }
      const oldMesh = fighter.mesh;
      if (oldMesh) {
        holder.position.copy(oldMesh.position);
        holder.rotation.copy(oldMesh.rotation);
        this.scene.remove(oldMesh);
      } else {
        holder.position.set(fighter.pos.x, fighter.pos.y, fighter.pos.z);
        holder.rotation.y = fighter.facing < 0 ? Math.PI : 0;
      }
      fighter.mesh = holder;
      this.scene.add(holder);
    }

    resize(width, height) {
      this.renderer.setSize(width, height, false);
      this.camera.aspect = width / Math.max(1, height);
      this.camera.updateProjectionMatrix();
    }

    render() {
      const midpoint = (this.engine.player.pos.x + this.engine.enemy.pos.x) / 2;
      this.spawnHitEffects();
      const hitstopT = this.engine.combat.hitstopMs > 0 ? clamp(this.engine.combat.hitstopMs / HITSTOP_MS, 0, 1) : 0;
      const shakeX = hitstopT ? Math.sin(this.engine.timeMs * 0.52) * 0.09 * hitstopT : 0;
      const shakeY = hitstopT ? Math.sin(this.engine.timeMs * 0.73) * 0.045 * hitstopT : 0;
      const shakeZ = hitstopT ? Math.cos(this.engine.timeMs * 0.61) * 0.10 * hitstopT : 0;
      const spacing = Math.abs(this.engine.player.pos.x - this.engine.enemy.pos.x);
      const closeT = clamp((2.6 - spacing) / 2.1, 0, 1);
      const targetZ = 6.05 + closeT * 0.14;
      const targetY = 2.18 + closeT * 0.06;
      this.camera.position.x += (clamp(midpoint, -3.8, 3.8) + shakeX - this.camera.position.x) * 0.08;
      this.camera.position.y += (targetY + shakeY - this.camera.position.y) * 0.05;
      this.camera.position.z += (targetZ + shakeZ - this.camera.position.z) * 0.05;
      this.camera.lookAt(this.camera.position.x, 1.02 + closeT * 0.06, 0.12 + closeT * -0.06);
      const dtMs = this.engine.frameDtMs || this.engine.lastDtMs || 16;
      this.updateFighterMesh(this.engine.player, 0x58c7ff, dtMs);
      this.updateFighterMesh(this.engine.enemy, 0xff6b6b, dtMs);
      this.updateAssistMesh();
      this.updateFighterAuras();
      this.updateCombatPoseEffects();
      this.updateStageAccents();
      this.updateEffects(dtMs);
      this.renderDebugBoxes();
      this.renderer.render(this.scene, this.camera);
    }

    updateStageAccents() {
      const pulse = 0.55 + Math.sin(this.engine.timeMs * 0.006) * 0.12;
      for (const accent of this.stageAccents) {
        if (!accent.material) continue;
        const min = accent.userData.pulseMin ?? 0.32;
        const max = accent.userData.pulseMax ?? 0.72;
        accent.material.opacity = clamp(pulse, min, max);
      }
    }

    updateFighterAuras() {
      for (const fighter of [this.engine.player, this.engine.enemy]) {
        const aura = this.fighterAuras.get(fighter.id);
        if (!aura) continue;
        const attackPulse = fighter.state === 'attack' ? 0.18 : 0;
        const hurtPulse = fighter.state === 'hurt' || fighter.state === 'blockstun' ? 0.22 : 0;
        const pulse = 0.94 + Math.sin(this.engine.timeMs * 0.008 + (fighter.isPlayer ? 0 : 1.4)) * 0.035;
        const displayZ = this.getDisplayZ(fighter);
        aura.position.set(fighter.pos.x, 0.028, displayZ);
        aura.rotation.z += 0.012 * fighter.facing;
        aura.scale.setScalar(pulse + attackPulse + hurtPulse);
        aura.userData.ring.material.opacity = fighter.onGround ? 0.18 + attackPulse + hurtPulse : 0.08;
        aura.userData.glow.material.opacity = fighter.state === 'attack' ? 0.16 : 0.075;
      }
    }

    updateCombatPoseEffects() {
      const effects = this.combatPoseEffects;
      if (!effects) return;
      const slash = effects.userData.slash;
      const shield = effects.userData.shield;
      const shieldRing = effects.userData.shieldRing;
      const shieldCore = effects.userData.shieldCore;
      const glyph = effects.userData.glyph;
      const player = this.engine.player;
      const enemy = this.engine.enemy;
      const pulse = 0.72 + Math.sin(this.engine.timeMs * 0.007) * 0.16;
      const showcase = player.state !== 'dead' && enemy.state !== 'dead';
      const playerStrike = player.state === 'attack' ? 1 : 0;
      const enemyGuard = enemy.state === 'guard' || enemy.state === 'blockstun' ? 1 : 0;
      slash.visible = playerStrike > 0 || showcase;
      shield.visible = enemyGuard > 0 || showcase;
      if (!slash.visible && !shield.visible) return;
      const idlePulse = showcase ? 0.055 : 0;
      slash.position.set(player.pos.x + 0.92, player.pos.y + 0.98, this.getDisplayZ(player) + 0.62);
      slash.rotation.y = -0.52;
      slash.rotation.z = -0.22 + Math.sin(this.engine.timeMs * 0.004) * 0.035;
      slash.scale.set(0.82 + playerStrike * 0.30, 0.84 + playerStrike * 0.20, 1);
      slash.material.opacity = (idlePulse + playerStrike * 0.38) * pulse;
      shield.position.set(enemy.pos.x - 0.72, enemy.pos.y + 1.18, this.getDisplayZ(enemy) + 0.66);
      shield.rotation.y = 0.54;
      shield.rotation.z += 0.012;
      shield.scale.setScalar(0.88 + enemyGuard * 0.16 + Math.sin(this.engine.timeMs * 0.006) * 0.02);
      shieldRing.material.opacity = 0.10 + enemyGuard * 0.34;
      shieldCore.material.opacity = 0.018 + enemyGuard * 0.085;
      glyph.material.opacity = 0.16 + enemyGuard * 0.26;
    }

    getDisplayZ(fighter) {
      return this.displayZ.get(fighter.id) ?? fighter.pos.z;
    }

    updateDisplayDepth(fighter) {
      const opponent = fighter === this.engine.player ? this.engine.enemy : this.engine.player;
      const spacing = Math.abs(this.engine.player.pos.x - this.engine.enemy.pos.x);
      const closeT = clamp((2.7 - spacing) / 2.0, 0, 1);
      const attackBias = fighter.state === 'attack' ? -0.10 : opponent.state === 'attack' ? 0.10 : 0;
      const side = fighter.isPlayer ? 1 : -1;
      const target = fighter.pos.z + side * (0.18 + closeT * 0.34) + attackBias;
      const prev = this.getDisplayZ(fighter);
      const next = prev + (target - prev) * 0.14;
      this.displayZ.set(fighter.id, next);
      return next;
    }

    spawnHitEffects() {
      if (this.seenHitSeq === this.engine.combat.lastHitSeq) return;
      this.seenHitSeq = this.engine.combat.lastHitSeq;
      const hit = this.engine.combat.lastHit;
      if (!hit) return;
      const defender = hit.defender === this.engine.player.id ? this.engine.player : this.engine.enemy;
      const color = hit.blocked ? 0x9be7ff : 0xfff3b0;
      const group = new THREE.Group();
      group.position.set(defender.pos.x - defender.facing * 0.42, defender.pos.y + 1.15, this.getDisplayZ(defender));
      const burst = new THREE.Mesh(
        new THREE.RingGeometry(hit.blocked ? 0.18 : 0.24, hit.blocked ? 0.42 : 0.62, 48),
        new THREE.MeshBasicMaterial({
          color: hit.blocked ? 0x9be7ff : 0xfff3b0,
          transparent: true,
          opacity: hit.blocked ? 0.62 : 0.82,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
          side: THREE.DoubleSide,
        })
      );
      burst.position.set(-defender.facing * 0.28, 0.04, 0.56);
      burst.rotation.set(Math.PI / 2, 0.15, hit.blocked ? 0.3 : -0.15);
      group.add(burst);
      const slash = new THREE.Mesh(
        new THREE.PlaneGeometry(hit.blocked ? 0.8 : 1.45, hit.blocked ? 0.22 : 0.34),
        new THREE.MeshBasicMaterial({
          color: hit.blocked ? 0x9be7ff : 0xffffff,
          transparent: true,
          opacity: hit.blocked ? 0.55 : 0.82,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
          side: THREE.DoubleSide,
        })
      );
      slash.position.set(-defender.facing * 0.18, 0.02, 0.52);
      slash.rotation.z = hit.data?.vfx === 'slash_up' ? 0.78 : hit.data?.vfx === 'slash_down' ? -0.62 : 0.18;
      group.add(slash);
      for (let i = 0; i < 10; i++) {
        const shard = createPart(
          new THREE.BoxGeometry(i % 2 ? 0.06 : 0.10, 0.035, 0.11),
          new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.9, blending: THREE.AdditiveBlending })
        );
        shard.position.set((i - 4.5) * 0.07, (i % 3) * 0.055 - 0.03, 0.03);
        shard.rotation.set(i * 0.4, i * 0.2, i * 0.7);
        group.add(shard);
      }
      const damageText = createDamageTextSprite(hit.blocked ? 'GUARD' : String(hit.damage || ''));
      damageText.position.set(0.18, 0.56, 0.65);
      group.add(damageText);
      group.userData.age = 0;
      group.userData.life = hit.blocked ? 220 : 360;
      group.userData.blocked = hit.blocked;
      group.userData.damageText = damageText;
      this.effects.push(group);
      this.scene.add(group);
    }

    updateEffects(dtMs) {
      const alive = [];
      for (const effect of this.effects) {
        effect.userData.age += dtMs;
        const t = clamp(effect.userData.age / effect.userData.life, 0, 1);
        effect.scale.setScalar(1 + t * (effect.userData.blocked ? 0.9 : 1.6));
        effect.rotation.z += 0.12;
        effect.position.y += dtMs * 0.00045;
        effect.traverse((node) => {
          if (node.material) node.material.opacity = Math.max(0, 0.9 * (1 - t));
        });
        if (t < 1) alive.push(effect);
        else {
          disposeObject(effect);
          this.scene.remove(effect);
        }
      }
      this.effects = alive;
    }

    updateFighterMesh(fighter, color, dtMs = 16) {
      const mesh = fighter.mesh;
      if (!mesh) return;
      const displayZ = this.updateDisplayDepth(fighter);
      mesh.position.set(fighter.pos.x, fighter.pos.y, displayZ);
      const presentationTurn = fighter.isPlayer ? -0.12 : 0.12;
      mesh.rotation.y = presentationTurn;
      this.updateSlashTrail(fighter, mesh);
      this.updateModelMaterialState(fighter, mesh);
      if (mesh.userData.animator) {
        mesh.userData.animator.update(fighter, dtMs);
        this.updateImportedModelPose(fighter, mesh);
        return;
      }
      if (applyHumanoidPose(fighter, this.engine.timeMs)) {
        return;
      }
      const material = mesh.userData.material;
      if (fighter.state === 'hurt') material.color.setHex(0xffffff);
      else if (fighter.state === 'guard' || fighter.state === 'blockstun') material.color.setHex(0x9be7ff);
      else if (fighter.state === 'attack') material.color.setHex(fighter.isAttackActive() ? 0xfff3b0 : color);
      else material.color.setHex(color);
      const sx = fighter.state === 'attack' ? 1.08 : 1;
      const sy = fighter.state === 'guard' ? 0.92 : 1;
      mesh.scale.set(sx, sy, 1);
    }

    updateModelMaterialState(fighter, mesh) {
      const attacking = fighter.state === 'attack' && fighter.isAttackActive();
      const guarded = fighter.state === 'guard' || fighter.state === 'blockstun';
      const hurt = fighter.state === 'hurt';
      const pulse = attacking ? 0.55 : guarded ? 0.35 : hurt ? 0.5 : 0;
      if (!pulse && mesh.userData.lastMaterialPulse === 0) return;
      mesh.userData.lastMaterialPulse = pulse;
      const tint = hurt ? new THREE.Color(0xffffff) : guarded ? new THREE.Color(0x9be7ff) : new THREE.Color(0xfff3b0);
      mesh.traverse((node) => {
        if (!node.material) return;
        const materials = Array.isArray(node.material) ? node.material : [node.material];
        for (const material of materials) {
          if (material.color && material.userData.baseColor) {
            material.color.copy(material.userData.baseColor).lerp(tint, pulse * 0.28);
          }
          if (material.emissive && material.userData.baseEmissive) {
            material.emissive.copy(material.userData.baseEmissive).lerp(tint, pulse * 0.22);
          }
          if ('emissiveIntensity' in material) {
            material.emissiveIntensity = (material.userData.baseEmissiveIntensity || 0) + pulse * 0.45;
          }
        }
      });
    }

    updateImportedModelPose(fighter, mesh) {
      const rig = mesh.userData.rigNodes;
      if (!rig) return;
      const animated = Boolean(mesh.userData.animator?.mixer);
      if (!animated) {
        for (const node of [...rig.weapons, ...rig.offhands, ...rig.rightArms, ...rig.leftArms, ...rig.reactive]) {
          const base = node.userData.arenaBaseTransform;
          if (!base) continue;
          node.position.copy(base.position);
          node.rotation.copy(base.rotation);
          node.scale.copy(base.scale);
        }
      }

      const now = this.engine.timeMs;
      for (const node of rig.reactive) {
        const name = node.name.toLowerCase();
        const phase = name.includes('tail') ? 0.7 : name.includes('cape') ? 1.3 : name.includes('charm') ? 1.9 : 0.2;
        node.rotation.z += Math.sin(now * 0.006 + phase) * 0.018;
      }

      if (fighter.state === 'guard' || fighter.state === 'blockstun') {
        for (const node of rig.leftArms) {
          node.rotation.x += -0.24;
          node.rotation.z += -0.12;
        }
        for (const node of rig.offhands) {
          node.rotation.z += -0.36;
          node.position.z += 0.08;
        }
      }

      if (fighter.state !== 'attack' || !fighter.attack) return;
      const data = fighter.attack.data;
      const progress = clamp(fighter.stateMs / Math.max(1, data.duration), 0, 1);
      const windup = clamp(progress / 0.24, 0, 1);
      const swingT = clamp((progress - 0.18) / 0.40, 0, 1);
      const snap = Math.sin(swingT * Math.PI);
      const recover = clamp((progress - 0.64) / 0.34, 0, 1);
      const heavy = data.kind === 'heavy' || data.kind === 'dash' || data.kind === 'spell';
      const lift = heavy ? 0.18 : 0.10;
      const sweep = data.vfx === 'slash_up' ? 1 : data.vfx === 'slash_down' ? -1 : 0.55;

      for (const node of rig.rightArms) {
        node.rotation.x += 0.24 * windup - (0.68 + lift) * snap + 0.18 * recover;
        node.rotation.y += -0.14 * fighter.facing * snap;
        node.rotation.z += -0.22 * windup + (0.44 + lift) * snap;
        node.position.z += 0.04 + 0.13 * snap;
      }
      for (const node of rig.leftArms) {
        node.rotation.x += 0.10 - 0.16 * snap;
        node.rotation.z += -0.16 * fighter.facing * snap;
      }
      for (const node of rig.weapons) {
        node.rotation.x += -0.30 * windup + (0.72 + lift) * snap - 0.12 * recover;
        node.rotation.y += -0.22 * fighter.facing * snap;
        node.rotation.z += -0.72 * windup + (1.32 + lift) * snap * sweep - 0.22 * recover;
        node.position.x += fighter.facing * 0.035 * snap;
        node.position.y += -0.055 * snap;
        node.position.z += 0.12 * snap;
        node.scale.y *= heavy ? 1.14 + 0.10 * snap : 1.06 + 0.06 * snap;
      }
      for (const node of rig.offhands) {
        node.rotation.z += -0.28 * fighter.facing * snap;
        node.position.z += 0.055 * snap;
      }
    }

    updateSlashTrail(fighter, mesh) {
      const trail = mesh.userData.slashTrail;
      if (!trail) return;
      const active = fighter.state === 'attack' && fighter.attack;
      if (!active) {
        trail.visible = false;
        return;
      }
      const data = fighter.attack.data;
      const progress = clamp(fighter.stateMs / Math.max(1, data.duration), 0, 1);
      const strike = progress > 0.22 && progress < 0.72;
      trail.visible = strike;
      if (!strike) return;
      const isHeavy = data.kind === 'heavy' || data.kind === 'spell' || data.kind === 'dash';
      const sweep = data.vfx === 'slash_up' ? 0.7 : data.vfx === 'slash_down' ? -0.72 : 0.08;
      trail.position.set(0.55 * fighter.facing, 1.24, 0.44);
      trail.rotation.set(0, -fighter.facing * 0.55, sweep + fighter.facing * (progress - 0.45) * 1.8);
      trail.scale.set(isHeavy ? 1.42 : 1.05, isHeavy ? 1.35 : 1.0, 1);
      const opacity = Math.sin(clamp((progress - 0.22) / 0.5, 0, 1) * Math.PI);
      trail.userData.outer.material.opacity = (isHeavy ? 0.48 : 0.34) * opacity;
      trail.userData.core.material.opacity = (isHeavy ? 0.9 : 0.68) * opacity;
    }

    updateAssistMesh() {
      const assist = this.engine.assist;
      const mesh = assist.mesh;
      if (!mesh) return;
      mesh.visible = assist.activeMs > 0;
      mesh.position.set(assist.pos.x, assist.pos.y, assist.pos.z + 0.45);
      mesh.rotation.y += 0.08;
    }

    renderDebugBoxes() {
      for (const box of this.debugBoxes) this.scene.remove(box);
      this.debugBoxes = [];
      if (!this.engine.debug) return;
      const boxes = [
        { box: this.engine.player.hurtbox(), color: 0x4cc9f0 },
        { box: this.engine.enemy.hurtbox(), color: 0xf72585 },
        { box: this.engine.player.hitbox(), color: 0xffd166 },
        { box: this.engine.enemy.hitbox(), color: 0xffd166 },
        { box: this.engine.assist.hitbox(), color: 0xffe066 },
      ];
      const visibleBoxes = boxes.filter((entry) => entry.box);
      for (const entry of visibleBoxes) {
        const geometry = new THREE.BoxGeometry(entry.box.w, entry.box.h, entry.box.d);
        const material = new THREE.MeshBasicMaterial({
          color: entry.color,
          transparent: true,
          opacity: 0.24,
          wireframe: true,
        });
        const mesh = new THREE.Mesh(geometry, material);
        mesh.position.set(entry.box.x, entry.box.y, entry.box.z);
        this.scene.add(mesh);
        this.debugBoxes.push(mesh);
      }
    }
  }

  class ArenaEngine {
    constructor(config, host = {}) {
      this.config = config;
      this.host = host;
      this.state = 'ready';
      this.score = 0;
      this.ended = false;
      this.debug = Boolean(config?.config?.debug_hitboxes);
      this.timeMs = 0;
      this.lastDtMs = 16;
      this.frameDtMs = 16;
      this.roundMs = 60_000;
      this.player = new Fighter('player', { name: 'You', isPlayer: true, x: -2.35, facing: 1 });
      this.enemy = new Fighter('dummy', { name: 'AI Dummy', x: 2.35, facing: -1, hp: 100, damageScale: 0.7 });
      this.assist = new BitCatAssist(this.player);
      this.ai = new FighterAI(this.enemy);
      this.combat = new ArenaCombat(this.player, this.enemy, this.assist);
      this.renderer = null;
      this.lastLoggedAssist = false;
    }

    getState() {
      return this.state;
    }

    hudText() {
      const assist = this.assist.cooldownMs > 0 ? `${Math.ceil(this.assist.cooldownMs / 1000)}s` : 'ready';
      const combo = this.player.combo > 1 ? ` x${this.player.combo}` : '';
      const ai = this.ai.intent ? ` AI ${this.ai.intent}` : '';
      return `YOU ${this.player.hp}  CPU ${this.enemy.hp} - assist ${assist}${combo}${ai}`;
    }

    hudState() {
      const timer = Math.max(0, Math.ceil((this.roundMs - this.timeMs) / 1000));
      const meter = (fighter) => {
        const dash = 1 - clamp(fighter.skillCooldowns.dash / 2600, 0, 1);
        const spell = 1 - clamp(fighter.skillCooldowns.spell / 5200, 0, 1);
        return Math.round(((dash + spell) / 2) * 100);
      };
      const hp = (fighter) => Math.round(clamp(fighter.hp / Math.max(1, fighter.maxHp), 0, 1) * 100);
      return {
        roundLabel: 'ROUND 1',
        metaLabel: this.player.combo > 1 ? `COMBO x${this.player.combo}` : this.ai.intent ? `AI ${this.ai.intent}` : '1V1',
        timer: String(timer).padStart(2, '0'),
        playerName: 'BITCAT',
        enemyName: 'YOUMIAO',
        playerHp: hp(this.player),
        enemyHp: hp(this.enemy),
        playerMeter: meter(this.player),
        enemyMeter: meter(this.enemy),
      };
    }

    readyText() {
      return '';
    }

    handleInput(input) {
      if (!input) return;
      if (this.ended) {
        if (input.type === 'confirm' && this.host.restartGame) this.host.restartGame();
        else if (input.type === 'cancel' && this.host.closeEndedGame) this.host.closeEndedGame(this.state);
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
      if (this.state === 'ready' && ['direction', 'attack_primary', 'skill', 'guard', 'assist'].includes(input.type)) {
        this.state = 'playing';
      }
      const now = this.timeMs;
      if (input.type === 'direction') {
        const dx = Math.sign(input.dx || 0);
        const dy = Math.sign(input.dy || 0);
        this.player.input.setHeld('left', dx < 0);
        this.player.input.setHeld('right', dx > 0);
        this.player.input.setHeld('down', dy > 0);
        if (dy < 0) this.player.input.push('jump', now);
        return;
      }
      if (input.type === 'attack_primary') {
        this.player.input.push('light', now);
        return;
      }
      if (input.type === 'confirm') {
        this.player.input.push('light', now);
        return;
      }
      if (input.type === 'skill') {
        if (input.slot === 2) this.player.input.push('spell', now);
        else this.player.input.push('dash', now);
        return;
      }
      if (input.type === 'guard') {
        this.player.input.setHeld('guard', true);
        return;
      }
      if (input.type === 'boost') {
        if (input.active) this.tryAssist();
      }
    }

    handleKeyUp(key) {
      if (key === 'ArrowLeft' || key === 'a' || key === 'A') this.player.input.setHeld('left', false);
      if (key === 'ArrowRight' || key === 'd' || key === 'D') this.player.input.setHeld('right', false);
      if (key === 'ArrowDown' || key === 's' || key === 'S') this.player.input.setHeld('down', false);
      if (key === 'Shift') this.player.input.setHeld('guard', false);
    }

    tryAssist() {
      const ok = this.assist.trigger(this.enemy);
      if (ok && this.host.log) this.host.log('arena bitcat assist triggered');
      return ok;
    }

    update(dtMs) {
      this.frameDtMs = dtMs;
      this.lastDtMs = dtMs;
      if (this.ended || this.state === 'paused' || this.state === 'ready') return;
      this.timeMs += dtMs;
      if (this.state !== 'playing') return;
      this.ai.update(dtMs, this.timeMs, this.player);
      this.player.update(dtMs, this.timeMs, this.enemy);
      this.enemy.update(dtMs, this.timeMs, this.player);
      this.assist.update(dtMs);
      this.combat.update(dtMs);
      if (this.enemy.hp <= 0) {
        this.score += 100 + this.player.hp + this.player.combo * 5;
        this.finish('win');
      } else if (this.player.hp <= 0 || this.timeMs >= this.roundMs) {
        this.finish(this.player.hp > this.enemy.hp ? 'win' : 'lose');
      }
    }

    finish(result) {
      if (this.ended) return;
      this.ended = true;
      this.state = result;
      if (result === 'win') this.player.state = 'win';
      if (result === 'lose') this.enemy.state = 'win';
    }

    render(ctx, metrics) {
      if (!this.renderer) {
        this.renderer = new ArenaRenderer(ctx.canvas, this);
      }
      const width = metrics?.width || window.innerWidth || ctx.canvas.clientWidth || 960;
      const height = metrics?.height || window.innerHeight || ctx.canvas.clientHeight || 540;
      this.renderer.resize(width, height);
      this.renderer.render();
    }
  }

  function createArenaEngine(config, host) {
    return new ArenaEngine(config, host);
  }

  window.BitCatGames = window.BitCatGames || {};
  window.BitCatGames.arena = createArenaEngine;
  window.BitCatArenaTest = {
    ArenaEngine,
    ArenaCombat,
    Fighter,
    FighterAI,
    BitCatAssist,
    InputBuffer,
    attackData,
    boxesOverlap,
  };
})();
