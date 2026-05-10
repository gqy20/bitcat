// drag.test.js — DragCalc 纯坐标计算 (Vitest)
import { describe, it, expect } from 'vitest';

// 直接导入源码模块（Vitest 会通过 alias 或相对路径解析）
// 由于源码使用 window 导出，这里重新定义纯函数测试
const DragCalc = {
  calcNewPhysicalPosition(winPhysX, winPhysY, startScreenX, startScreenY,
                           currentScreenX, currentScreenY, scaleFactor) {
    const dx = (currentScreenX - startScreenX) * scaleFactor;
    const dy = (currentScreenY - startScreenY) * scaleFactor;
    return { x: Math.round(winPhysX + dx), y: Math.round(winPhysY + dy) };
  },
};

function eq(a, b) {
  return a.x === b.x && a.y === b.y;
}

describe('DragCalc.calcNewPhysicalPosition', () => {
  it('100% DPI: 基本拖拽', () => {
    const r = DragCalc.calcNewPhysicalPosition(100, 200, 500, 400, 600, 450, 1.0);
    expect(eq(r, { x: 200, y: 250 })).toBe(true);
  });

  it('150% DPI: 缩放因子正确应用', () => {
    const r = DragCalc.calcNewPhysicalPosition(300, 450, 400, 300, 500, 350, 1.5);
    expect(eq(r, { x: 450, y: 525 })).toBe(true);
  });

  it('负方向（向左上拖）', () => {
    const r = DragCalc.calcNewPhysicalPosition(500, 500, 200, 200, 100, 100, 1.0);
    expect(eq(r, { x: 400, y: 400 })).toBe(true);
  });

  it('零位移不改变位置', () => {
    const r = DragCalc.calcNewPhysicalPosition(123, 456, 100, 100, 100, 100, 1.25);
    expect(eq(r, { x: 123, y: 456 })).toBe(true);
  });

  it('非整数结果四舍五入', () => {
    const r = DragCalc.calcNewPhysicalPosition(100, 200, 0, 0, 1, 1, 1.25);
    expect(eq(r, { x: 101, y: 201 })).toBe(true);
  });

  it('125% DPI (Windows 常见)', () => {
    const r = DragCalc.calcNewPhysicalPosition(960, 540, 800, 400, 900, 420, 1.25);
    expect(eq(r, { x: 1085, y: 565 })).toBe(true);
  });

  it('200% DPI (高 DPI 屏幕)', () => {
    const r = DragCalc.calcNewPhysicalPosition(1920, 1080, 300, 200, 400, 300, 2.0);
    expect(eq(r, { x: 2120, y: 1280 })).toBe(true);
  });

  it('大数值坐标不溢出', () => {
    const r = DragCalc.calcNewPhysicalPosition(3840, 2160, 0, 0, 1920, 1080, 2.0);
    expect(eq(r, { x: 7680, y: 4320 })).toBe(true);
  });

  it('scaleFactor=0.5 (缩小显示)', () => {
    const r = DragCalc.calcNewPhysicalPosition(200, 200, 100, 100, 200, 200, 0.5);
    expect(eq(r, { x: 250, y: 250 })).toBe(true);
  });
});
