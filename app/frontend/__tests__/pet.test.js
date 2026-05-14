// pet.test.js — PetStateMachine 状态机 (Vitest)
import { describe, it, expect, beforeEach } from 'vitest';

// 从 pet.js 复制核心逻辑（避免 DOM 依赖）
const STATE_CONFIG = {
  idle:     { frameCount: 4, frameDuration: 500, autoIdleTimeout: null },
  walk:     { frameCount: 4, frameDuration: 150, autoIdleTimeout: 3000 },
  sleep:    { frameCount: 2, frameDuration: 800, autoIdleTimeout: null },
  talk:     { frameCount: 3, frameDuration: 300, autoIdleTimeout: 5000 },
  happy:    { frameCount: 3, frameDuration: 200, autoIdleTimeout: 2000 },
  confused: { frameCount: 2, frameDuration: 400, autoIdleTimeout: 3000 },
  gameplay: { frameCount: 2, frameDuration: 300, autoIdleTimeout: null },
  gamewin:  { frameCount: 3, frameDuration: 200, autoIdleTimeout: 3000 },
  gamelose: { frameCount: 2, frameDuration: 400, autoIdleTimeout: 3000 },
};

const NOTIFICATION_CONFIG = {
  ai_thinking: { state: 'talk', ttlMs: 30000 },
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
  focused: 'talk',
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
    this.speed = 60;
    this.targetX = null;
    this.bubble = null;
    this.mode = 'idle';
    this.reactionMood = 'idle';
    this.notifications = [];
  }

  setState(newState) {
    if (this.state === newState) return;
    this.state = newState;
    this.frame = 0;
    this.frameTimeMs = 0;
    this.stateTimeMs = 0;
    if (newState !== 'walk') this.targetX = null;
  }

  walkTo(x) {
    this.setState('walk');
    this.targetX = x;
  }

  setMode(mode) {
    this.mode = MODE_STATE[mode] ? mode : 'idle';
    this.applySemanticState();
  }

  react(mood, speech) {
    this.reactionMood = MOOD_STATE[mood] ? mood : 'idle';
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
    if (kind == null) this.notifications = [];
    else this.notifications = this.notifications.filter(n => n.kind !== kind);
    this.applySemanticState();
  }

  expireNotifications(now) {
    const before = this.notifications.length;
    this.notifications = this.notifications.filter(n => n.expiresAt == null || n.expiresAt > now);
    if (this.notifications.length !== before) this.applySemanticState();
  }

  currentVisualState() {
    if (this.mode === 'sleep' || this.mode === 'game_play' || this.mode === 'gameplay') {
      return MODE_STATE[this.mode] || 'idle';
    }
    const active = this.notifications.length ? this.notifications[this.notifications.length - 1] : null;
    if (active && NOTIFICATION_CONFIG[active.kind]) return NOTIFICATION_CONFIG[active.kind].state;
    return MOOD_STATE[this.reactionMood] || 'idle';
  }

  applySemanticState() {
    this.setState(this.currentVisualState());
  }

  update(dtMs) {
    this.expireNotifications(performance.now());
    this.stateTimeMs += dtMs;
    this.frameTimeMs += dtMs;

    const config = STATE_CONFIG[this.state] || STATE_CONFIG.idle;
    while (this.frameTimeMs >= config.frameDuration) {
      this.frameTimeMs -= config.frameDuration;
      this.frame = (this.frame + 1) % config.frameCount;
    }

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

    if (config.autoIdleTimeout !== null && this.stateTimeMs >= config.autoIdleTimeout) {
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
          this.react(event.mood, event.speech);
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
      }
      return;
    }
    if (event.state) {
      if (event.state === 'exit') return;
      this.setState(event.state);
    }
    if (event.walk_to != null) this.walkTo(event.walk_to);
    if (event.bubble) this.bubble = event.bubble;
  }
}

describe('PetStateMachine', () => {
  let pet;

  beforeEach(() => {
    pet = new PetStateMachine();
  });

  describe('初始状态', () => {
    it('默认状态为 idle', () => {
      expect(pet.state).toBe('idle');
    });

    it('默认帧为 0', () => {
      expect(pet.frame).toBe(0);
    });

    it('默认朝右', () => {
      expect(pet.facingRight).toBe(true);
    });
  });

  describe('setState', () => {
    it('切换状态重置帧和计时器', () => {
      pet.frameTimeMs = 999;
      pet.stateTimeMs = 9999;
      pet.frame = 3;
      pet.setState('talk');

      expect(pet.frame).toBe(0);
      expect(pet.frameTimeMs).toBe(0);
      expect(pet.stateTimeMs).toBe(0);
    });

    it('相同状态不重置', () => {
      pet.frameTimeMs = 500;
      pet.setState('idle');
      expect(pet.frameTimeMs).toBe(500);
    });

    it('切换到非 walk 状态清除 targetX', () => {
      pet.walkTo(100);
      expect(pet.targetX).toBe(100);
      pet.setState('talk');
      expect(pet.targetX).toBeNull();
    });

    it('切换到 walk 不清除 targetX (由 walkTo 设置)', () => {
      pet.setState('walk');
      // walk 本身设置 targetX，setState 不清除
      expect(pet.targetX).toBeNull(); // setState 不设 target
    });
  });

  describe('帧推进', () => {
    it('未达 frameDuration 不推进帧', () => {
      pet.update(499);
      expect(pet.frame).toBe(0);
    });

    it('达到 frameDuration 推进一帧', () => {
      pet.update(499);
      pet.update(1);
      expect(pet.frame).toBe(1);
    });

    it('多帧循环', () => {
      // idle 有 4 帧，每帧 500ms
      pet.update(2000); // 正好 4 个周期
      expect(pet.frame).toBe(0);
    });
  });

  describe('Walk 移动', () => {
    it('向目标移动', () => {
      pet.x = 0;
      pet.speed = 100;
      pet.walkTo(50);
      pet.update(500);

      expect(Math.abs(pet.x - 50)).toBeLessThan(1);
    });

    it('移动时面朝目标方向', () => {
      pet.x = 0;
      pet.speed = 100;
      pet.walkTo(50);
      pet.update(500);

      expect(pet.facingRight).toBe(true);
    });

    it('向左移动时面朝左', () => {
      pet.x = 100;
      pet.speed = 100;
      pet.walkTo(50);
      pet.update(500);

      expect(pet.facingRight).toBe(false);
    });
  });

  describe('自动回 idle 超时', () => {
    it('walk 在 2999ms 时仍为 walk', () => {
      pet.setState('walk');
      pet.update(2999);
      expect(pet.state).toBe('walk');
    });

    it('walk 在 3000ms 时自动回 idle', () => {
      pet.setState('walk');
      pet.update(3000);
      expect(pet.state).toBe('idle');
    });

    it('sleep 永不超时', () => {
      pet.setState('sleep');
      pet.update(100000);
      expect(pet.state).toBe('sleep');
    });

    it('happy 2000ms 后回 idle', () => {
      pet.setState('happy');
      pet.update(1999);
      expect(pet.state).toBe('happy');
      pet.update(1);
      expect(pet.state).toBe('idle');
    });

    it('gameplay 不自动回 idle', () => {
      pet.setState('gameplay');
      pet.update(100000);
      expect(pet.state).toBe('gameplay');
    });

    it('gamewin 和 gamelose 3000ms 后回 idle', () => {
      pet.setState('gamewin');
      pet.update(2999);
      expect(pet.state).toBe('gamewin');
      pet.update(1);
      expect(pet.state).toBe('idle');

      pet.setState('gamelose');
      pet.update(3000);
      expect(pet.state).toBe('idle');
    });
  });

  describe('applyEvent', () => {
    it('notify 事件切换到对应视觉状态', () => {
      pet.applyEvent({ type: 'notify', kind: 'ai_thinking', body: '思考中', ttl_ms: 30000, refresh: true });
      expect(pet.state).toBe('talk');
    });

    it('react 事件切换情绪', () => {
      pet.applyEvent({ type: 'react', mood: 'happy', speech: null });
      expect(pet.state).toBe('happy');
    });

    it('bubble 事件存储文本', () => {
      pet.applyEvent({ type: 'show_bubble', text: 'hello' });
      expect(pet.bubble).toBe('hello');
    });

    it('walk_to 事件触发行走', () => {
      pet.applyEvent({ type: 'walk_to', x: 200 });
      expect(pet.state).toBe('walk');
      expect(pet.targetX).toBe(200);
    });

    it('sleep mode 优先于普通通知', () => {
      pet.applyEvent({ type: 'set_mode', mode: 'sleep' });
      pet.applyEvent({ type: 'notify', kind: 'tool_running', body: '执行命令', ttl_ms: 30000, refresh: true });
      expect(pet.state).toBe('sleep');
    });

    it('clear_notification 后回到 reaction', () => {
      pet.applyEvent({ type: 'react', mood: 'happy', speech: null });
      pet.applyEvent({ type: 'notify', kind: 'tool_running', body: '执行命令', ttl_ms: 30000, refresh: true });
      expect(pet.state).toBe('talk');
      pet.applyEvent({ type: 'clear_notification', kind: 'tool_running' });
      expect(pet.state).toBe('happy');
    });
  });
});

// ---- 嘴巴热区判定（纯函数，从 app.js 的 window.PetApp 引入）----

// 从 app.js IIFE 暴露的纯函数
function isMouthHotzone(x, y, canvasSize) {
  if (typeof window !== 'undefined' && window.PetApp && window.PetApp.isMouthHotzone) {
    return window.PetApp.isMouthHotzone(x, y, canvasSize);
  }
  // fallback（vitest 环境中 window.PetApp 可能未加载）
  var ratio = canvasSize / 128;
  return x >= 32 * ratio && x <= 96 * ratio && y >= 56 * ratio && y <= 96 * ratio;
}

function isLeftEyeHotzone(x, y, canvasSize) {
  if (typeof window !== 'undefined' && window.PetApp && window.PetApp.isLeftEyeHotzone) {
    return window.PetApp.isLeftEyeHotzone(x, y, canvasSize);
  }
  var ratio = canvasSize / 128;
  if (ratio <= 0) return false;
  return x >= 22 * ratio && x <= 44 * ratio && y >= 44 * ratio && y <= 62 * ratio;
}

describe('嘴巴热区判定 isMouthHotzone', () => {
  describe('正常态 128×128', () => {
    it('嘴巴正中心 (64, 76) 在热区内', () => {
      expect(isMouthHotzone(64, 76, 128)).toBe(true);
    });

    it('左腮红点 (40, 64) 在热区内', () => {
      expect(isMouthHotzone(40, 64, 128)).toBe(true);
    });

    it('右腮红点 (88, 64) 在热区内', () => {
      expect(isMouthHotzone(88, 64, 128)).toBe(true);
    });

    it('talk 大嘴底部 (64, 80) 在热区内', () => {
      expect(isMouthHotzone(64, 80, 128)).toBe(true);
    });

    it('眼睛位置 (40, 48) 不在热区', () => {
      expect(isMouthHotzone(40, 48, 128)).toBe(false);
    });

    it('头顶 (64, 16) 不在热区', () => {
      expect(isMouthHotzone(64, 16, 128)).toBe(false);
    });

    it('左边缘外侧 (20, 76) 不在热区', () => {
      expect(isMouthHotzone(20, 76, 128)).toBe(false);
    });

    it('右边缘外侧 (108, 76) 不在热区', () => {
      expect(isMouthHotzone(108, 76, 128)).toBe(false);
    });

    it('热区上边界 (64, 56) 在热区内（含边界）', () => {
      expect(isMouthHotzone(64, 56, 128)).toBe(true);
    });

    it('热区下边界 (64, 96) 在热区内（含边界）', () => {
      expect(isMouthHotzone(64, 96, 128)).toBe(true);
    });

    it('热区刚好上方 (64, 55) 不在热区', () => {
      expect(isMouthHotzone(64, 55, 128)).toBe(false);
    });
  });

  describe('折叠态 48×48（按比例缩放）', () => {
    it('嘴巴中心 (24, 29) 在热区内', () => {
      expect(isMouthHotzone(24, 29, 48)).toBe(true);
    });

    it('眼睛位置 (15, 18) 不在热区', () => {
      expect(isMouthHotzone(15, 18, 48)).toBe(false);
    });

    it('热区左边界 (12, 36) 在热区内', () => {
      expect(isMouthHotzone(12, 36, 48)).toBe(true);
    });

    it('左边界外 (11, 30) 不在热区', () => {
      expect(isMouthHotzone(11, 30, 48)).toBe(false);
    });
  });

  describe('边界安全', () => {
    it('坐标为 0 不在热区', () => {
      expect(isMouthHotzone(0, 0, 128)).toBe(false);
    });

    it('canvasSize 为 0 时全部不在热区（ratio=0）', () => {
      expect(isMouthHotzone(50, 50, 0)).toBe(false);
    });
  });
});

describe('左眼热区判定 isLeftEyeHotzone', () => {
  describe('正常态 128×128', () => {
    it('左眼中心 (32, 52) 在热区内', () => {
      expect(isLeftEyeHotzone(32, 52, 128)).toBe(true);
    });

    it('右眼中心 (88, 52) 不在热区', () => {
      expect(isLeftEyeHotzone(88, 52, 128)).toBe(false);
    });

    it('嘴巴中心 (64, 76) 不在热区', () => {
      expect(isLeftEyeHotzone(64, 76, 128)).toBe(false);
    });

    it('热区边界包含左上角和右下角', () => {
      expect(isLeftEyeHotzone(22, 44, 128)).toBe(true);
      expect(isLeftEyeHotzone(44, 62, 128)).toBe(true);
    });
  });

  describe('折叠态 48×48（按比例缩放）', () => {
    it('左眼中心 (12, 20) 在热区内', () => {
      expect(isLeftEyeHotzone(12, 20, 48)).toBe(true);
    });

    it('右眼位置 (34, 20) 不在热区', () => {
      expect(isLeftEyeHotzone(34, 20, 48)).toBe(false);
    });
  });

  describe('边界安全', () => {
    it('canvasSize 为 0 时全部不在热区', () => {
      expect(isLeftEyeHotzone(32, 52, 0)).toBe(false);
    });
  });
});
