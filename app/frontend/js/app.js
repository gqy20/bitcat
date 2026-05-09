// app.js — Tauri 事件监听 + 主循环入口

(function() {
  'use strict';

  const pet = new PetState.PetStateMachine();
  let lastTime = performance.now();
  let canvas, ctx;
  let prevState = null;
  let bodyEl;

  function init() {
    canvas = document.getElementById('sprite');
    ctx = canvas.getContext('2d');
    bodyEl = document.body;
    syncStateClass(pet.state);
    requestAnimationFrame(loop);
    setupTauriEvents();
  }

  function loop(now) {
    const dt = now - lastTime;
    lastTime = now;
    pet.update(dt);

    // 状态切换 → 同步 body class + 触发闪烁 + 粒子
    if (pet.state !== prevState) {
      syncStateClass(pet.state);
      flashSprite();
      Particles.onStateEnter(pet.state);
      prevState = pet.state;
    }

    // 渲染精灵（多帧 + 左右翻转）
    SpriteRenderer.renderSprite(ctx, pet.state, pet.frame, pet.facingRight, 8);

    // 持续粒子（睡觉的 Z、困惑的 ?）
    Particles.tick(pet.state, dt);

    requestAnimationFrame(loop);
  }

  function syncStateClass(state) {
    // 移除 state-xxx 旧类，添加新的
    bodyEl.className = bodyEl.className
      .split(/\s+/)
      .filter(c => !c.startsWith('state-'))
      .concat(`state-${state}`)
      .join(' ');
  }

  function flashSprite() {
    canvas.classList.remove('flash');
    // 强制 reflow 让 animation 重新触发
    void canvas.offsetWidth;
    canvas.classList.add('flash');
  }

  function setupTauriEvents() {
    // Tauri 环境
    if (window.__TAURI__) {
      window.__TAURI__.event.listen('pet-event', (event) => {
        // bubble 字段已迁移到独立气泡窗口，pet 端忽略
        pet.applyEvent(event.payload);
      });
    }
  }

  document.addEventListener('DOMContentLoaded', init);
})();
