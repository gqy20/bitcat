// app.js — Tauri 事件监听 + 主循环入口

(function() {
  'use strict';

  const pet = new PetState.PetStateMachine();
  let lastTime = performance.now();
  let canvas, ctx;

  function init() {
    canvas = document.getElementById('sprite');
    ctx = canvas.getContext('2d');
    requestAnimationFrame(loop);
    setupTauriEvents();
  }

  function loop(now) {
    const dt = now - lastTime;
    lastTime = now;
    pet.update(dt);

    // 渲染精灵
    SpriteRenderer.renderSprite(ctx, pet.state, 8);

    // 渲染气泡
    updateBubble();

    requestAnimationFrame(loop);
  }

  function updateBubble() {
    const el = document.getElementById('bubble');
    if (pet.bubble) {
      el.textContent = pet.bubble;
      el.classList.remove('hidden');
    } else {
      el.classList.add('hidden');
    }
  }

  function setupTauriEvents() {
    // Tauri 环境
    if (window.__TAURI__) {
      window.__TAURI__.event.listen('pet-event', (event) => {
        pet.applyEvent(event.payload);
      });
    }
  }

  document.addEventListener('DOMContentLoaded', init);
})();
