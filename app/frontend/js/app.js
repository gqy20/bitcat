// app.js — Tauri 事件监听 + 主循环入口 + 右键菜单 + 折叠/展开 + 拖拽

(function() {
  'use strict';

  // ---- 嘴巴热区判定（纯函数，可独立测试）----
  // 热区覆盖精灵嘴巴/腮红区域：正常态 x[32,96] y[56,96] (canvas 坐标)
  // 折叠态按 canvasSize 比例缩放
  function isMouthHotzone(x, y, canvasSize) {
    var ratio = canvasSize / 128;
    return x >= 32 * ratio && x <= 96 * ratio && y >= 56 * ratio && y <= 96 * ratio;
  }

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

  /// 吸附竖条模式：多层发光 + hover 展宽 + 物理呼吸 + 点击恢复
  function setupSnapBar() {
    const w = canvas.width = 24;   // 更宽的热区（视觉上只画边缘 2px）
    const h = canvas.height = 100;

    let edgeReversed = false;     // right=true 时画在右侧
    let hovered = false;          // 鼠标悬停状态
    let breathPhase = 0;          // 呼吸相位
    let lastTime = performance.now();

    // ---- 绘制 ----
    function drawGlow() {
      ctx.clearRect(0, 0, w, h);

      // 基于时间差的物理呼吸（不依赖帧率）
      const now = performance.now();
      const dt = (now - lastTime) / 1000;
      lastTime = now;
      breathPhase += dt * 1.8; // 约 3.5s 一个完整周期

      // 双层呼吸：核心快 + 外层慢（相位差营造层次感）
      const coreBreath = 0.65 + 0.35 * (0.5 + 0.5 * Math.sin(breathPhase));
      const outerBreath = 0.55 + 0.30 * (0.5 + 0.5 * Math.sin(breathPhase * 0.7));

      // hover 时整体提亮
      const hoverBoost = hovered ? 0.25 : 0;

      // 视觉宽度：默认 2px，hover 时展宽到 6px
      const visualW = hovered ? 6 : 2;
      const coreX = edgeReversed ? w - visualW : 0;

      // ===== 第一层：外层柔光（宽范围低透明度）=====
      const glowW = Math.min(w, hovered ? 20 : 12); // hover 时柔光更广
      const gx0 = edgeReversed ? w - glowW : 0;
      const gx1 = edgeReversed ? w : glowW;
      const outerGrad = ctx.createLinearGradient(gx0, 0, gx1, 0);
      const oa = (outerBreath + hoverBoost) * 0.35;
      outerGrad.addColorStop(0, `rgba(99, 102, 241, ${oa.toFixed(2)})`);
      outerGrad.addColorStop(0.5, `rgba(99, 102, 241, ${(oa * 0.4).toFixed(2)})`);
      outerGrad.addColorStop(1, 'rgba(99, 102, 241, 0)');
      ctx.fillStyle = outerGrad;
      roundRect(ctx, 0, 0, w, h, 4);
      ctx.fill();

      // ===== 第二层：中层辉光（窄范围中透明度）=====
      const midW = Math.min(w, hovered ? 14 : 8);
      const mx0 = edgeReversed ? w - midW : 0;
      const mx1 = edgeReversed ? w : midW;
      const midGrad = ctx.createLinearGradient(mx0, 0, mx1, 0);
      const ma = (coreBreath + hoverBoost) * 0.55;
      midGrad.addColorStop(0, `rgba(139, 92, 246, ${ma.toFixed(2)})`);
      midGrad.addColorStop(0.6, `rgba(139, 92, 246, ${(ma * 0.35).toFixed(2)})`);
      midGrad.addColorStop(1, 'rgba(139, 92, 246, 0)');
      ctx.fillStyle = midGrad;
      roundRect(ctx, 0, 0, w, h, 3);
      ctx.fill();

      // ===== 第三层：高亮核心（最细最亮）=====
      const ca = (coreBreath + hoverBoost) * 0.95;
      ctx.fillStyle = `rgba(165, 180, 252, ${ca.toFixed(2)})`;
      ctx.fillRect(coreX, 4, visualW, h - 8);

      // hover 时在核心旁画一个微小的"点击提示"亮点
      if (hovered) {
        const dotY = h / 2 + Math.sin(breathPhase * 2) * 8;
        const dotX = edgeReversed ? w - visualW - 4 : visualW + 4;
        ctx.fillStyle = `rgba(200, 210, 255, ${(0.6 * coreBreath).toFixed(2)})`;
        ctx.beginPath();
        ctx.arc(dotX, dotY, 1.5, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    /// 圆角矩形辅助函数
    function roundRect(c, x, y, rw, rh, r) {
      c.beginPath();
      c.moveTo(x + r, y);
      c.lineTo(x + rw - r, y);
      c.quadraticCurveTo(x + rw, y, x + rw, y + r);
      c.lineTo(x + rw, y + rh - r);
      c.quadraticCurveTo(x + rw, y + rh, x + rw - r, y + rh);
      c.lineTo(x + r, y + rh);
      c.quadraticCurveTo(x, y + rh, x, y + rh - r);
      c.lineTo(x, y + r);
      c.quadraticCurveTo(x, y, x + r, y);
      c.closePath();
    }

    // 动画循环
    function animateBreath() {
      drawGlow();
      requestAnimationFrame(animateBreath);
    }
    animateBreath();

    // ---- 交互：hover 检测 ----
    // Tauri 透明窗口的整个区域都可接收鼠标事件
    canvas.addEventListener('mouseenter', () => { hovered = true; });
    canvas.addEventListener('mouseleave', () => { hovered = false; });
    // 设置 cursor 样式
    canvas.style.cursor = 'pointer';

    // ---- 点击恢复宠物 ----
    canvas.addEventListener('mousedown', async (e) => {
      if (e.button !== 0) return;
      // 点击反馈：短暂变亮后执行恢复
      hovered = true;
      drawGlow(); // 立即重绘一次 hover 态
      await cmdUnsnapTransform();
    });

    // ---- 方向同步（通过 eval 直接调用，无需事件）----
    // Rust 侧通过 window.eval 调用此函数设置方向
    window.__setSnapEdge = function(edge) {
      edgeReversed = edge === 'right';
    };
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
    var root = document.getElementById('pet-root');
    if (!root) return;

    // 坐标分区：嘴巴区域 → 聊天，其余 → 拖拽
    root.addEventListener('mousedown', async function(e) {
      if (e.button !== 0) return;
      var win = getCurrentWin();
      if (!win) return;

      // 用 offsetX/offsetY（相对于事件目标元素），再映射到 canvas 坐标系
      // fallback 到 clientX - rect.left（兼容性兜底）
      var rawX = e.offsetX !== undefined ? e.offsetX : (e.clientX - canvas.getBoundingClientRect().left);
      var rawY = e.offsetY !== undefined ? e.offsetY : (e.clientY - canvas.getBoundingClientRect().top);

      // 关键修正：将 CSS 像素坐标映射到 canvas 逻辑像素坐标系
      // canvas.width 是逻辑像素(128)，但 DOM 尺寸受 DPI 缩放影响
      var rect = canvas.getBoundingClientRect();
      var cssW = rect.width;   // canvas 的 CSS 渲染宽度（可能 ≠ 128）
      var cssH = rect.height;
      var logicW = canvas.width; // 逻辑像素（始终 128 或 48）
      var scale = cssW / logicW; // DPI 缩放比（如 1.5 / 2.0）

      // 将 CSS 坐标 → 逻辑坐标（与精灵像素对齐）
      var cx = rawX / scale;
      var cy = rawY / scale;

      var diag = JSON.stringify({
        raw: { x: Math.round(rawX), y: Math.round(rawY), target: e.target.id || e.target.className },
        cssSize: { w: Math.round(cssW), h: Math.round(cssH) },
        logicSize: logicW,
        scale: scale.toFixed(2),
        logicCoord: { x: Math.round(cx), y: Math.round(cy) },
        hotzone: isMouthHotzone(cx, cy, logicW) ? 'MOUTH' : 'DRAG',
      });
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('cmd_pet_log', { msg: 'mousedown 坐标诊断 ' + diag }).catch(function() {});
      }

      if (isMouthHotzone(cx, cy, logicW)) {
        if (window.__TAURI__ && window.__TAURI__.core) {
          window.__TAURI__.core.invoke('cmd_pet_log', { msg: '✓ 嘴巴热区命中 → cmd_open_chat' }).catch(function() {});
        }
        try { await window.__TAURI__.core.invoke('cmd_open_chat'); }
        catch (err) {
          if (window.__TAURI__ && window.__TAURI__.core) {
            window.__TAURI__.core.invoke('cmd_pet_log', { msg: 'cmd_open_chat 失败: ' + err }).catch(function() {});
          }
        }
      } else {
        if (window.__TAURI__ && window.__TAURI__.core) {
          window.__TAURI__.core.invoke('cmd_pet_log', { msg: '→ 拖拽模式' }).catch(function() {});
        }
        try { await win.startDragging(); } catch (_) {}
        startSnapPoll(win);
      }
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

  // 暴露纯函数供测试访问
  window.PetApp = { isMouthHotzone: isMouthHotzone };
})();
