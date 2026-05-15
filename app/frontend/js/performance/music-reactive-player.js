import {
  clamp01,
  computeMusicDanceOffset,
  computeMusicSpriteOptions,
} from './motion.js';

export class MusicReactivePlayer {
  constructor(payload, metrics) {
    this.kind = 'music-reactive';
    this.sessionId = payload.session_id;
    this.metrics = metrics;
    this.time = 0;
    this.energy = 0;
    this.targetEnergy = 0;
    this.action = 'idle';
    this.onsetMs = 9999;
    this.staleMs = 9999;
  }

  handleFrame(frame) {
    var payload = frame || {};
    this.staleMs = 0;
    this.targetEnergy = payload.silence ? 0 : clamp01(payload.energy);

    var bass = payload.bass == null ? this.targetEnergy : clamp01(payload.bass);
    if (payload.onset && this.targetEnergy > 0.12) {
      this.onsetMs = 0;
      this.action = bass > 0.55 || this.targetEnergy > 0.72 ? 'jump' : 'shake';
    } else if (payload.silence || this.targetEnergy < 0.08) {
      this.action = 'idle';
    } else if (this.action !== 'jump' && this.action !== 'shake') {
      this.action = this.targetEnergy > 0.36 ? 'wave' : 'idle';
    }
  }

  update(dt) {
    this.time += dt;
    this.onsetMs += dt;
    this.staleMs += dt;

    if (this.staleMs > 1600) {
      this.targetEnergy = 0;
    }

    this.energy += (this.targetEnergy - this.energy) * 0.18;

    if (this.onsetMs > 360) {
      this.action = this.energy > 0.28 ? 'wave' : 'idle';
    }

    var progress = (this.onsetMs % 360) / 360;
    var intensity = Math.max(0.08, this.energy);
    var sprite = computeMusicSpriteOptions(this.action, progress, this.time, intensity);

    return {
      done: false,
      action: this.action,
      offset: computeMusicDanceOffset(this.action, progress, this.time, this.metrics, intensity),
      spriteOptions: sprite.opts,
      facingRight: sprite.facingRight,
    };
  }
}
