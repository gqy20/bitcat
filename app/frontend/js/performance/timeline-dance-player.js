import {
  computeTimelineDanceOffset,
  computeTimelineSpriteOptions,
  stepRepeat,
} from './motion.js';

export class TimelineDancePlayer {
  constructor(payload, metrics) {
    this.kind = 'timeline-dance';
    this.sessionId = payload.session_id;
    this.steps = payload.dance.steps;
    this.index = 0;
    this.repeatIndex = 0;
    this.time = 0;
    this.elapsed = 0;
    this.maxDurationMs = typeof payload.dance.max_duration_ms === 'number' ? payload.dance.max_duration_ms : null;
    this.loop_ = payload.dance.loop_ !== false;
    this.metrics = metrics;
  }

  advanceStep() {
    var step = this.steps[this.index];
    var repeat = stepRepeat(step);

    if (this.repeatIndex + 1 < repeat) {
      this.repeatIndex++;
      return true;
    }

    this.repeatIndex = 0;
    this.index++;

    if (this.index < this.steps.length) return true;
    if (this.loop_) {
      this.index = 0;
      return true;
    }
    return false;
  }

  update(dt) {
    this.time += dt;
    this.elapsed += dt;

    if (this.maxDurationMs != null && this.elapsed >= this.maxDurationMs) {
      return { done: true, reason: 'max_duration' };
    }

    var step = this.steps[this.index];
    while (step && this.time >= step.duration_ms) {
      this.time -= step.duration_ms;
      if (!this.advanceStep()) return { done: true, reason: 'finished' };
      step = this.steps[this.index];
    }
    if (!step) return { done: true, reason: 'finished' };

    var action = step.action;
    var progress = this.time / step.duration_ms;
    var sprite = computeTimelineSpriteOptions(action, progress, this.time, 1);

    return {
      done: false,
      action: action,
      offset: computeTimelineDanceOffset(action, progress, this.time, this.metrics, 1),
      spriteOptions: sprite.opts,
      facingRight: sprite.facingRight,
    };
  }
}
