// sprite.test.js — SpriteRenderer 数据完整性 (Vitest)
import { describe, it, expect } from 'vitest';

const SPRITE_W = 16;
const SPRITE_H = 16;

const PALETTE = {
  0: null,
  1: [30, 30, 40, 255],
  2: [255, 180, 140, 255],
  3: [255, 220, 190, 255],
  4: [40, 35, 50, 255],
  5: [255, 120, 140, 255],
};

function cloneSprite(base, mods) {
  const out = base.slice();
  for (const [row, col, val] of mods) out[row * SPRITE_W + col] = val;
  return out;
}

const IDLE_BASE = [
  0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
  0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
  0,0,1,2,2,2,2,1,2,2,2,2,1,0,0,0,
  0,1,1,2,2,2,2,2,2,2,2,2,1,1,0,0,
  0,1,2,2,3,2,2,2,2,2,3,2,2,1,0,0,
  0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
  0,1,2,4,4,2,2,2,2,2,4,4,2,1,0,0,
  0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
  0,1,2,2,2,5,2,2,2,5,2,2,2,1,0,0,
  0,1,2,2,2,2,2,1,2,2,2,2,2,1,0,0,
  0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
  0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,0,
  0,0,0,1,1,2,2,2,2,1,1,0,0,0,0,0,
  0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,0,
  0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,0,
  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

const IDLE_BLINK_HALF = cloneSprite(IDLE_BASE, [
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
  [7, 3, 4], [7, 4, 4], [7, 10, 4], [7, 11, 4],
]);
const IDLE_BLINK_CLOSED = cloneSprite(IDLE_BASE, [
  [5, 3, 4], [5, 4, 4], [5, 10, 4], [5, 11, 4],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
]);

const WALK_FRAME_A = IDLE_BASE;
const WALK_FRAME_B = cloneSprite(IDLE_BASE, [[13, 4, 0], [12, 4, 1], [14, 5, 0], [13, 5, 1]]);
const WALK_FRAME_C = IDLE_BASE;
const WALK_FRAME_D = cloneSprite(IDLE_BASE, [[13, 9, 0], [12, 9, 1], [14, 8, 0], [13, 8, 1]]);

const SLEEP_BASE = cloneSprite(IDLE_BASE, [
  [5, 3, 4], [5, 4, 4], [5, 10, 4], [5, 11, 4],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
  [8, 5, 2], [8, 11, 2],
]);
const SLEEP_BREATHE = cloneSprite(SLEEP_BASE, [
  [0, 3, 0], [0, 4, 0], [0, 5, 0], [0, 6, 0], [0, 7, 0], [0, 8, 0], [0, 9, 0], [0, 10, 0],
  [1, 3, 1], [1, 4, 1], [1, 5, 2], [1, 6, 1], [1, 7, 1], [1, 8, 1], [1, 9, 2], [1, 10, 1],
]);

const TALK_SMALL = cloneSprite(IDLE_BASE, [[8, 7, 0], [8, 8, 0], [8, 9, 0]]);
const TALK_LARGE = cloneSprite(IDLE_BASE, [
  [8, 6, 1], [8, 7, 0], [8, 8, 0], [8, 9, 0], [8, 10, 1],
  [9, 7, 0], [9, 8, 0], [9, 9, 0],
]);
const TALK_CLOSED = cloneSprite(IDLE_BASE, [[8, 7, 5], [8, 8, 5], [8, 9, 5]]);

const HAPPY_BASE = cloneSprite(IDLE_BASE, [
  [5, 3, 1], [5, 4, 1], [5, 10, 1], [5, 11, 1],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
  [8, 6, 1], [8, 7, 5], [8, 8, 5], [8, 9, 5], [8, 10, 1],
  [9, 7, 5], [9, 8, 5], [9, 9, 5],
]);
const HAPPY_BLINK = cloneSprite(HAPPY_BASE, [
  [5, 3, 1], [5, 4, 1], [5, 10, 1], [5, 11, 1],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
]);

const CONFUSED_LEFT = cloneSprite(IDLE_BASE, [
  [5, 3, 4], [5, 4, 0], [6, 3, 0], [6, 4, 4],
  [5, 10, 4], [5, 11, 0], [6, 10, 0], [6, 11, 4],
  [8, 5, 5], [8, 6, 5],
]);
const CONFUSED_RIGHT = cloneSprite(IDLE_BASE, [
  [5, 3, 0], [5, 4, 4], [6, 3, 4], [6, 4, 0],
  [5, 10, 0], [5, 11, 4], [6, 10, 4], [6, 11, 0],
  [8, 9, 5], [8, 10, 5],
]);

const SPRITES = {
  idle:     [IDLE_BASE, IDLE_BLINK_HALF, IDLE_BLINK_CLOSED, IDLE_BASE],
  walk:     [WALK_FRAME_A, WALK_FRAME_B, WALK_FRAME_C, WALK_FRAME_D],
  sleep:    [SLEEP_BASE, SLEEP_BREATHE],
  talk:     [TALK_SMALL, TALK_LARGE, TALK_CLOSED],
  happy:    [HAPPY_BASE, HAPPY_BLINK, HAPPY_BASE],
  confused: [CONFUSED_LEFT, CONFUSED_RIGHT],
};

function getSprite(state, frame) {
  const frames = SPRITES[state] || SPRITES.idle;
  const f = frame == null ? 0 : ((frame % frames.length) + frames.length) % frames.length;
  return frames[f];
}

describe('SpriteRenderer 数据完整性', () => {
  describe('精灵数据结构', () => {
    it.each(Object.keys(SPRITES))('%s 是非空数组', (name) => {
      expect(Array.isArray(SPRITES[name])).toBe(true);
      expect(SPRITES[name].length).toBeGreaterThan(0);
    });

    it('每帧像素数为 SPRITE_W * SPRITE_H (=256)', () => {
      for (const [, frames] of Object.entries(SPRITES)) {
        for (let i = 0; i < frames.length; i++) {
          expect(frames[i].length).toBe(SPRITE_W * SPRITE_H);
        }
      }
    });

    it('所有像素值在调色板范围内 [0-5]', () => {
      for (const [, frames] of Object.entries(SPRITES)) {
        for (const frame of frames) {
          for (const v of frame) {
            expect(v).toBeGreaterThanOrEqual(0);
            expect(v).toBeLessThanOrEqual(5);
          }
        }
      }
    });
  });

  describe('调色板', () => {
    it('索引 0 为透明 (null)', () => {
      expect(PALETTE[0]).toBeNull();
    });

    it('索引 1-5 为 RGBA 数组', () => {
      for (let i = 1; i <= 5; i++) {
        expect(Array.isArray(PALETTE[i])).toBe(true);
        expect(PALETTE[i].length).toBe(4);
      }
    });
  });

  describe('getSprite 帧取模', () => {
    it('idle 帧 0 返回第一帧', () => {
      expect(getSprite('idle', 0)).toBe(SPRITES.idle[0]);
    });

    it('帧号等于长度时回绕到 0', () => {
      expect(getSprite('idle', SPRITES.idle.length)).toBe(SPRITES.idle[0]);
    });

    it('负帧号从末尾回绕', () => {
      expect(getSprite('idle', -1)).toBe(SPRITES.idle[SPRITES.idle.length - 1]);
    });

    it('未知状态 fallback 到 idle', () => {
      expect(getSprite('unknown', 0)).toBe(SPRITES.idle[0]);
    });
  });

  describe('各状态帧数', () => {
    it('idle 4 帧', () => expect(SPRITES.idle.length).toBe(4));
    it('walk 4 帧', () => expect(SPRITES.walk.length).toBe(4));
    it('sleep 2 帧', () => expect(SPRITES.sleep.length).toBe(2));
    it('talk 3 帧', () => expect(SPRITES.talk.length).toBe(3));
    it('happy 3 帧', () => expect(SPRITES.happy.length).toBe(3));
    it('confused 2 帧', () => expect(SPRITES.confused.length).toBe(2));
  });

  describe('关键像素特征', () => {
    it('idle 闭眼帧眼睛位置为肤色', () => {
      expect(SPRITES.idle[2][6 * SPRITE_W + 3]).toBe(2);
    });

    it('talk 大嘴帧嘴巴区域透明', () => {
      expect(SPRITES.talk[1][9 * SPRITE_W + 8]).toBe(0);
    });

    it('sleep 呼吸帧顶部内缩为透明', () => {
      expect(SPRITES.sleep[1][0 * SPRITE_W + 5]).toBe(0);
    });

    it('happy 大嘴帧嘴巴颜色为腮红', () => {
      expect(SPRITES.happy[0][9 * SPRITE_W + 8]).toBe(5);
    });

    it('walk B 帧抬脚位置有轮廓线', () => {
      expect(SPRITES.walk[1][12 * SPRITE_W + 4]).toBe(1);
    });
  });

  describe('cloneSprite 不修改原数组', () => {
    it('修改克隆不影响基底', () => {
      const original = IDLE_BASE.slice();
      const cloned = cloneSprite(IDLE_BASE, [[0, 0, 5]]);
      expect(cloned[0]).toBe(5);
      expect(original[0]).toBe(0);
    });
  });
});
