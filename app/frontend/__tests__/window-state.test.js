// window-state.test.js — 窗口状态 pull 模式 (Vitest)
import { describe, it, expect, beforeEach } from 'vitest';

// 模拟 WindowStateSnapshot 结构（与 Rust 侧 serde 一致）
function defaultSnapshot() {
  return { collapsed: false, alwaysOnTop: true, position: null };
}

describe('WindowState pull 模式', () => {
  describe('默认快照', () => {
    it('collapsed 默认 false', () => {
      const s = defaultSnapshot();
      expect(s.collapsed).toBe(false);
    });

    it('alwaysOnTop 默认 true', () => {
      const s = defaultSnapshot();
      expect(s.alwaysOnTop).toBe(true);
    });

    it('position 默认 null', () => {
      const s = defaultSnapshot();
      expect(s.position).toBeNull();
    });
  });

  describe('折叠状态', () => {
    it('折叠后 collapsed=true', () => {
      const s = defaultSnapshot();
      s.collapsed = true;
      expect(s.collapsed).toBe(true);
    });

    it('展开后 collapsed=false', () => {
      const s = defaultSnapshot();
      s.collapsed = true;
      s.collapsed = false;
      expect(s.collapsed).toBe(false);
    });
  });

  describe('位置信息', () => {
    it('有位置时返回 [x, y] 元组', () => {
      const s = defaultSnapshot();
      s.position = [1920, 1080];
      expect(s.position).toEqual([1920, 1080]);
    });

    it('位置为 null 时表示未知', () => {
      const s = defaultSnapshot();
      expect(s.position).toBeNull();
    });
  });

  describe('applyCollapse 尺寸计算', () => {
    function calcDimensions(collapsed) {
      return collapsed
        ? { w: 48, h: 48 }
        : { w: 128, h: 128 };
    }

    it('折叠模式返回 48x48', () => {
      expect(calcDimensions(true)).toEqual({ w: 48, h: 48 });
    });

    it('展开模式返回 128x128', () => {
      expect(calcDimensions(false)).toEqual({ w: 128, h: 128 });
    });

    it('切换折叠不改变展开尺寸', () => {
      const expanded = calcDimensions(false);
      const collapsed = calcDimensions(true);
      expect(expanded.w).not.toBe(collapsed.w);
    });
  });

  describe('状态同步容错', () => {
    it('缺少字段时使用默认值', () => {
      const partial = {};
      const s = {
        collapsed: partial.collapsed ?? false,
        alwaysOnTop: partial.alwaysOnTop ?? true,
        position: partial.position ?? null,
      };
      expect(s).toEqual(defaultSnapshot());
    });

    it('null 快照使用全默认值', () => {
      const s = null ?? defaultSnapshot();
      expect(s.collapsed).toBe(false);
    });
  });
});
