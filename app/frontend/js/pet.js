// pet.js — 前端宠物状态机（镜像 Rust core::pet::Pet 逻辑）
//
// 动画引擎：时间轴查表（elapsed-driven），每帧独立 duration。
// 状态分两类：循环态（loop: true）和瞬变态（repeat: N + fallback）。

const STATE_CONFIG = {
  idle: {
    frames: [
      { sprite: 0, duration: 1500 },  // 睁眼 — 悠闲
      { sprite: 1, duration: 120 },   // 半眯 — 快速
      { sprite: 2, duration: 200 },   // 闭眼 — 短暂
      { sprite: 1, duration: 120 },   // 半眯 — 恢复
      { sprite: 0, duration: 1800 },  // 睁眼 — 深呼吸停顿
    ],
    loop: true,
  },
  walk: {
    frames: [
      { sprite: 0, duration: 150 },
      { sprite: 1, duration: 150 },
      { sprite: 0, duration: 150 },
      { sprite: 2, duration: 150 },
    ],
    loop: true,
    autoIdleTimeout: 3000,
  },
  sleep: {
    frames: [
      { sprite: 0, duration: 800 },
      { sprite: 1, duration: 800 },
    ],
    loop: true,
  },
  talk: {
    frames: [
      { sprite: 0, duration: 300 },
      { sprite: 1, duration: 300 },
      { sprite: 2, duration: 400 },
    ],
    repeat: 3,
    fallback: 'idle',
  },
  happy: {
    frames: [
      { sprite: 0, duration: 250 },
      { sprite: 1, duration: 120 },
      { sprite: 0, duration: 230 },
    ],
    repeat: 3,
    fallback: 'idle',
  },
  confused: {
    frames: [
      { sprite: 0, duration: 400 },
      { sprite: 1, duration: 400 },
    ],
    repeat: 2,
    fallback: 'idle',
  },
  focused: {
    frames: [
      { sprite: 0, duration: 700 },
      { sprite: 1, duration: 140 },
      { sprite: 0, duration: 520 },
      { sprite: 2, duration: 120 },
    ],
    loop: true,
  },
  preparing: {
    frames: [
      { sprite: 1, duration: 180 },
      { sprite: 2, duration: 180 },
      { sprite: 1, duration: 180 },
      { sprite: 0, duration: 260 },
    ],
    loop: true,
  },
  gameplay: {
    frames: [
      { sprite: 0, duration: 300 },
      { sprite: 1, duration: 300 },
    ],
    loop: true,
  },
  gamewin: {
    frames: [
      { sprite: 0, duration: 250 },
      { sprite: 1, duration: 120 },
      { sprite: 0, duration: 230 },
    ],
    repeat: 5,
    fallback: 'idle',
  },
  gamelose: {
    frames: [
      { sprite: 0, duration: 400 },
      { sprite: 1, duration: 400 },
    ],
    repeat: 4,
    fallback: 'idle',
  },
};

const NOTIFICATION_CONFIG = {
  ai_thinking: { state: 'talk', ttlMs: 30000 },
  ai_writing: { state: 'talk', ttlMs: 30000 },
  tool_preparing: { state: 'preparing', ttlMs: 30000 },
  tool_running: { state: 'talk', ttlMs: 30000 },
  tool_blocked: { state: 'confused', ttlMs: 15000 },
  tool_failed: { state: 'confused', ttlMs: 15000 },
  listening: { state: 'talk', ttlMs: null },
  screenshot_observing: { state: 'talk', ttlMs: 5000 },
};

const MOOD_STATE = {
  idle: 'idle',
  happy: 'happy',
  confused: 'confused',
  focused: 'focused',
  caring: 'happy',
  excited: 'happy',
  sleepy: 'sleep',
};

const MODE_STATE = {
  idle: 'idle',
  sleep: 'sleep',
  game_play: 'gameplay',
  gameplay: 'gameplay',
};

class PetStateMachine {
  constructor() {
    this.state = 'idle';
    this.frame = 0;
    this.frameTimeMs = 0;
    this.stateTimeMs = 0;
    this.x = 64;
    this.y = 64;
    this.facingRight = true;
    this.speed = 60; // px/s
    this.targetX = null;
    this.bubble = null;
    this.mode = 'idle';
    this.reactionMood = 'idle';
    this.reactionExpiresAt = null;
    this.notifications = [];
  }

  setState(newState) {
    if (this.state === newState) return;
    this.state = newState;
    this.frame = 0;
    this.frameTimeMs = 0;
    this.stateTimeMs = 0;
    if (newState !== 'walk') {
      this.targetX = null;
    }
  }

  walkTo(x) {
    this.setState('walk');
    this.targetX = x;
  }

  setMode(mode) {
    const normalized = MODE_STATE[mode] ? mode : 'idle';
    this.mode = normalized;
    this.applySemanticState();
  }

  react(mood, speech, ttlMs) {
    this.reactionMood = MOOD_STATE[mood] ? mood : 'idle';
    this.reactionExpiresAt = ttlMs == null ? null : performance.now() + ttlMs;
    if (speech) this.bubble = speech;
    this.applySemanticState();
  }

  setNotification(kind, body, ttlMs, refresh) {
    const config = NOTIFICATION_CONFIG[kind];
    if (!config) return;

    const now = performance.now();
    const effectiveTtl = ttlMs === undefined || ttlMs === null ? config.ttlMs : ttlMs;
    const existing = this.notifications.find(n => n.kind === kind);
    if (existing && refresh !== false) {
      existing.body = body || existing.body;
      existing.updatedAt = now;
      existing.expiresAt = effectiveTtl == null ? null : now + effectiveTtl;
    } else if (!existing) {
      this.notifications.push({
        kind,
        body: body || null,
        updatedAt: now,
        expiresAt: effectiveTtl == null ? null : now + effectiveTtl,
      });
    }
    this.applySemanticState();
  }

  clearNotification(kind) {
    if (kind == null) {
      this.notifications = [];
    } else {
      this.notifications = this.notifications.filter(n => n.kind !== kind);
    }
    this.applySemanticState();
  }

  expireNotifications(now) {
    const before = this.notifications.length;
    this.notifications = this.notifications.filter(n => n.expiresAt == null || n.expiresAt > now);
    const reactionExpired = this.reactionExpiresAt != null && this.reactionExpiresAt <= now;
    if (reactionExpired) {
      this.reactionMood = 'idle';
      this.reactionExpiresAt = null;
    }
    if (this.notifications.length !== before || reactionExpired) {
      this.applySemanticState();
    }
  }

  currentVisualState() {
    if (this.mode === 'sleep' || this.mode === 'game_play' || this.mode === 'gameplay') {
      return MODE_STATE[this.mode] || 'idle';
    }
    const active = this.notifications.length ? this.notifications[this.notifications.length - 1] : null;
    if (active && NOTIFICATION_CONFIG[active.kind]) {
      return NOTIFICATION_CONFIG[active.kind].state;
    }
    return MOOD_STATE[this.reactionMood] || 'idle';
  }

  applySemanticState() {
    const next = this.currentVisualState();
    this.setState(next);
  }

  update(dtMs) {
    this.expireNotifications(performance.now());
    this.stateTimeMs += dtMs;
    this.frameTimeMs += dtMs;

    const config = STATE_CONFIG[this.state] || STATE_CONFIG.idle;
    const frames = config.frames;
    const passDuration = frames.reduce(function(s, f) { return s + f.duration; }, 0);

    if (passDuration === 0) return;

    // 瞬态：播完 repeat 遍后回落
    if (config.repeat != null) {
      if (this.frameTimeMs >= passDuration * config.repeat) {
        this.setState(config.fallback || 'idle');
        return;
      }
    }

    // 时间轴查表：根据 elapsed 定位当前帧
    let elapsed = this.frameTimeMs % passDuration;
    for (let i = 0; i < frames.length; i++) {
      if (elapsed < frames[i].duration) {
        this.frame = frames[i].sprite;
        break;
      }
      elapsed -= frames[i].duration;
    }

    // Walk 移动
    if (this.state === 'walk' && this.targetX !== null) {
      const dx = this.targetX - this.x;
      const move = this.speed * dtMs / 1000;
      if (Math.abs(dx) < move) {
        this.x = this.targetX;
      } else {
        this.x += Math.sign(dx) * move;
      }
      this.facingRight = dx > 0;
    }

    // Walk 自动超时（未到达目标时强制停止）
    if (config.autoIdleTimeout != null && this.stateTimeMs >= config.autoIdleTimeout) {
      this.setState('idle');
    }
  }

  applyEvent(event) {
    if (!event) return;

    if (event.type) {
      switch (event.type) {
        case 'notify':
          this.setNotification(event.kind, event.body, event.ttl_ms, event.refresh);
          break;
        case 'clear_notification':
          this.clearNotification(event.kind);
          break;
        case 'react':
          this.react(event.mood, event.speech, event.ttl_ms);
          break;
        case 'set_mode':
          this.setMode(event.mode);
          break;
        case 'walk_to':
          this.walkTo(event.x);
          break;
        case 'show_bubble':
          this.bubble = event.text;
          break;
        case 'exit':
          break;
        case 'play_dance':
          break;
      }
      return;
    }

  }
}

// 测试函数
function runPetTests() {
  const results = [];

  function assert(name, condition) {
    results.push({ name, pass: !!condition });
  }

  // 默认状态
  const pet = new PetStateMachine();
  assert('default_state_idle', pet.state === 'idle');
  assert('default_frame_0', pet.frame === 0);
  assert('default_facing_right', pet.facingRight === true);

  // setState 重置帧
  pet.frameTimeMs = 999;
  pet.stateTimeMs = 9999;
  pet.frame = 3;
  pet.setState('talk');
  assert('set_state_resets_frame', pet.frame === 0);
  assert('set_state_resets_timers', pet.frameTimeMs === 0 && pet.stateTimeMs === 0);

  // 同状态不重置
  const pet2 = new PetStateMachine();
  pet2.frameTimeMs = 500;
  pet2.setState('idle');
  assert('same_state_no_reset', pet2.frameTimeMs === 500);

  // ===== 非均匀帧时长测试 =====

  // idle: 前 1500ms 保持 sprite 0（睁眼）
  const pet3 = new PetStateMachine();
  pet3.update(1499);
  assert('idle_no_advance_1499', pet3.frame === 0);
  pet3.update(1); // 总计 1500ms → 切到 sprite 1（半眯）
  assert('idle_advance_at_1500', pet3.frame === 1);

  // idle: 1500+120=1620ms 后切到 sprite 2（闭眼）
  const pet3b = new PetStateMachine();
  pet3b.update(1619);
  assert('idle_still_1_at_1619', pet3b.frame === 1);
  pet3b.update(1); // 1620ms → sprite 2
  assert('idle_closed_at_1620', pet3b.frame === 2);

  // idle 循环：一整圈后回到 sprite 0
  const pet3c = new PetStateMachine();
  const idleTotal = 1500 + 120 + 200 + 120 + 1800; // 3740ms
  pet3c.update(idleTotal - 1);
  assert('idle_last_frame_before_loop', pet3c.frame === 0); // 最后帧也是 sprite 0
  pet3c.update(1); // 回到第一帧
  assert('idle_loops_back_to_0', pet3c.frame === 0);

  // ===== 瞬态 repeat+fallback 测试 =====

  // happy: 3 遍 × (250+120+230)=600ms/遍，总计 1800ms 后→idle
  const pet4 = new PetStateMachine();
  pet4.setState('happy');
  assert('happy_starts_sprite_0', pet4.frame === 0);
  pet4.update(1799);
  assert('happy_still_happy_at_1799', pet4.state === 'happy');
  pet4.update(1); // 1800ms → fallback to idle
  assert('happy_fallbacks_at_1800', pet4.state === 'idle');

  // confused: 2 遍 × (400+400)=800ms/遍，总计 1600ms 后→idle
  const pet5 = new PetStateMachine();
  pet5.setState('confused');
  pet5.update(1599);
  assert('confused_still_confused_at_1599', pet5.state === 'confused');
  pet5.update(1); // 1600ms → idle
  assert('confused_fallbacks_at_1600', pet5.state === 'idle');

  // talk: 3 遍 × (300+300+400)=1000ms/遍，总计 3000ms 后→idle
  const pet6 = new PetStateMachine();
  pet6.setState('talk');
  pet6.update(2999);
  assert('talk_still_talk_at_2999', pet6.state === 'talk');
  pet6.update(1); // 3000ms → idle
  assert('talk_fallbacks_at_3000', pet6.state === 'idle');

  // ===== Walk 测试 =====

  // Walk 移动
  const pet7 = new PetStateMachine();
  pet7.x = 0;
  pet7.speed = 100;
  pet7.walkTo(50);
  pet7.update(500);
  assert('walk_reaches_target', Math.abs(pet7.x - 50) < 1);
  assert('walk_facing_right', pet7.facingRight === true);

  // Walk 自动超时
  const pet8 = new PetStateMachine();
  pet8.setState('walk');
  pet8.update(2999);
  assert('walk_still_walk_at_2999', pet8.state === 'walk');
  pet8.update(1);
  assert('walk_auto_idle_at_3000', pet8.state === 'idle');

  // Sleep 不超时、不 fallback
  const pet9 = new PetStateMachine();
  pet9.setState('sleep');
  pet9.update(100000);
  assert('sleep_no_timeout', pet9.state === 'sleep');

  // Idle 不超时、不 fallback
  const pet10 = new PetStateMachine();
  pet10.update(100000);
  assert('idle_no_timeout', pet10.state === 'idle');

  // applyEvent
  const pet11 = new PetStateMachine();
  pet11.applyEvent({ type: 'notify', kind: 'ai_writing', body: 'hello', ttl_ms: 30000, refresh: true });
  assert('apply_event_state', pet11.state === 'talk');
  pet11.applyEvent({ type: 'show_bubble', text: 'hello' });
  assert('apply_event_bubble', pet11.bubble === 'hello');
  pet11.applyEvent({ type: 'walk_to', x: 200 });
  assert('apply_event_walk', pet11.state === 'walk' && pet11.targetX === 200);

  // ===== 大 dt 掉帧恢复测试 =====

  // idle: 掉帧后时间轴直接定位正确位置
  const pet12 = new PetStateMachine();
  pet12.update(50000); // 50秒后，应该在某帧上而不是 panic
  assert('large_dt_idle_frame_valid', pet12.frame >= 0 && pet12.frame <= 2);

  // happy: 超过总时长应已 fallback 到 idle
  const pet13 = new PetStateMachine();
  pet13.setState('happy');
  pet13.update(99999);
  assert('large_dt_happy_fallback', pet13.state === 'idle');

  return results;
}

if (typeof window !== 'undefined') {
  window.PetState = { PetStateMachine, STATE_CONFIG, runPetTests };
}

export { PetStateMachine, STATE_CONFIG, runPetTests };
