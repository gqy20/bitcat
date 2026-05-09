// app.js — Tauri 事件监听 + 主循环入口 + 右键菜单 + 折叠/展开 + 拖拽

(function() {
  'use strict';

  const pet = new PetState.PetStateMachine();
  let lastTime = performance.now();
  let canvas, ctx;
  let prevState = null;
  let bodyEl;

  // 折叠状态
  let collapsed = false;
  let alwaysOnTop = true;

  // Tauri 2 正确的 API 路径：getCurrentWindow() 不是 getCurrent()
  function getCurrentWin() {
    try {
      return window.__TAURI__.window.getCurrentWindow();
    } catch (e) {
      console.error('[pet] getCurrentWindow 失败:', e);
      return null;
    }
  }

  function init() {
    canvas = document.getElementById('sprite');
    ctx = canvas.getContext('2d');
    bodyEl = document.body;
    syncStateClass(pet.state);
    requestAnimationFrame(loop);
    setupTauriEvents();
    setupContextMenu();
    setupDrag();
  }

  // ========== 拖拽：纯 JS 手动跟踪 ==========

  let dragState = null; // { startX, startY, winX, winY }

  function setupDrag() {
    const root = document.getElementById('pet-root');
    if (!root) return;

    root.addEventListener('mousedown', async (e) => {
      if (e.button !== 0) return;
      if (e.target && e.target.closest('.ctx-menu')) return;

      const win = getCurrentWin();
      if (!win) return;

      try {
        const pos = await win.outerPosition();
        dragState = {
          startX: e.screenX,
          startY: e.screenY,
          winX: pos.x,
          winY: pos.y,
        };
      } catch (err) {
        console.error('[pet] outerPosition 失败:', err);
      }
    });

    document.addEventListener('mousemove', async (e) => {
      if (!dragState) return;
      const win = getCurrentWin();
      if (!win) return;
      try {
        await win.setPosition({
          x: dragState.winX + (e.screenX - dragState.startX),
          y: dragState.winY + (e.screenY - dragState.startY),
        });
      } catch (err) {
        // setPosition 在拖拽中频繁调用，偶尔失败不打印避免刷屏
      }
    });

    document.addEventListener('mouseup', () => {
      dragState = null;
    });
  }

  // ========== 主循环 ==========

  function loop(now) {
    const dt = now - lastTime;
    lastTime = now;

    if (!collapsed) {
      pet.update(dt);

      if (pet.state !== prevState) {
        syncStateClass(pet.state);
        flashSprite();
        Particles.onStateEnter(pet.state);
        prevState = pet.state;
      }

      SpriteRenderer.renderSprite(ctx, pet.state, pet.frame, pet.facingRight, 8);
      Particles.tick(pet.state, dt);
    } else {
      SpriteRenderer.renderMini(ctx, pet.state);
    }

    requestAnimationFrame(loop);
  }

  function syncStateClass(state) {
    bodyEl.className = bodyEl.className
      .split(/\s+/)
      .filter(c => !c.startsWith('state-'))
      .concat(`state-${state}`, collapsed ? 'collapsed' : '')
      .join(' ')
      .trim();
  }

  function flashSprite() {
    canvas.classList.remove('flash');
    void canvas.offsetWidth;
    canvas.classList.add('flash');
  }

  // ========== 右键上下文菜单 ==========

  function setupContextMenu() {
    const menu = document.getElementById('ctx-menu');
    const ctxCollapse = document.getElementById('ctx-collapse');
    const ctxTop = document.getElementById('ctx-top');
    const ctxExit = document.getElementById('ctx-exit');

    bodyEl.addEventListener('contextmenu', (e) => {
      e.preventDefault();

      const mx = e.clientX || 0;
      const my = e.clientY || 0;
      menu.style.left = Math.min(mx, window.innerWidth - 130) + 'px';
      menu.style.top = Math.min(my, window.innerHeight - 120) + 'px';
      menu.classList.remove('hidden');

      ctxCollapse.textContent = collapsed ? '展开' : '折叠';
      ctxTop.textContent = alwaysOnTop ? '取消置顶' : '置顶';
    });

    document.addEventListener('mousedown', (e) => {
      if (!menu.contains(e.target)) {
        menu.classList.add('hidden');
      }
    });

    ctxCollapse.addEventListener('click', () => {
      toggleCollapse();
      menu.classList.add('hidden');
    });

    ctxTop.addEventListener('click', () => {
      toggleAlwaysOnTop();
      menu.classList.add('hidden');
    });

    ctxExit.addEventListener('click', () => {
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('tauri:exit').catch(() => {});
      }
      menu.classList.add('hidden');
    });
  }

  // ========== 折叠/展开（Rust 侧销毁重建）==========

  async function toggleCollapse() {
    collapsed = !collapsed;
    syncStateClass(pet.state);

    const w = collapsed ? 48 : 128;
    const h = collapsed ? 48 : 128;
    canvas.width = w;
    canvas.height = h;

    // 取当前位置，重建后恢复到同一位置
    const win = getCurrentWin();
    let posX = 0, posY = 0;
    if (win) {
      try {
        const pos = await win.outerPosition();
        posX = pos.x;
        posY = pos.y;
      } catch (_) {}
    }

    if (window.__TAURI__ && window.__TAURI__.core) {
      try {
        await window.__TAURI__.core.invoke('cmd_recreate_pet_window', {
          width: w, height: h, x: posX, y: posY
        });
      } catch (err) {
        console.error('[pet] recreate 窗口失败:', err);
      }
    }
  }

  async function toggleAlwaysOnTop() {
    alwaysOnTop = !alwaysOnTop;
    const win = getCurrentWin();
    if (win) {
      try { await win.setAlwaysOnTop(alwaysOnTop); } catch (_) {}
    }
  }

  // ========== Tauri 事件 ==========

  function setupTauriEvents() {
    if (window.__TAURI__) {
      window.__TAURI__.event.listen('pet-event', (event) => {
        pet.applyEvent(event.payload);
      });
    }
  }

  document.addEventListener('DOMContentLoaded', init);
})();
