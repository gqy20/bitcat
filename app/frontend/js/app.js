// app.js — Tauri 事件监听 + 主循环入口 + 右键菜单 + 折叠/展开 + 拖拽

(function() {
  'use strict';

  const pet = new PetState.PetStateMachine();
  let lastTime = performance.now();
  let canvas, ctx;
  let prevState = null;
  let bodyEl;

  // 折叠状态（默认值，initPullState 会从 Rust 侧拉取真实值）
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

  async function init() {
    canvas = document.getElementById('sprite');
    ctx = canvas.getContext('2d');
    bodyEl = document.body;

    // Pull 模式：从 Rust 侧拉取当前窗口状态（替代不可靠的 emit push）
    await initPullState();

    syncStateClass(pet.state);
    requestAnimationFrame(loop);
    setupTauriEvents();
    setupContextMenu();
    setupDrag();
  }

  // ========== Pull 模式：前端 init 时从 Rust 拉取窗口状态 ==========
  // 解决 emit 时序问题：build() 返回时 JS 的 __TAURI__ API 还未初始化，
  // listen() 还没注册，emit 的事件会丢失（Tauri Issue #7835/#9296）

  async function initPullState() {
    try {
      if (!window.__TAURI__ || !window.__TAURI__.core) return;
      const snap = await window.__TAURI__.core.invoke('cmd_get_window_state');
      console.log('[pet] pull 状态:', snap);

      if (snap.collapsed !== undefined) collapsed = snap.collapsed;
      if (snap.alwaysOnTop !== undefined) alwaysOnTop = snap.alwaysOnTop;

      // 只调整 canvas 尺寸和 CSS，不调用 cmd_recreate_pet_window（避免无限循环）
      // 窗口重建只由托盘事件驱动（pet-toggle-collapse）
      const w = collapsed ? 48 : 128;
      const h = collapsed ? 48 : 128;
      canvas.width = w;
      canvas.height = h;
      syncStateClass(pet.state);
    } catch (err) {
      console.warn('[pet] pull 状态失败，使用默认值:', err);
      collapsed = false;
      alwaysOnTop = true;
    }
  }

  // ========== 拖拽：Tauri 原生 startDragging（自动处理 DPI）==========

  function setupDrag() {
    const root = document.getElementById('pet-root');
    if (!root) return;
    root.addEventListener('mousedown', async (e) => {
      if (e.button !== 0) return;
      const win = getCurrentWin();
      if (win) try { await win.startDragging(); } catch (_) {}
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

  // ========== 右键菜单已统一到系统托盘 ==========

  function setupContextMenu() {
    bodyEl.addEventListener('contextmenu', (e) => {
      e.preventDefault();
    });
  }

  // ========== 折叠/展开（由 Rust 托盘事件驱动 + init pull）==========

  async function applyCollapse() {
    console.log('[pet] applyCollapse:', { collapsed, state: pet.state });

    const w = collapsed ? 48 : 128;
    const h = collapsed ? 48 : 128;
    canvas.width = w;
    canvas.height = h;
    console.log('[pet] canvas resize:', w, 'x', h);

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

  function applyAlwaysOnTop() {
    const win = getCurrentWin();
    if (win) {
      try { win.setAlwaysOnTop(alwaysOnTop); } catch (_) {}
    }
  }

  // ========== Tauri 事件 ==========

  function setupTauriEvents() {
    if (!window.__TAURI__) return;

    window.__TAURI__.event.listen('pet-event', (event) => {
      pet.applyEvent(event.payload);
    });

    // 托盘实时事件：折叠/展开切换（非 init，是用户主动操作）
    window.__TAURI__.event.listen('pet-toggle-collapse', (event) => {
      console.log('[pet] 收到 pet-toggle-collapse:', event.payload);
      collapsed = event.payload;
      applyCollapse();
    });

    // 托盘实时事件：置顶切换
    window.__TAURI__.event.listen('pet-toggle-top', (event) => {
      console.log('[pet] 收到 pet-toggle-top:', event.payload);
      alwaysOnTop = event.payload;
      applyAlwaysOnTop();
    });
  }

  document.addEventListener('DOMContentLoaded', init);
})();
