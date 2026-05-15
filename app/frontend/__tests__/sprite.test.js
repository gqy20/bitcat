// sprite.test.js — SpriteRenderer 数据完整性 (Vitest)
import { describe, expect, it } from 'vitest';
import {
  PALETTE,
  SPRITES,
  SPRITE_H,
  SPRITE_W,
  cloneSprite,
  getSprite,
} from '../js/sprite.js';

describe('SpriteRenderer 数据完整性', () => {
  describe('精灵数据结构', () => {
    it.each(Object.keys(SPRITES))('%s 是非空数组', (name) => {
      expect(Array.isArray(SPRITES[name])).toBe(true);
      expect(SPRITES[name].length).toBeGreaterThan(0);
    });

    it('每帧像素数为 SPRITE_W * SPRITE_H', () => {
      for (const [, frames] of Object.entries(SPRITES)) {
        for (const frame of frames) {
          expect(frame.length).toBe(SPRITE_W * SPRITE_H);
        }
      }
    });

    it('所有像素值在调色板范围内', () => {
      const maxPaletteIndex = Math.max(...Object.keys(PALETTE).map(Number));
      for (const [, frames] of Object.entries(SPRITES)) {
        for (const frame of frames) {
          for (const v of frame) {
            expect(v).toBeGreaterThanOrEqual(0);
            expect(v).toBeLessThanOrEqual(maxPaletteIndex);
          }
        }
      }
    });
  });

  describe('调色板', () => {
    it('索引 0 为透明', () => {
      expect(PALETTE[0]).toBeNull();
    });

    it('可见颜色为 RGBA 数组', () => {
      for (let i = 1; i <= 5; i++) {
        expect(Array.isArray(PALETTE[i])).toBe(true);
        expect(PALETTE[i].length).toBe(4);
      }
    });
  });

  describe('getSprite 帧取模', () => {
    it('正常帧号返回对应帧', () => {
      expect(getSprite('idle', 0)).toBe(SPRITES.idle[0]);
    });

    it('越界帧号会回绕', () => {
      expect(getSprite('idle', SPRITES.idle.length)).toBe(SPRITES.idle[0]);
      expect(getSprite('idle', -1)).toBe(SPRITES.idle[SPRITES.idle.length - 1]);
    });

    it('未知状态 fallback 到 idle', () => {
      expect(getSprite('unknown', 0)).toBe(SPRITES.idle[0]);
    });

    it('focused 和 preparing 有专属帧，不 fallback 到 idle', () => {
      expect(getSprite('focused', 0)).toBe(SPRITES.focused[0]);
      expect(getSprite('preparing', 0)).toBe(SPRITES.preparing[0]);
      expect(getSprite('focused', 0)).not.toBe(SPRITES.idle[0]);
      expect(getSprite('preparing', 0)).not.toBe(SPRITES.idle[0]);
    });
  });

  describe('各状态帧数', () => {
    it('基础状态帧数匹配状态机配置', () => {
      expect(SPRITES.idle.length).toBe(7);
      expect(SPRITES.walk.length).toBe(4);
      expect(SPRITES.sleep.length).toBe(2);
      expect(SPRITES.talk.length).toBe(3);
      expect(SPRITES.happy.length).toBe(3);
      expect(SPRITES.confused.length).toBe(2);
      expect(SPRITES.focused.length).toBe(4);
      expect(SPRITES.preparing.length).toBe(4);
    });
  });

  describe('关键像素特征', () => {
    it('idle 闭眼帧眼睛位置为肤色', () => {
      expect(SPRITES.idle[2][6 * SPRITE_W + 3]).toBe(2);
    });

    it('idle variant 帧包含耳朵和视线变化', () => {
      expect(SPRITES.idle[4][0 * SPRITE_W + 3]).toBe(0);
      expect(SPRITES.idle[5][6 * SPRITE_W + 4]).toBe(4);
      expect(SPRITES.idle[6][6 * SPRITE_W + 3]).toBe(4);
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

    it('focused 有更亮的专注高光', () => {
      expect(SPRITES.focused[0][4 * SPRITE_W + 4]).toBe(3);
      expect(SPRITES.focused[3][6 * SPRITE_W + 4]).toBe(4);
    });

    it('preparing 有速度线和忙碌嘴型', () => {
      expect(SPRITES.preparing[0][1 * SPRITE_W + 1]).toBe(1);
      expect(SPRITES.preparing[0][8 * SPRITE_W + 8]).toBe(0);
    });
  });

  describe('cloneSprite 不修改原数组', () => {
    it('修改克隆不影响基底', () => {
      const original = SPRITES.idle[0].slice();
      const cloned = cloneSprite(SPRITES.idle[0], [[0, 0, 5]]);

      expect(cloned[0]).toBe(5);
      expect(original[0]).toBe(0);
      expect(SPRITES.idle[0][0]).toBe(0);
    });
  });
});
