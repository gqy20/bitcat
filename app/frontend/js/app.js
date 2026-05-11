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

  // 吸附状态
  let isSnapped = false;
  let snapEdge = null;  // 'left' | 'right'

  // 当前窗口是否为吸附竖条窗口
  let isSnapWindow = false;

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

    // 检测当前窗口类型
    const win = getCurrentWin();
    if (win) {
      isSnapWindow = win.label === 'pet-snap';
      console.log('[pet] 窗口 label:', win.label, 'isSnapWindow:', isSnapWindow);
    }

    if (isSnapWindow) {
      // 吸附竖条模式：不启动宠物渲染循环，只显示发光条 + 监听点击
      setupSnapBar();
      return;
    }

    // Pull 模式：从 Rust 侧拉取当前窗口状态（替代不可靠的 emit push）
    await initPullState();

    syncStateClass(pet.state);
    requestAnimationFrame(loop);
    setupTauriEvents();
    setupContextMenu();
    setupDrag();
  }

  /// 吸附竖条模式：精致发光细线 + 脉冲呼吸动画 + 点击恢复
  function setupSnapBar() {
    const w = canvas.width = 8;
    const h = canvas.height = 100;
    let edgeReversed = false;
    let breathPhase = 0;

    function drawGlow() {
      ctx.clearRect(0, 0, w, h);

      // 呼吸脉冲：亮度在 0.6~1.0 之间缓慢波动
      const breath = 0.6 + 0.4 * (0.5 + 0.5 * Math.sin(breathPhase));
      const alpha = breath;

      // 发光核心：2px 宽的亮线
      const coreX = edgeReversed ? w - 2 : 0;
      ctx.fillStyle = `rgba(96, 216, 255, ${(0.95 * alpha).toFixed(2)})`;
      ctx.fillRect(coreX, 4, 2, h - 8);

      // 外层柔光：渐变扩散
      const x0 = edgeReversed ? w : 0;
      const x1 = edgeReversed ? 0 : w;
      const grad = ctx.createLinearGradient(x0, 0, x1, 0);
      grad.addColorStop(0, `rgba(96, 216, 255, ${(0.45 * alpha).toFixed(2)})`);
      grad.addColorStop(0.4, `rgba(96, 216, 255, ${(0.15 * alpha).toFixed(2)})`);
      grad.addColorStop(1, 'rgba(96, 216, 255, 0)');
      ctx.fillStyle = grad;

      // 圆角矩形
      const r = 3;
      ctx.beginPath();
      ctx.moveTo(0, r);
      ctx.quadraticCurveTo(0, 0, r, 0);
      ctx.lineTo(w - r, 0);
      ctx.quadraticCurveTo(w, 0, w, r);
      ctx.lineTo(w, h - r);
      ctx.quadraticCurveTo(w, h, w - r, h);
      ctx.lineTo(r, h);
      ctx.quadraticCurveTo(0, h, 0, h - r);
      ctx.closePath();
      ctx.fill();
    }

    // 呼吸动画循环
    function animateBreath() {
      breathPhase += 0.03;
      drawGlow();
      requestAnimationFrame(animateBreath);
    }
    animateBreath();

    // 点击恢复宠物
    const root = document.getElementById('pet-root');
    if (root) {
      root.addEventListener('mousedown', async (e) => {
        if (e.button !== 0) return;
        console.log('[pet-snap] 点击竖条，恢复宠物');
        await cmdUnsnapTransform();
      });
    }

    // 监听 edge 方向（用于翻转渐变方向）
    if (window.__TAURI__ && window.__TAURI__.event) {
      window.__TAURI__.event.listen('snap-edge', (event) => {
        edgeReversed = event.payload === 'right';
      });
    }
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

  // ========== 拖拽：原生 startDragging + 松手贴边吸附 ==========

  function setupDrag() {
    const root = document.getElementById('pet-root');
    if (!root) return;

    root.addEventListener('mousedown', async (e) => {
      if (e.button !== 0) return;
      const win = getCurrentWin();
      if (!win) return;

      // 原生拖拽必须在 mousedown 里同步调用
      try { await win.startDragging(); } catch (_) {}
      startSnapPoll(win);
    });
  }

  /// 轮询检测拖拽结束，判断是否需要吸附
  function startSnapPoll(win) {
    let lastKey = '';
    let stableCount = 0;
    const pollId = setInterval(async () => {
      try {
        const pos = await win.outerPosition();
        const key = pos.x + ',' + pos.y;
        if (key === lastKey) {
          stableCount++;
          if (stableCount >= 3) {
            clearInterval(pollId);
            console.log('[pet] 拖拽结束，位置:', pos);
            // 调用 Rust 计算吸附目标（只有靠近边缘才返回有效结果）
            const result = await cmdSnapPet(pos.x, pos.y);
            console.log('[pet] cmd_snap_pet 结果:', result);
            if (result && result.edge && result.edge !== 'none') {
              const { edge, x: toX, y: toY } = result;
              console.log('[pet] 吸附到', edge, toX, toY);
              // 先动画到吸附位置，再切换为竖条窗口
              await animateSnap(win, pos.x, pos.y, toX, toY);
              // 等动画完成再切换窗口
              await new Promise(r => setTimeout(r, 320));
              await cmdSnapTransform(edge, toX, toY);
            }
          }
        } else {
          lastKey = key;
          stableCount = 0;
        }
      } catch (_) { clearInterval(pollId); }
    }, 100);
  }

  /// 调用 Rust cmd_snap_pet，返回 { edge, x, y }
  async function cmdSnapPet(x, y) {
    try {
      return await window.__TAURI__.core.invoke('cmd_snap_pet', { x, y });
    } catch (e) { console.error('[pet] cmd_snap_pet 失败:', e); return {}; }
  }

  /// 调用 Rust cmd_snap_transform，将宠物窗口切换为竖条
  async function cmdSnapTransform(edge, x, y) {
    try {
      await window.__TAURI__.core.invoke('cmd_snap_transform', { edge, x, y });
    } catch (e) { console.error('[pet] cmd_snap_transform 失败:', e); }
  }

  /// 调用 Rust cmd_unsnap_transform，恢复宠物窗口
  async function cmdUnsnapTransform() {
    try {
      await window.__TAURI__.core.invoke('cmd_unsnap_transform');
    } catch (e) { console.error('[pet] cmd_unsnap_transform 失败:', e); }
  }

  /// 弹簧缓动：轻微回弹，模拟"吸附"质感
  function easeOutBack(t) {
    const c1 = 1.70158;
    const c3 = c1 + 1;
    return 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2);
  }

  /// 标准缓动
  function easeOutCubic(t) {
    return 1 - Math.pow(1 - t, 3);
  }

  /// 从当前位置动画滑到吸附目标
  async function animateSnap(win, fromX, fromY, toX, toY) {
    console.log('[pet] animateSnap 开始:', { fromX, fromY, toX, toY, scale: win.scaleFactor });
    // outerPosition() 返回物理像素，PhysicalPosition 也用物理像素，保持一致
    const duration = 300;
    const start = performance.now();
    const Pos = window.__TAURI__.window.PhysicalPosition;

    function frame(now) {
      const t = Math.min((now - start) / duration, 1);
      const e = easeOutBack(t);
      const cx = Math.round(fromX + (toX - fromX) * e);
      const cy = Math.round(fromY + (toY - fromY) * e);
      win.setPosition(new Pos(cx, cy));
      if (t < 1) {
        requestAnimationFrame(frame);
      } else {
        console.log('[pet] animateSnap 完成，位置:', cx, cy);
      }
    }
    requestAnimationFrame(frame);
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
