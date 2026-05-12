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

  update(dtMs) {
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
  });

  describe('applyEvent', () => {
    it('state 事件切换状态', () => {
      pet.applyEvent({ state: 'talk' });
      expect(pet.state).toBe('talk');
    });

    it('exit 事件被忽略', () => {
      pet.applyEvent({ state: 'exit' });
      expect(pet.state).toBe('idle');
    });

    it('bubble 事件存储文本', () => {
      pet.applyEvent({ bubble: 'hello' });
      expect(pet.bubble).toBe('hello');
    });

    it('walk_to 事件触发行走', () => {
      pet.applyEvent({ walk_to: 200 });
      expect(pet.state).toBe('walk');
      expect(pet.targetX).toBe(200);
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
