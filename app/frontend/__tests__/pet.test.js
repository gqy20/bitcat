// pet.test.js — PetStateMachine 状态机 (Vitest)
import { beforeEach, describe, expect, it } from 'vitest';
import { PetStateMachine, STATE_CONFIG } from '../js/pet.js';

function timelineDuration(state) {
  return STATE_CONFIG[state].frames.reduce((sum, frame) => sum + frame.duration, 0);
}

describe('PetStateMachine', () => {
  let pet;

  beforeEach(() => {
    pet = new PetStateMachine();
  });

  describe('初始状态', () => {
    it('默认状态为 idle', () => {
      expect(pet.state).toBe('idle');
      expect(pet.frame).toBe(0);
      expect(pet.facingRight).toBe(true);
    });
  });

  describe('setState', () => {
    it('切换状态重置帧和计时器', () => {
      pet.frameTimeMs = 999;
      pet.stateTimeMs = 9999;
      pet.frame = 3;

      pet.setState('talk');

      expect(pet.state).toBe('talk');
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
      pet.setState('focused');
      expect(pet.targetX).toBeNull();
    });
  });

  describe('时间轴帧推进', () => {
    it('使用非均匀 idle 帧时长', () => {
      pet.update(1499);
      expect(pet.frame).toBe(0);

      pet.update(1);
      expect(pet.frame).toBe(1);

      pet.update(120);
      expect(pet.frame).toBe(2);
    });

    it('idle 一整圈后继续循环', () => {
      pet.update(timelineDuration('idle'));
      expect(pet.state).toBe('idle');
      expect(pet.frame).toBe(0);
    });

    it('大 dt 直接定位合法帧', () => {
      pet.update(50_000);
      const maxFrame = Math.max(...STATE_CONFIG.idle.frames.map((frame) => frame.sprite));
      expect(pet.frame).toBeGreaterThanOrEqual(0);
      expect(pet.frame).toBeLessThanOrEqual(maxFrame);
    });
  });

  describe('repeat + fallback', () => {
    it('happy 播三遍后回 idle', () => {
      pet.setState('happy');
      pet.update(timelineDuration('happy') * 3 - 1);
      expect(pet.state).toBe('happy');

      pet.update(1);
      expect(pet.state).toBe('idle');
    });

    it('confused 播两遍后回 idle', () => {
      pet.setState('confused');
      pet.update(timelineDuration('confused') * 2 - 1);
      expect(pet.state).toBe('confused');

      pet.update(1);
      expect(pet.state).toBe('idle');
    });

    it('gamewin 和 gamelose 使用各自 repeat 时长', () => {
      pet.setState('gamewin');
      pet.update(timelineDuration('gamewin') * 5 - 1);
      expect(pet.state).toBe('gamewin');
      pet.update(1);
      expect(pet.state).toBe('idle');

      pet.setState('gamelose');
      pet.update(timelineDuration('gamelose') * 4 - 1);
      expect(pet.state).toBe('gamelose');
      pet.update(1);
      expect(pet.state).toBe('idle');
    });
  });

  describe('Walk 移动', () => {
    it('向目标移动并更新朝向', () => {
      pet.x = 0;
      pet.speed = 100;
      pet.walkTo(50);
      pet.update(500);

      expect(Math.abs(pet.x - 50)).toBeLessThan(1);
      expect(pet.facingRight).toBe(true);
    });

    it('向左移动时面朝左', () => {
      pet.x = 100;
      pet.speed = 100;
      pet.walkTo(50);
      pet.update(500);

      expect(pet.facingRight).toBe(false);
    });

    it('walk 在 3000ms 后自动回 idle', () => {
      pet.setState('walk');
      pet.update(2999);
      expect(pet.state).toBe('walk');

      pet.update(1);
      expect(pet.state).toBe('idle');
    });
  });

  describe('语义事件', () => {
    it('notify 事件切换到对应视觉状态', () => {
      pet.applyEvent({
        type: 'notify',
        kind: 'ai_thinking',
        body: '思考中',
        ttl_ms: 30000,
        refresh: true,
      });

      expect(pet.state).toBe('talk');
    });

    it('tool_preparing 使用 preparing 视觉状态', () => {
      pet.applyEvent({
        type: 'notify',
        kind: 'tool_preparing',
        body: '准备工具',
        ttl_ms: 30000,
        refresh: true,
      });

      expect(pet.state).toBe('preparing');
    });

    it('focused mood 使用 focused 视觉状态', () => {
      pet.applyEvent({ type: 'react', mood: 'focused', speech: null, ttl_ms: 5000 });
      expect(pet.state).toBe('focused');
    });

    it('react ttl 到期后回 idle', () => {
      pet.applyEvent({ type: 'react', mood: 'happy', speech: null, ttl_ms: 1 });
      expect(pet.state).toBe('happy');

      pet.expireNotifications(pet.reactionExpiresAt + 1);
      expect(pet.state).toBe('idle');
    });

    it('sleep mode 优先于普通通知', () => {
      pet.applyEvent({ type: 'set_mode', mode: 'sleep' });
      pet.applyEvent({
        type: 'notify',
        kind: 'tool_running',
        body: '执行命令',
        ttl_ms: 30000,
        refresh: true,
      });

      expect(pet.state).toBe('sleep');
    });

    it('clear_notification 后回到 reaction', () => {
      pet.applyEvent({ type: 'react', mood: 'happy', speech: null, ttl_ms: 5000 });
      pet.applyEvent({
        type: 'notify',
        kind: 'tool_running',
        body: '执行命令',
        ttl_ms: 30000,
        refresh: true,
      });
      expect(pet.state).toBe('talk');

      pet.applyEvent({ type: 'clear_notification', kind: 'tool_running' });
      expect(pet.state).toBe('happy');
    });

    it('show_bubble 和 walk_to 保持显式动作语义', () => {
      pet.applyEvent({ type: 'show_bubble', text: 'hello' });
      expect(pet.bubble).toBe('hello');

      pet.applyEvent({ type: 'walk_to', x: 200 });
      expect(pet.state).toBe('walk');
      expect(pet.targetX).toBe(200);
    });
  });
});

function isMouthHotzone(x, y, canvasSize) {
  if (typeof window !== 'undefined' && window.PetApp && window.PetApp.isMouthHotzone) {
    return window.PetApp.isMouthHotzone(x, y, canvasSize);
  }
  const ratio = canvasSize / 128;
  return ratio > 0 && x >= 32 * ratio && x <= 96 * ratio && y >= 56 * ratio && y <= 96 * ratio;
}

function isLeftEyeHotzone(x, y, canvasSize) {
  if (typeof window !== 'undefined' && window.PetApp && window.PetApp.isLeftEyeHotzone) {
    return window.PetApp.isLeftEyeHotzone(x, y, canvasSize);
  }
  const ratio = canvasSize / 128;
  return ratio > 0 && x >= 22 * ratio && x <= 44 * ratio && y >= 44 * ratio && y <= 62 * ratio;
}

describe('嘴巴热区判定 isMouthHotzone', () => {
  it('正常态嘴巴和腮红区域在热区内', () => {
    expect(isMouthHotzone(64, 76, 128)).toBe(true);
    expect(isMouthHotzone(40, 64, 128)).toBe(true);
    expect(isMouthHotzone(88, 64, 128)).toBe(true);
  });

  it('眼睛和边缘区域不在嘴巴热区', () => {
    expect(isMouthHotzone(40, 48, 128)).toBe(false);
    expect(isMouthHotzone(20, 76, 128)).toBe(false);
  });

  it('折叠态按比例缩放', () => {
    expect(isMouthHotzone(24, 29, 48)).toBe(true);
    expect(isMouthHotzone(15, 18, 48)).toBe(false);
  });

  it('canvasSize 为 0 时全部不在热区', () => {
    expect(isMouthHotzone(50, 50, 0)).toBe(false);
  });
});

describe('左眼热区判定 isLeftEyeHotzone', () => {
  it('正常态左眼在热区内，右眼和嘴巴不在热区', () => {
    expect(isLeftEyeHotzone(32, 52, 128)).toBe(true);
    expect(isLeftEyeHotzone(88, 52, 128)).toBe(false);
    expect(isLeftEyeHotzone(64, 76, 128)).toBe(false);
  });

  it('折叠态按比例缩放', () => {
    expect(isLeftEyeHotzone(12, 20, 48)).toBe(true);
    expect(isLeftEyeHotzone(34, 20, 48)).toBe(false);
  });

  it('canvasSize 为 0 时全部不在热区', () => {
    expect(isLeftEyeHotzone(32, 52, 0)).toBe(false);
  });
});
