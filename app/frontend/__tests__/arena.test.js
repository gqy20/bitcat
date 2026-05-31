import { describe, expect, it } from 'vitest';

const FLOOR_Y = 0;

function boxesOverlap(a, b) {
  return (
    Math.abs(a.x - b.x) * 2 < a.w + b.w &&
    Math.abs(a.y - b.y) * 2 < a.h + b.h &&
    Math.abs(a.z - b.z) * 2 < a.d + b.d
  );
}

function attackData(kind = 'light') {
  if (kind === 'heavy') {
    return { damage: 16, knockback: 6.8, hitstun: 420, width: 1.32, height: 1.1, depth: 1.5, offsetX: 1.02, offsetY: 1.05 };
  }
  return { damage: 8, knockback: 3.8, hitstun: 280, width: 1.06, height: 0.95, depth: 1.35, offsetX: 0.86, offsetY: 1.02 };
}

class Fighter {
  constructor(id, x, facing) {
    this.id = id;
    this.pos = { x, y: FLOOR_Y, z: 0 };
    this.facing = facing;
    this.hp = 100;
    this.state = 'idle';
    this.stateMs = 0;
    this.attack = null;
    this.attackHitIds = new Set();
    this.guardHeld = false;
    this.onGround = true;
    this.combo = 0;
  }

  hurtbox() {
    return { x: this.pos.x, y: this.pos.y + 0.95, z: 0, w: 0.72, h: 1.9, d: 0.92 };
  }

  hitbox() {
    if (!this.attack) return null;
    const data = this.attack.data;
    return {
      x: this.pos.x + this.facing * data.offsetX,
      y: this.pos.y + data.offsetY,
      z: 0,
      w: data.width,
      h: data.height,
      d: data.depth,
      data,
    };
  }

  isBlockingAgainst(attacker) {
    const towardAttacker = Math.sign(attacker.pos.x - this.pos.x) || this.facing;
    return this.guardHeld && this.facing === towardAttacker;
  }

  startAttack(kind) {
    this.attack = { data: attackData(kind) };
    this.attackHitIds.clear();
  }

  applyHit(attacker, hitbox) {
    const blocked = this.isBlockingAgainst(attacker);
    const damage = blocked ? Math.max(1, Math.floor(hitbox.data.damage * 0.2)) : hitbox.data.damage;
    this.hp = Math.max(0, this.hp - damage);
    this.state = blocked ? 'blockstun' : this.hp <= 0 ? 'dead' : 'hurt';
    return { blocked, applied: true, damage };
  }
}

class Combat {
  checkHit(attacker, defender) {
    const hitbox = attacker.hitbox();
    if (!hitbox || attacker.attackHitIds.has(defender.id)) return false;
    if (!boxesOverlap(hitbox, defender.hurtbox())) return false;
    attacker.attackHitIds.add(defender.id);
    const hit = defender.applyHit(attacker, hitbox);
    if (hit.applied && !hit.blocked) attacker.combo += 1;
    return hit;
  }
}

describe('Arena combat rules', () => {
  it('detects overlapping hitboxes', () => {
    expect(boxesOverlap(
      { x: 0, y: 0, z: 0, w: 1, h: 1, d: 1 },
      { x: 0.4, y: 0, z: 0, w: 1, h: 1, d: 1 },
    )).toBe(true);
    expect(boxesOverlap(
      { x: 0, y: 0, z: 0, w: 1, h: 1, d: 1 },
      { x: 2, y: 0, z: 0, w: 1, h: 1, d: 1 },
    )).toBe(false);
  });

  it('light attack damages and stuns defender once', () => {
    const attacker = new Fighter('p1', 0, 1);
    const defender = new Fighter('p2', 1.2, -1);
    attacker.startAttack('light');
    const combat = new Combat();
    const hit = combat.checkHit(attacker, defender);
    expect(hit.damage).toBe(8);
    expect(defender.hp).toBe(92);
    expect(defender.state).toBe('hurt');
    expect(attacker.combo).toBe(1);
    expect(combat.checkHit(attacker, defender)).toBe(false);
  });

  it('guard reduces damage when facing attacker', () => {
    const attacker = new Fighter('p1', 0, 1);
    const defender = new Fighter('p2', 1.2, -1);
    defender.guardHeld = true;
    attacker.startAttack('heavy');
    const hit = new Combat().checkHit(attacker, defender);
    expect(hit.blocked).toBe(true);
    expect(hit.damage).toBe(3);
    expect(defender.hp).toBe(97);
    expect(defender.state).toBe('blockstun');
  });
});
