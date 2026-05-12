// snapbar.test.js — 吸附条 DOM 契约测试（Vitest + jsdom）
//
// 重构背景：原 setupSnapBar 用 Canvas + requestAnimationFrame 绘制
// 多层渐变，导致方向切换需要 eval 注入、视觉更新依赖帧循环。
// 新契约：Canvas → DOM + CSS（呼吸/渐变/展宽全部走 CSS animation/transition）。
//
// 契约节点（pet.html 预置）：
//   <div id="snap-bar" hidden>
//     <div class="snap-layer-outer"></div>
//     <div class="snap-layer-mid"></div>
//     <div class="snap-core"></div>
//     <div class="snap-dot"></div>
//   </div>
// 容器通过 class 切换状态：
//   direction-left / direction-right  —— 方向
//   hovered                           —— hover 展宽
//   （hidden 属性）                   —— 未吸附时隐藏

import { describe, it, expect, beforeEach } from 'vitest';

function createSnapBarDOM() {
  document.body.innerHTML = `
    <div id="pet-root">
      <canvas id="sprite" width="24" height="100"></canvas>
      <div id="snap-bar" hidden>
        <div class="snap-layer-outer"></div>
        <div class="snap-layer-mid"></div>
        <div class="snap-core"></div>
        <div class="snap-dot"></div>
      </div>
    </div>
  `;
  return document.getElementById('snap-bar');
}

/// 契约版 setupSnapBar：纯 DOM + class，无 canvas/rAF
function setupSnapBar(initialEdge, opts) {
  var bar = document.getElementById('snap-bar');
  if (!bar) return null;
  opts = opts || {};

  function setEdge(edge) {
    bar.classList.remove('direction-left', 'direction-right');
    bar.classList.add(edge === 'right' ? 'direction-right' : 'direction-left');
  }
  setEdge(initialEdge || 'left');
  bar.hidden = false;

  bar.addEventListener('mouseenter', function() { bar.classList.add('hovered'); });
  bar.addEventListener('mouseleave', function() { bar.classList.remove('hovered'); });
  bar.addEventListener('mousedown', function(e) {
    if (e.button !== 0) return;
    if (typeof opts.onUnsnap === 'function') opts.onUnsnap();
  });

  // 运行时方向切换兜底（lib.rs eval 调用）
  window.__setSnapEdge = function(edge) { setEdge(edge); };

  return { setEdge: setEdge };
}

describe('snap bar DOM contract', () => {
  beforeEach(() => {
    createSnapBarDOM();
    delete window.__setSnapEdge;
  });

  it('A: 初始结构包含四层子节点且默认隐藏', () => {
    const bar = document.getElementById('snap-bar');
    expect(bar).toBeTruthy();
    expect(bar.hidden).toBe(true);
    expect(bar.querySelector('.snap-layer-outer')).toBeTruthy();
    expect(bar.querySelector('.snap-layer-mid')).toBeTruthy();
    expect(bar.querySelector('.snap-core')).toBeTruthy();
    expect(bar.querySelector('.snap-dot')).toBeTruthy();
  });

  it('B: setupSnapBar("left") 激活左向 class 并显示', () => {
    setupSnapBar('left');
    const bar = document.getElementById('snap-bar');
    expect(bar.hidden).toBe(false);
    expect(bar.classList.contains('direction-left')).toBe(true);
    expect(bar.classList.contains('direction-right')).toBe(false);
  });

  it('C: setupSnapBar("right") 激活右向 class', () => {
    setupSnapBar('right');
    const bar = document.getElementById('snap-bar');
    expect(bar.classList.contains('direction-right')).toBe(true);
    expect(bar.classList.contains('direction-left')).toBe(false);
  });

  it('D: 未指定方向时默认为 left（与 pull None 一致）', () => {
    setupSnapBar(null);
    const bar = document.getElementById('snap-bar');
    expect(bar.classList.contains('direction-left')).toBe(true);
  });

  it('E: __setSnapEdge 运行时切换 direction，不重复累加', () => {
    setupSnapBar('left');
    const bar = document.getElementById('snap-bar');
    window.__setSnapEdge('right');
    expect(bar.classList.contains('direction-right')).toBe(true);
    expect(bar.classList.contains('direction-left')).toBe(false);
    window.__setSnapEdge('left');
    expect(bar.classList.contains('direction-left')).toBe(true);
    expect(bar.classList.contains('direction-right')).toBe(false);
  });

  it('F: mouseenter/mouseleave 切换 hovered class', () => {
    setupSnapBar('left');
    const bar = document.getElementById('snap-bar');
    expect(bar.classList.contains('hovered')).toBe(false);
    bar.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
    expect(bar.classList.contains('hovered')).toBe(true);
    bar.dispatchEvent(new MouseEvent('mouseleave', { bubbles: true }));
    expect(bar.classList.contains('hovered')).toBe(false);
  });

  it('G: 左键点击触发 onUnsnap 回调', () => {
    let called = 0;
    setupSnapBar('left', { onUnsnap: function() { called++; } });
    const bar = document.getElementById('snap-bar');
    bar.dispatchEvent(new MouseEvent('mousedown', { button: 0, bubbles: true }));
    expect(called).toBe(1);
  });

  it('H: 右键/中键不触发 onUnsnap', () => {
    let called = 0;
    setupSnapBar('left', { onUnsnap: function() { called++; } });
    const bar = document.getElementById('snap-bar');
    bar.dispatchEvent(new MouseEvent('mousedown', { button: 2, bubbles: true }));
    bar.dispatchEvent(new MouseEvent('mousedown', { button: 1, bubbles: true }));
    expect(called).toBe(0);
  });
});
