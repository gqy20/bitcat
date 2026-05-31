import * as THREE from 'three';
import { GLTFLoader } from '../vendor/three/examples/jsm/loaders/GLTFLoader.js';

(function () {
  const GRAVITY = -36;
  const FLOOR_Y = 0;
  const ARENA_HALF_WIDTH = 8.2;
  const DEFAULT_HP = 100;
  const HITSTOP_MS = 70;
  const MODEL_BASE_URL = '/assets/arena/';

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
    if (kind === 'heavy') {
      return {
        kind,
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
      };
    }
    return {
      kind: 'light',
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
      const data = attackData(kind);
      this.state = 'attack';
      this.stateMs = 0;
      this.attack = { data, startedAt: now };
      this.attackHitIds.clear();
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
      const damage = blocked ? Math.max(1, Math.floor(hitbox.data.damage * 0.2)) : hitbox.data.damage;
      this.hp = Math.max(0, this.hp - damage);
      this.vel.x = attacker.facing * (blocked ? hitbox.data.knockback * 0.32 : hitbox.data.knockback);
      this.vel.y = blocked ? Math.max(this.vel.y, 0) : Math.max(this.vel.y, 2.2);
      this.onGround = blocked ? this.onGround : false;
      if (this.hp <= 0) {
        this.state = 'dead';
        this.stateMs = 0;
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
          const attack = this.input.consume(['heavy', 'light'], now);
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
      this.pos.x = clamp(this.pos.x, -ARENA_HALF_WIDTH, ARENA_HALF_WIDTH);
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
      this.lastHit = { attacker: attacker.id, defender: defender.id, ...hit };
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
      this.lastHit = { attacker: 'bitcat', defender: defender.id, ...hit };
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

  function createPart(geometry, material) {
    const mesh = new THREE.Mesh(geometry, material);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    return mesh;
  }

  function createBoxMesh(color, size = [0.8, 1.8, 0.8]) {
    const material = createStandardMaterial(color);
    const mesh = createPart(new THREE.BoxGeometry(size[0], size[1], size[2]), material);
    mesh.userData.material = material;
    return mesh;
  }

  function createHumanoidMesh(palette) {
    const root = new THREE.Group();
    const materials = {
      primary: createStandardMaterial(palette.primary),
      accent: createStandardMaterial(palette.accent),
      skin: createStandardMaterial(palette.skin),
      dark: createStandardMaterial(palette.dark),
      glow: createStandardMaterial(palette.glow, { roughness: 0.38, metalness: 0.12 }),
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
      const windup = attackProgress < 0.38;
      const strike = fighter.isAttackActive();
      const leadArm = fighter.facing > 0 ? parts.rightArm : parts.leftArm;
      const backArm = fighter.facing > 0 ? parts.leftArm : parts.rightArm;
      leadArm.rotation.x = windup ? 0.85 : strike ? -1.38 : -0.35;
      leadArm.rotation.z = fighter.facing > 0 ? -0.26 : 0.26;
      leadArm.position.z = strike ? 0.34 : 0.12;
      backArm.rotation.x = 0.28;
      parts.torso.rotation.y = fighter.facing > 0 ? -0.12 : 0.12;
      parts.torso.rotation.x = strike ? -0.08 : 0.04;
      parts.head.rotation.y = fighter.facing > 0 ? -0.08 : 0.08;
    }

    if (fighter.state === 'hurt') {
      parts.torso.rotation.x = 0.34;
      parts.head.rotation.x = 0.28;
      parts.leftArm.rotation.x = 0.62;
      parts.rightArm.rotation.x = 0.62;
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
      this.mixer = clips.length ? new THREE.AnimationMixer(root) : null;
      this.actions = new Map();
      this.current = null;
      this.currentKey = null;
      this.clipNames = clips.map((clip) => clip.name);
      for (const clip of clips) {
        const key = this.keyForClip(clip.name);
        if (!key || this.actions.has(key) || !this.mixer) continue;
        this.actions.set(key, this.mixer.clipAction(clip));
      }
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
      action.enabled = true;
      action.reset();
      action.fadeIn(0.08);
      if (this.current) this.current.fadeOut(0.08);
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
            () => resolve(null)
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
    const scale = clamp(3.1 / height, 0.15, 5.8);
    root.scale.multiplyScalar(scale);
    root.position.x -= center.x * scale;
    root.position.z -= center.z * scale;
    root.position.y -= box.min.y * scale;
    root.traverse((node) => {
      if (!node.isMesh) return;
      node.castShadow = true;
      node.receiveShadow = true;
      if (node.material) {
        node.material.roughness = node.material.roughness ?? 0.68;
        node.material.needsUpdate = true;
      }
    });
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
        alpha: true,
        antialias: true,
        powerPreference: 'high-performance',
      });
      this.renderer.setClearColor(0x000000, 0);
      this.renderer.shadowMap.enabled = true;
      this.scene = new THREE.Scene();
      this.camera = new THREE.PerspectiveCamera(42, 16 / 9, 0.1, 100);
      this.camera.position.set(0, 4.5, 12);
      this.camera.lookAt(0, 1.2, 0);
      this.debugBoxes = [];
      this.effects = [];
      this.seenHitSeq = 0;
      this.stageAccents = [];
      this.modelLoader = new ArenaModelLoader();
      this.buildScene();
    }

    buildScene() {
      const ambient = new THREE.HemisphereLight(0xd8f3ff, 0x302a42, 1.7);
      this.scene.add(ambient);
      const key = new THREE.DirectionalLight(0xffffff, 2.4);
      key.position.set(-3, 7, 5);
      key.castShadow = true;
      this.scene.add(key);
      const floor = new THREE.Mesh(
        new THREE.BoxGeometry(19, 0.24, 4.2),
        new THREE.MeshStandardMaterial({ color: 0x2c3140, roughness: 0.74, metalness: 0.04 })
      );
      floor.position.set(0, -0.14, 0);
      floor.receiveShadow = true;
      this.scene.add(floor);
      const matLineBlue = new THREE.MeshBasicMaterial({ color: 0x4cc9f0, transparent: true, opacity: 0.58 });
      const matLineRed = new THREE.MeshBasicMaterial({ color: 0xf72585, transparent: true, opacity: 0.52 });
      const matCenter = new THREE.MeshBasicMaterial({ color: 0xffd166, transparent: true, opacity: 0.4 });
      const leftLine = new THREE.Mesh(new THREE.BoxGeometry(6.8, 0.025, 0.035), matLineBlue);
      leftLine.position.set(-4.4, 0.015, 1.82);
      const rightLine = new THREE.Mesh(new THREE.BoxGeometry(6.8, 0.025, 0.035), matLineRed);
      rightLine.position.set(4.4, 0.015, 1.82);
      const centerMark = new THREE.Mesh(new THREE.BoxGeometry(0.08, 0.03, 3.5), matCenter);
      centerMark.position.set(0, 0.02, 0);
      this.scene.add(leftLine, rightLine, centerMark);
      this.stageAccents.push(leftLine, rightLine, centerMark);
      const back = new THREE.Mesh(
        new THREE.BoxGeometry(19, 3.2, 0.16),
        new THREE.MeshStandardMaterial({ color: 0x111827, transparent: true, opacity: 0.72 })
      );
      back.position.set(0, 1.5, -2.0);
      this.scene.add(back);
      const bannerMat = new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.12 });
      for (let i = -4; i <= 4; i += 2) {
        const panel = new THREE.Mesh(new THREE.BoxGeometry(0.9, 0.045, 0.035), bannerMat);
        panel.position.set(i, 2.68, -1.9);
        panel.rotation.z = i % 4 === 0 ? 0.08 : -0.08;
        this.scene.add(panel);
        this.stageAccents.push(panel);
      }
      this.engine.player.mesh = createHumanoidMesh({
        primary: 0x2577ff,
        accent: 0x58c7ff,
        skin: 0xffd6b0,
        dark: 0x172033,
        glow: 0x91f7ff,
      });
      this.engine.enemy.mesh = createHumanoidMesh({
        primary: 0xd9465f,
        accent: 0xff8a80,
        skin: 0xf5c8a6,
        dark: 0x2b1724,
        glow: 0xffd166,
      });
      this.engine.assist.mesh = createBoxMesh(0xffd166, [0.52, 0.52, 0.52]);
      this.scene.add(this.engine.player.mesh, this.engine.enemy.mesh, this.engine.assist.mesh);
      this.loadOptionalModel(this.engine.player, this.engine.config?.config?.player_model || `${MODEL_BASE_URL}player.glb`);
      this.loadOptionalModel(this.engine.enemy, this.engine.config?.config?.enemy_model || `${MODEL_BASE_URL}enemy.glb`);
    }

    async loadOptionalModel(fighter, path) {
      const gltf = await this.modelLoader.load(path);
      if (!gltf || !fighter.mesh || !this.scene) return;
      const modelRoot = gltf.scene.clone(true);
      normalizeImportedModel(modelRoot);
      const holder = new THREE.Group();
      holder.add(modelRoot);
      holder.userData.animator = new ModelAnimator(holder, gltf.animations || []);
      holder.userData.modelPath = path;
      if (this.engine.host?.log) {
        this.engine.host.log(`arena model loaded ${path} clips=${holder.userData.animator.clipNames.join(',') || '<none>'}`);
      }
      const oldMesh = fighter.mesh;
      holder.position.copy(oldMesh.position);
      holder.rotation.copy(oldMesh.rotation);
      this.scene.remove(oldMesh);
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
      const shake = this.engine.combat.hitstopMs > 0 ? Math.sin(this.engine.timeMs * 0.52) * 0.08 : 0;
      this.camera.position.x += (clamp(midpoint, -3.5, 3.5) + shake - this.camera.position.x) * 0.08;
      this.camera.lookAt(this.camera.position.x, 1.2, 0);
      const dtMs = this.engine.frameDtMs || this.engine.lastDtMs || 16;
      this.updateFighterMesh(this.engine.player, 0x58c7ff, dtMs);
      this.updateFighterMesh(this.engine.enemy, 0xff6b6b, dtMs);
      this.updateAssistMesh();
      this.updateStageAccents();
      this.updateEffects(dtMs);
      this.renderDebugBoxes();
      this.renderer.render(this.scene, this.camera);
    }

    updateStageAccents() {
      const pulse = 0.55 + Math.sin(this.engine.timeMs * 0.006) * 0.12;
      for (const accent of this.stageAccents) {
        if (accent.material) accent.material.opacity = clamp(pulse, 0.32, 0.72);
      }
    }

    spawnHitEffects() {
      if (this.seenHitSeq === this.engine.combat.lastHitSeq) return;
      this.seenHitSeq = this.engine.combat.lastHitSeq;
      const hit = this.engine.combat.lastHit;
      if (!hit) return;
      const defender = hit.defender === this.engine.player.id ? this.engine.player : this.engine.enemy;
      const color = hit.blocked ? 0x9be7ff : 0xfff3b0;
      const group = new THREE.Group();
      group.position.set(defender.pos.x - defender.facing * 0.42, defender.pos.y + 1.15, defender.pos.z);
      for (let i = 0; i < 7; i++) {
        const shard = createPart(
          new THREE.BoxGeometry(0.08, 0.08, 0.08),
          new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.9 })
        );
        shard.position.set((i - 3) * 0.045, (i % 3) * 0.035, 0);
        shard.rotation.set(i * 0.4, i * 0.2, i * 0.7);
        group.add(shard);
      }
      group.userData.age = 0;
      group.userData.life = hit.blocked ? 180 : 240;
      group.userData.blocked = hit.blocked;
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
        else this.scene.remove(effect);
      }
      this.effects = alive;
    }

    updateFighterMesh(fighter, color, dtMs = 16) {
      const mesh = fighter.mesh;
      if (!mesh) return;
      mesh.position.set(fighter.pos.x, fighter.pos.y, fighter.pos.z);
      mesh.rotation.y = fighter.facing < 0 ? Math.PI : 0;
      if (mesh.userData.animator) {
        mesh.userData.animator.update(fighter, dtMs);
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

    updateAssistMesh() {
      const assist = this.engine.assist;
      const mesh = assist.mesh;
      if (!mesh) return;
      mesh.visible = assist.activeMs > 0 || assist.cooldownMs <= 6900;
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
      ].filter((entry) => entry.box);
      for (const entry of boxes) {
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
      this.debug = true;
      this.timeMs = 0;
      this.lastDtMs = 16;
      this.frameDtMs = 16;
      this.roundMs = 60_000;
      this.player = new Fighter('player', { name: 'You', isPlayer: true, x: -1.45, facing: 1 });
      this.enemy = new Fighter('dummy', { name: 'AI Dummy', x: 1.45, facing: -1 });
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
        if (input.slot === 2) this.tryAssist();
        else this.player.input.push('heavy', now);
        return;
      }
      if (input.type === 'guard') {
        this.player.input.setHeld('guard', true);
        return;
      }
      if (input.type === 'boost') {
        this.player.input.push(input.active ? 'heavy' : 'light', now);
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
