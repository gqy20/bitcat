import { TimelineDancePlayer } from './timeline-dance-player.js';
import { MusicReactivePlayer } from './music-reactive-player.js';

export class PerformerHost {
  constructor(callbacks) {
    this.callbacks = callbacks;
    this.active = null;
  }

  hasActive() {
    return this.active != null;
  }

  async start(payload) {
    var metrics = this.callbacks.getMetrics ? await this.callbacks.getMetrics() : null;
    this.cancel();

    if (payload.kind === 'timeline-dance') {
      this.active = new TimelineDancePlayer(payload, metrics);
    } else if (payload.kind === 'music-reactive') {
      this.active = new MusicReactivePlayer(payload, metrics);
    } else {
      if (this.callbacks.log) this.callbacks.log('[performance] unknown kind: ' + payload.kind);
      return;
    }

    if (this.callbacks.setActiveClass) this.callbacks.setActiveClass(true);
    if (this.callbacks.log) {
      this.callbacks.log('[performance] start ' + payload.kind + ' #' + this.active.sessionId);
    }
  }

  frame(payload) {
    if (!this.active || !payload || payload.session_id !== this.active.sessionId) return;
    if (this.active.kind === 'music-reactive') this.active.handleFrame(payload);
  }

  stop(payload) {
    if (!this.active) return;
    if (payload && payload.session_id != null && payload.session_id !== this.active.sessionId) return;

    var player = this.active;
    this.active = null;
    if (this.callbacks.setActiveClass) this.callbacks.setActiveClass(false);
    if (this.callbacks.resetPosition) {
      this.callbacks.resetPosition(player, (payload && payload.reason) || 'stopped');
    }
  }

  cancel() {
    if (!this.active) return;
    this.active = null;
    if (this.callbacks.setActiveClass) this.callbacks.setActiveClass(false);
  }

  update(dt) {
    if (!this.active) return false;

    var frame = this.active.update(dt);
    if (frame.done) {
      this.stop({ session_id: this.active.sessionId, reason: frame.reason });
      return true;
    }

    if (frame.facingRight != null && this.callbacks.setFacingRight) {
      this.callbacks.setFacingRight(frame.facingRight);
    }
    if (this.callbacks.applyOffset) this.callbacks.applyOffset(this.active, frame.offset);
    if (this.callbacks.renderSprite) {
      this.callbacks.renderSprite(frame.action, frame.spriteOptions || {}, frame.spriteScale);
    }
    return true;
  }
}
