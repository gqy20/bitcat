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

  // 舞蹈播放器（非 null 时劫持渲染循环）
  let dancePlayer = null;  // { steps, index, time, loop }

  // Tauri 2 正确的 API 路径：getCurrentWindow() 不是 getCurrent()
  function getCurrentWin() {
    try {
      return window.__TAURI__.window.getCurrentWindow();
    } catch (e) {
      console.error('[pet] getCurrentWindow 失败:', e);
      return null;
    }
  }

  // ========== Crossfade 工具（Task 4） ==========
  // 由 Rust 侧 cmd_snap_transform / cmd_unsnap_transform 通过 eval 调用，
  // 与 CSS body.fading-in/out/pre-fade 配合实现双窗口平滑切换。
  const FADE_MS = 150;
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
      // 下一个微任务/帧移除 pre-fade 触发 transition
      setTimeout(() => {
        document.body.classList.remove('pre-fade');
        document.body.classList.add('fading-in');
        setTimeout(resolve, FADE_MS);
      }, 0);
    });
  };
  window.__fadeReset = function() {
    document.body.classList.remove('fading-in', 'fading-out', 'pre-fade');
  };

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
      // Pull 模式：init 时先从 Rust 拉取吸附方向，替代脆弱的 eval 注入
      var initialEdge = null;
      try {
        if (window.__TAURI__ && window.__TAURI__.core) {
          var snap = await window.__TAURI__.core.invoke('cmd_get_window_state');
          if (snap && snap.snap_edge) initialEdge = snap.snap_edge;
          console.log('[pet-snap] pull 初始方向:', initialEdge);
        }
      } catch (err) {
        console.warn('[pet-snap] pull snap_edge 失败，默认 left:', err);
      }
      setupSnapBar(initialEdge);
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

  /// 吸附竖条模式：DOM + CSS 驱动（Task 3）
  /// - 不再使用 canvas + requestAnimationFrame，视觉效果全部交给 CSS animation/transition
  /// - class 切换：direction-left/right（方向）、hovered（展宽）
  /// - initialEdge 来自 Rust pull 模式；window.__setSnapEdge 作为运行时兜底
  function setupSnapBar(initialEdge) {
    const bar = document.getElementById('snap-bar');
    if (!bar) {
      console.error('[pet-snap] #snap-bar 节点缺失');
      return;
    }

    // 进入 snap 模式：body 标记用于隐藏主精灵/阴影/粒子
    document.body.classList.add('snap-mode');

    // 主 canvas 在 snap 窗口中不再绘制任何东西
    if (canvas) {
      canvas.width = 24;
      canvas.height = 100;
      canvas.style.display = 'none';
    }

    function setEdge(edge) {
      bar.classList.remove('direction-left', 'direction-right');
      bar.classList.add(edge === 'right' ? 'direction-right' : 'direction-left');
    }
    setEdge(initialEdge || 'left');
    bar.hidden = false;

    // hover 交互（CSS 负责视觉展宽/指示点）
    bar.addEventListener('mouseenter', () => { bar.classList.add('hovered'); });
    bar.addEventListener('mouseleave', () => { bar.classList.remove('hovered'); });

    // 点击恢复宠物
    bar.addEventListener('mousedown', async (e) => {
      if (e.button !== 0) return;
      bar.classList.add('hovered'); // 点击瞬间保持高亮反馈
      await cmdUnsnapTransform();
    });

    // 运行时方向切换兜底（lib.rs cmd_snap_transform 通过 eval 调用）
    window.__setSnapEdge = function(edge) { setEdge(edge); };
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
      if (dancePlayer) {
        // 舞蹈模式：按时间轴切换动作帧
        updateDance(dt);
      } else {
        // 正常模式：状态机驱动
        pet.update(dt);

        if (pet.state !== prevState) {
          syncStateClass(pet.state);
          flashSprite();
          Particles.onStateEnter(pet.state);
          prevState = pet.state;
        }

        SpriteRenderer.renderSprite(ctx, pet.state, pet.frame, pet.facingRight, 8);
        Particles.tick(pet.state, dt);
      }
    } else {
      SpriteRenderer.renderMini(ctx, pet.state);
    }

    requestAnimationFrame(loop);
  }

  function updateDance(dt) {
    dancePlayer.time += dt;
    var step = dancePlayer.steps[dancePlayer.index];
    if (dancePlayer.time >= step.duration_ms) {
      dancePlayer.time = 0;
      dancePlayer.index++;
      if (dancePlayer.index >= dancePlayer.steps.length) {
        if (dancePlayer.loop_) {
          dancePlayer.index = 0;
        } else {
          // 舞蹈结束，交还控制权给状态机
          dancePlayer = null;
          return;
        }
      }
    }
    var currentAction = dancePlayer.steps[dancePlayer.index].action;
    SpriteRenderer.renderSprite(ctx, currentAction, 0, pet.facingRight, 8);
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

    // 舞蹈播放事件（Rust 侧 cmd_play_dance 发出）
    window.__TAURI__.event.listen('play-dance', (event) => {
      console.log('[pet] 收到 play-dance:', event.payload);
      var payload = event.payload;
      dancePlayer = {
        steps: payload.steps,
        index: 0,
        time: 0,
        loop_: payload.loop_ !== false,
      };
    });
  }

  document.addEventListener('DOMContentLoaded', init);

  // 暴露纯函数供测试访问
  window.PetApp = { isMouthHotzone: isMouthHotzone };
})();
