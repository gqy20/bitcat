// crossfade.test.js — 双窗口切换时的 body 淡入/淡出契约（Vitest + jsdom）
//
// Task 4 背景：pet → pet-snap（反之亦然）切换时，旧窗口 hide 和新窗口
// show 会出现瞬时双闪。方案：CSS opacity transition 驱动
//   body.fading-out  透明度 1 → 0
//   body.fading-in   透明度 0 → 1
//   body.pre-fade    初始强制 opacity:0（show 后一帧移除，即触发 fade-in）
//
// Rust 侧通过 eval 调用 window.__fadeOut() / window.__fadeIn()
// 本测试验证这两个函数的前端契约。

import { describe, it, expect, beforeEach, vi } from 'vitest';

function createPetDOM() {
  document.body.innerHTML = '';
  document.body.className = '';
}

/// 契约版 fade 工具（setTimeout 兜底，独立于 transitionend）
const FADE_MS = 150;
function installFade() {
  window.__fadeOut = function() {
    return new Promise((resolve) => {
      document.body.classList.remove('fading-in', 'pre-fade');
      document.body.classList.add('fading-out');
      setTimeout(resolve, FADE_MS);
    });
  };
  window.__fadeIn = function() {
    return new Promise((resolve) => {
      document.body.classList.remove('fading-out');
      document.body.classList.add('pre-fade');
      // 触发 reflow 后移除 pre-fade 并加 fading-in
      setTimeout(() => {
        document.body.classList.remove('pre-fade');
        document.body.classList.add('fading-in');
        setTimeout(resolve, FADE_MS);
      }, 0);
    });
  };
}

describe('crossfade body transitions', () => {
  beforeEach(() => {
    createPetDOM();
    vi.useFakeTimers();
    installFade();
  });

  it('A: body 默认无 fading-* / pre-fade class', () => {
    expect(document.body.classList.contains('fading-in')).toBe(false);
    expect(document.body.classList.contains('fading-out')).toBe(false);
    expect(document.body.classList.contains('pre-fade')).toBe(false);
  });

  it('B: __fadeOut 加 fading-out，并在 FADE_MS 后 resolve', async () => {
    const p = window.__fadeOut();
    expect(document.body.classList.contains('fading-out')).toBe(true);
    expect(document.body.classList.contains('fading-in')).toBe(false);
    await vi.advanceTimersByTimeAsync(FADE_MS);
    await p; // 不应抛出/悬挂
  });

  it('C: __fadeIn 先 pre-fade，一帧后切 fading-in；FADE_MS 后 resolve', async () => {
    const p = window.__fadeIn();
    expect(document.body.classList.contains('pre-fade')).toBe(true);
    expect(document.body.classList.contains('fading-in')).toBe(false);

    await vi.advanceTimersByTimeAsync(0);
    expect(document.body.classList.contains('pre-fade')).toBe(false);
    expect(document.body.classList.contains('fading-in')).toBe(true);

    await vi.advanceTimersByTimeAsync(FADE_MS);
    await p;
  });

  it('D: __fadeIn 会清除残留的 fading-out', async () => {
    document.body.classList.add('fading-out');
    const p = window.__fadeIn();
    expect(document.body.classList.contains('fading-out')).toBe(false);
    await vi.advanceTimersByTimeAsync(FADE_MS + 1);
    await p;
  });

  it('E: __fadeOut 会清除残留的 fading-in 与 pre-fade（切换方向不累加）', async () => {
    document.body.classList.add('fading-in', 'pre-fade');
    const p = window.__fadeOut();
    expect(document.body.classList.contains('fading-in')).toBe(false);
    expect(document.body.classList.contains('pre-fade')).toBe(false);
    expect(document.body.classList.contains('fading-out')).toBe(true);
    await vi.advanceTimersByTimeAsync(FADE_MS);
    await p;
  });

  it('F: 连续多次 __fadeOut 幂等（不累加 class）', async () => {
    await (async () => {
      const p1 = window.__fadeOut();
      const p2 = window.__fadeOut();
      await vi.advanceTimersByTimeAsync(FADE_MS + 1);
      await Promise.all([p1, p2]);
    })();
    // classList 不支持重复项，但至少确保只留 fading-out
    expect(document.body.classList.contains('fading-out')).toBe(true);
    expect(document.body.classList.length).toBe(1);
  });
});
