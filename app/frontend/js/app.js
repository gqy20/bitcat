// app.js — Tauri 事件监听 + 主循环入口 + 右键菜单 + 折叠/展开 + 拖拽

import { PerformerHost } from './performance/performer-host.js';

(function() {
  'use strict';

  let SpriteRenderer = window.SpriteRenderer;
  const PetState = window.PetState;
  const Particles = window.Particles;

  const DEFAULT_PET_HOTSPOTS = {
    observe: { x: 0.18, y: 0.10, w: 0.64, h: 0.40 },
    input: { x: 0.22, y: 0.38, w: 0.56, h: 0.34 },
  };

  function normalizeHotspotRect(spec, width, height) {
    if (!spec || typeof spec !== 'object') return null;
    var x = Number(spec.x);
    var y = Number(spec.y);
    var w = Number(spec.w);
    var h = Number(spec.h);
    if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(w) || !Number.isFinite(h)) {
      return null;
    }
    var useRatio = Math.max(Math.abs(x), Math.abs(y), Math.abs(w), Math.abs(h)) <= 1.5;
    return useRatio
      ? {
          left: x * width,
          top: y * height,
          right: (x + w) * width,
          bottom: (y + h) * height,
        }
      : {
          left: x,
          top: y,
          right: x + w,
          bottom: y + h,
        };
  }

  function getPetHotspots() {
    return (SpriteRenderer && SpriteRenderer.hotspots) || DEFAULT_PET_HOTSPOTS;
  }

  function hitPetHotspot(name, x, y, width, height) {
    var rect = normalizeHotspotRect(getPetHotspots()[name], width, height);
    return !!rect && x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
  }

  function flashPetHotspot(kind) {
    if (!document || !document.body) return;
    var body = document.body;
    body.classList.remove('pet-hotspot-observe', 'pet-hotspot-input');
    if (!kind) return;
    body.classList.add('pet-hotspot-' + kind);
    clearTimeout(flashPetHotspot._timer);
    flashPetHotspot._timer = setTimeout(function() {
      body.classList.remove('pet-hotspot-observe', 'pet-hotspot-input');
    }, 160);
  }

  let pet = null;
  let lastTime = performance.now();
  let canvas, ctx;
  let prevState = null;
  let bodyEl;

  // 折叠状态（默认值，initPullState 会从 Rust 侧拉取真实值）
  let collapsed = false;
  let alwaysOnTop = true;

  // 吸附状态
  let isSnapped = false;
  let snapEdge = null;  // 'left' | 'right' | 'top' | 'bottom'

  // 当前窗口是否为吸附竖条窗口
  let isSnapWindow = false;

  // 表现播放器：统一接管固定舞蹈、音乐响应舞动和后续 AI 即兴表演。
  let performerHost = null;
  let performerMoveState = { at: 0, x: null, y: null };
  let screenshotFeedbackTimer = null;
  let hoverActivityTimer = null;
  let hoverActionCooldownUntil = 0;
  let normalPetWidth = 128;
  let normalPetHeight = 128;
  const PET_SIZE_STORAGE_KEY = 'bitcat.petSize';
  const PET_BADGE_REFRESH_MS = 2500;
  const PET_MIN_SIZE = 72;
  const PET_MAX_SIZE = 256;
  const petBadgeCounts = { agent: 0, screenshot: 0 };

  function playPetAction(name) {
    return !!(pet && pet.playAction && pet.playAction(name));
  }

  function playPetActionAny(names) {
    for (var i = 0; i < names.length; i++) {
      if (playPetAction(names[i])) return true;
    }
    return false;
  }

  function clampPetSize(width, height) {
    var baseW = Math.max(1, (SpriteRenderer && SpriteRenderer.displayWidth) || 128);
    var baseH = Math.max(1, (SpriteRenderer && SpriteRenderer.displayHeight) || 128);
    var scale = Math.max(width / baseW, height / baseH);
    var minScale = PET_MIN_SIZE / Math.max(baseW, baseH);
    var maxScale = PET_MAX_SIZE / Math.max(baseW, baseH);
    scale = Math.min(maxScale, Math.max(minScale, scale));
    return {
      w: Math.round(baseW * scale),
      h: Math.round(baseH * scale),
    };
  }

  function loadSavedPetSize(fallback) {
    try {
      var raw = window.localStorage && window.localStorage.getItem(PET_SIZE_STORAGE_KEY);
      if (!raw) return fallback;
      var saved = JSON.parse(raw);
      var w = Number(saved && saved.w);
      var h = Number(saved && saved.h);
      if (!Number.isFinite(w) || !Number.isFinite(h)) return fallback;
      return clampPetSize(w, h);
    } catch (_) {
      return fallback;
    }
  }

  function savePetSize(width, height) {
    try {
      window.localStorage && window.localStorage.setItem(
        PET_SIZE_STORAGE_KEY,
        JSON.stringify({ w: Math.round(width), h: Math.round(height) })
      );
    } catch (_) {}
  }

  function viewportRenderScale() {
    if (!SpriteRenderer || !canvas || !SpriteRenderer.frameWidth || !SpriteRenderer.frameHeight) {
      return SpriteRenderer && SpriteRenderer.renderScale;
    }
    return Math.min(canvas.width / SpriteRenderer.frameWidth, canvas.height / SpriteRenderer.frameHeight);
  }

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
    if (window.SpriteRendererReady) {
      SpriteRenderer = await window.SpriteRendererReady;
    } else {
      SpriteRenderer = window.SpriteRenderer;
    }
    pet = new PetState.PetStateMachine({
      stateConfig: SpriteRenderer && SpriteRenderer.stateConfig,
      actionConfig: SpriteRenderer && SpriteRenderer.actionConfig,
    });

    canvas = document.getElementById('sprite');
    ctx = canvas.getContext('2d');
    bodyEl = document.body;
    var normalSize = resolveNormalPetSize();
    normalSize = loadSavedPetSize(normalSize);
    normalPetWidth = normalSize.w;
    normalPetHeight = normalSize.h;
    performerHost = new PerformerHost({
      getMetrics: async function() {
        var win = getCurrentWin();
        return win ? await getDanceScreenMetrics(win) : null;
      },
      applyOffset: applyPerformerOffset,
      resetPosition: resetPerformerPosition,
      renderSprite: function(action, opts, scale) {
        SpriteRenderer.renderSprite(ctx, action, 0, pet.facingRight, scale || viewportRenderScale(), opts);
      },
      setFacingRight: function(facingRight) {
        pet.facingRight = facingRight;
      },
      setActiveClass: function(active) {
        bodyEl.classList.toggle('dancing', active);
      },
      restoreSemanticState: function() {
        pet.applySemanticState();
        syncStateClass(pet.state);
        SpriteRenderer.renderSprite(ctx, pet.visualState(), pet.frame, pet.facingRight, viewportRenderScale());
        prevState = pet.state;
      },
      log: function(msg) {
        console.log(msg);
      },
    });

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
      var snapMetrics = null;
      try {
        if (window.__TAURI__ && window.__TAURI__.core) {
          var snap = await window.__TAURI__.core.invoke('cmd_get_window_state');
          if (snap && snap.snap_edge) initialEdge = snap.snap_edge;
          if (snap && snap.snap_w && snap.snap_h) snapMetrics = { w: snap.snap_w, h: snap.snap_h };
          console.log('[pet-snap] pull 初始方向:', initialEdge);
        }
      } catch (err) {
        console.warn('[pet-snap] pull snap_edge 失败，默认 left:', err);
      }
      setupSnapBar(initialEdge, snapMetrics);
      return;
    }

    // Pull 模式：从 Rust 侧拉取当前窗口状态（替代不可靠的 emit push）
    await initPullState();

    syncStateClass(pet.state);
    requestAnimationFrame(loop);
    setupTauriEvents();
    setupContextMenu();
    setupDrag();
    setupResizeHandle();
    setupPetBadge();
  }

  function resolveNormalPetSize() {
    return {
      w: Math.max(1, Math.round((SpriteRenderer && SpriteRenderer.displayWidth) || 128)),
      h: Math.max(1, Math.round((SpriteRenderer && SpriteRenderer.displayHeight) || 128)),
    };
  }

  async function applyViewportSize(width, height, resizeWindow) {
    if (!canvas) return;
    width = Math.max(1, Math.round(width));
    height = Math.max(1, Math.round(height));
    canvas.width = width;
    canvas.height = height;
    canvas.style.width = width + 'px';
    canvas.style.height = height + 'px';
    canvas.style.imageRendering = SpriteRenderer && SpriteRenderer.pixelated === false ? 'auto' : 'pixelated';
    document.documentElement.style.width = width + 'px';
    document.documentElement.style.height = height + 'px';
    document.body.style.width = width + 'px';
    document.body.style.height = height + 'px';
    document.body.style.setProperty('--pet-width', width + 'px');
    document.body.style.setProperty('--pet-height', height + 'px');

    if (!resizeWindow) return;
    var win = getCurrentWin();
    var api = window.__TAURI__ && window.__TAURI__.window;
    if (!win || !api || !api.LogicalSize) return;
    try {
      await win.setSize(new api.LogicalSize(width, height));
    } catch (err) {
      console.warn('[pet] resize window failed:', err);
    }
  }

  /// 吸附竖条模式：DOM + CSS 驱动（Task 3）
  /// - 不再使用 canvas + requestAnimationFrame，视觉效果全部交给 CSS animation/transition
  /// - class 切换：direction-left/right（方向）、hovered（展宽）
  /// - initialEdge 来自 Rust pull 模式；window.__setSnapEdge 作为运行时兜底
  function setupSnapBar(initialEdge, snapMetrics) {
    const bar = document.getElementById('snap-bar');
    if (!bar) {
      console.error('[pet-snap] #snap-bar 节点缺失');
      return;
    }

    // 进入 snap 模式：body 标记用于隐藏主精灵/阴影/粒子
    function applySnapMetrics(metrics) {
      var w = metrics && Number(metrics.w);
      var h = metrics && Number(metrics.h);
      if (!Number.isFinite(w) || w <= 0) w = 24;
      if (!Number.isFinite(h) || h <= 0) h = 67;
      document.documentElement.style.setProperty('--snap-w', w + 'px');
      document.documentElement.style.setProperty('--snap-h', h + 'px');
      document.body.style.setProperty('--snap-w', w + 'px');
      document.body.style.setProperty('--snap-h', h + 'px');
      if (canvas) {
        canvas.width = Math.round(w);
        canvas.height = Math.round(h);
      }
    }

    applySnapMetrics(snapMetrics);

    // 主 canvas 在 snap 窗口中不再绘制任何东西
    if (canvas) {
      canvas.style.display = 'none';
    }

    function setEdge(edge) {
      var normalized = ['left', 'right', 'top', 'bottom'].includes(edge) ? edge : null;
      bar.classList.remove('direction-left', 'direction-right', 'direction-top', 'direction-bottom');
      document.body.classList.remove('snap-left', 'snap-right', 'snap-top', 'snap-bottom');
      if (!normalized) {
        bar.hidden = true;
        document.body.classList.remove('snap-mode');
        return false;
      }
      bar.classList.add('direction-' + normalized);
      document.body.classList.add('snap-' + normalized);
      document.body.classList.add('snap-mode');
      bar.hidden = false;
      return true;
    }
    setEdge(initialEdge);

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
    window.__setSnapMetrics = function(w, h) { applySnapMetrics({ w: w, h: h }); };
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
      const w = collapsed ? 48 : normalPetWidth;
      const h = collapsed ? 48 : normalPetHeight;
      await applyViewportSize(w, h, true);
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
    var DRAG_THRESHOLD_PX = 6;
    var OBSERVE_DBLCLICK_GRACE_MS = 240;
    var activeGesture = null;
    var observeClickTimer = null;

    function logPet(msg) {
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('cmd_pet_log', { msg: msg }).catch(function() {});
      }
    }

    function clearObserveClickTimer() {
      if (!observeClickTimer) return;
      clearTimeout(observeClickTimer);
      observeClickTimer = null;
    }

    async function openChatFromPet(source) {
      clearObserveClickTimer();
      flashPetHotspot('input');
      playPetAction('acknowledge');
      logPet('pet click -> cmd_open_chat (' + source + ')');
      if (!window.__TAURI__ || !window.__TAURI__.core) return;
      try {
        await window.__TAURI__.core.invoke('cmd_open_chat');
      }
      catch (err) {
        playPetAction('blocked');
        logPet('cmd_open_chat failed: ' + err);
      }
    }

    async function triggerObserveNow() {
      clearObserveClickTimer();
      flashPetHotspot('observe');
      playPetAction('observe');
      logPet('observe dblclick -> cmd_screenshot_now');
      if (!window.__TAURI__ || !window.__TAURI__.core) return;
      try {
        await clearHiddenScreenshotCount();
        await window.__TAURI__.core.invoke('cmd_screenshot_now');
      }
      catch (err) {
        playPetAction('blocked');
        logPet('cmd_screenshot_now failed: ' + err);
      }
    }

    function eventToCanvasCoord(e) {
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

      return {
        rawX: rawX,
        rawY: rawY,
        cx: cx,
        cy: cy,
        cssW: cssW,
        cssH: cssH,
        logicW: logicW,
        logicH: canvas.height || logicW,
        scale: scale,
      };
    }

    root.addEventListener('dblclick', async function(e) {
      if (e.button !== 0) return;
      var coord = eventToCanvasCoord(e);
      if (!hitPetHotspot('observe', coord.cx, coord.cy, coord.logicW, coord.logicH)) return;

      e.preventDefault();
      e.stopPropagation();
      await triggerObserveNow();
    });

    root.addEventListener('pointerenter', function(e) {
      if (e.pointerType === 'touch') return;
      triggerHoverActivity();
    });
    root.addEventListener('pointermove', function(e) {
      if (e.pointerType === 'touch' || activeGesture) return;
      triggerHoverActivity();
    });
    root.addEventListener('pointerleave', function() {
      clearHoverActivity();
    });

    // 统一手势：单击打开对话，观察区双击截图，移动超过阈值则拖拽
    root.addEventListener('pointerdown', function(e) {
      if (e.button !== 0) return;
      var win = getCurrentWin();
      if (!win) return;
      clearHoverActivity();

      var coord = eventToCanvasCoord(e);
      var cx = coord.cx;
      var cy = coord.cy;
      var logicW = coord.logicW;
      var logicH = coord.logicH;
      var observeHit = hitPetHotspot('observe', cx, cy, logicW, logicH);
      var inputHit = hitPetHotspot('input', cx, cy, logicW, logicH);

      var diag = JSON.stringify({
        raw: { x: Math.round(coord.rawX), y: Math.round(coord.rawY), target: e.target.id || e.target.className },
        cssSize: { w: Math.round(coord.cssW), h: Math.round(coord.cssH) },
        logicSize: { w: logicW, h: logicH },
        scale: coord.scale.toFixed(2),
        logicCoord: { x: Math.round(cx), y: Math.round(cy) },
        hotzone: observeHit ? 'OBSERVE' : (inputHit ? 'INPUT' : 'BODY'),
      });
      logPet('pointerdown gesture ' + diag);

      activeGesture = {
        id: e.pointerId,
        startX: e.clientX,
        startY: e.clientY,
        win: win,
        observeHit: observeHit,
        inputHit: inputHit,
        dragging: false,
      };
      try { root.setPointerCapture(e.pointerId); } catch (_) {}
      e.preventDefault();
    });

    root.addEventListener('pointermove', async function(e) {
      if (!activeGesture || e.pointerId !== activeGesture.id || activeGesture.dragging) return;
      var dx = e.clientX - activeGesture.startX;
      var dy = e.clientY - activeGesture.startY;
      if (Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;

      clearObserveClickTimer();
      activeGesture.dragging = true;
      var gesture = activeGesture;
      activeGesture = null;
      try { root.releasePointerCapture(e.pointerId); } catch (_) {}
      e.preventDefault();

      if (gesture.win) {
        playPetAction('dragging');
        logPet('pet drag mode');
        try { await gesture.win.startDragging(); } catch (_) {}
        startSnapPoll(gesture.win);
      }
    });

    root.addEventListener('pointerup', function(e) {
      if (!activeGesture || e.pointerId !== activeGesture.id) return;
      var gesture = activeGesture;
      activeGesture = null;
      try { root.releasePointerCapture(e.pointerId); } catch (_) {}
      e.preventDefault();

      if (gesture.observeHit) {
        clearObserveClickTimer();
        observeClickTimer = setTimeout(function() {
          observeClickTimer = null;
          openChatFromPet('observe-single-click');
        }, OBSERVE_DBLCLICK_GRACE_MS);
      } else {
        openChatFromPet(gesture.inputHit ? 'input-hotspot' : 'body');
      }
    });

    ['pointercancel', 'pointerleave'].forEach(function(type) {
      root.addEventListener(type, function(e) {
        if (!activeGesture || e.pointerId !== activeGesture.id) return;
        activeGesture = null;
        try { root.releasePointerCapture(e.pointerId); } catch (_) {}
      });
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
            } else {
              await cmdSavePetPosition(pos.x, pos.y);
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
  async function cmdSavePetPosition(x, y) {
    try {
      await window.__TAURI__.core.invoke('cmd_save_pet_position', { x, y });
    } catch (e) { console.error('[pet] cmd_save_pet_position failed:', e); }
  }

  async function cmdUnsnapTransform() {
    try {
      await window.__TAURI__.core.invoke('cmd_unsnap_transform');
    } catch (e) { console.error('[pet] cmd_unsnap_transform 失败:', e); }
  }

  function setupResizeHandle() {
    var handle = document.getElementById('resize-handle');
    if (!handle || isSnapWindow) return;
    var gesture = null;
    var rafPending = false;
    var nextSize = null;

    function applyQueuedSize() {
      rafPending = false;
      if (!nextSize) return;
      normalPetWidth = nextSize.w;
      normalPetHeight = nextSize.h;
      applyViewportSize(nextSize.w, nextSize.h, true);
      nextSize = null;
    }

    handle.addEventListener('pointerdown', function(e) {
      if (e.button !== 0 || collapsed) return;
      e.preventDefault();
      e.stopPropagation();
      clearHoverActivity();
      gesture = {
        id: e.pointerId,
        startX: e.clientX,
        startY: e.clientY,
        startW: normalPetWidth,
        startH: normalPetHeight,
      };
      document.body.classList.add('pet-resizing');
      try { handle.setPointerCapture(e.pointerId); } catch (_) {}
    });

    handle.addEventListener('pointermove', function(e) {
      if (!gesture || e.pointerId !== gesture.id) return;
      e.preventDefault();
      e.stopPropagation();
      var baseW = Math.max(1, (SpriteRenderer && SpriteRenderer.displayWidth) || 128);
      var baseH = Math.max(1, (SpriteRenderer && SpriteRenderer.displayHeight) || 128);
      var nextW = gesture.startW + (e.clientX - gesture.startX);
      var nextH = gesture.startH + (e.clientY - gesture.startY);
      var scale = Math.max(nextW / baseW, nextH / baseH);
      var size = clampPetSize(baseW * scale, baseH * scale);
      nextSize = size;
      if (!rafPending) {
        rafPending = true;
        requestAnimationFrame(applyQueuedSize);
      }
    });

    function finish(e) {
      if (!gesture || e.pointerId !== gesture.id) return;
      try { handle.releasePointerCapture(e.pointerId); } catch (_) {}
      gesture = null;
      document.body.classList.remove('pet-resizing');
      if (nextSize) applyQueuedSize();
      savePetSize(normalPetWidth, normalPetHeight);
      var win = getCurrentWin();
      if (win) {
        win.outerPosition()
          .then(function(pos) { return cmdSavePetPosition(pos.x, pos.y); })
          .catch(function() {});
      }
    }

    handle.addEventListener('pointerup', finish);
    handle.addEventListener('pointercancel', finish);
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

  /// 弹性缓动：跳跃落地时的弹跳感
  function easeOutBounce(t) {
    var n1 = 7.5625, d1 = 2.75;
    if (t < 1 / d1) return n1 * t * t;
    if (t < 2 / d1) { t -= 1.5 / d1; return n1 * t * t + 0.75; }
    if (t < 2.5 / d1) { t -= 2.25 / d1; return n1 * t * t + 0.9375; }
    t -= 2.625 / d1; return n1 * t * t + 0.984375;
  }

  /// 获取当前显示器物理尺寸 + 窗口位置（用于舞蹈幅度计算）
  /// 多屏安全：通过 availableMonitors 匹配窗口所在屏幕，避免 fallback 到主屏原点
  async function getDanceScreenMetrics(win) {
    var pos;
    try { pos = await win.outerPosition(); } catch (e) {
      console.error('[dance] outerPosition 失败:', e);
      return null;
    }

    // 策略 1：currentMonitor（最直接）
    try {
      var monitor = await win.currentMonitor();
      if (monitor && monitor.size && monitor.size.width > 0) {
        var m = {
          baseX: pos.x, baseY: pos.y,
          screenW: monitor.size.width, screenH: monitor.size.height,
        };
        console.log('[dance] metrics(currentMonitor):', JSON.stringify(m));
        return m;
      }
    } catch (e) {
      console.warn('[dance] currentMonitor 失败，尝试 availableMonitors:', e.message || e);
    }

    // 策略 2：遍历所有显示器，找窗口所在的那个（多屏兼容）
    try {
      var monitors = await win.availableMonitors();
      for (var i = 0; i < monitors.length; i++) {
        var mon = monitors[i];
        // 检查窗口位置是否在该显示器的范围内（用 position + size 判定矩形包含）
        var mx = mon.position ? mon.position.x : (mon.positionX || 0);
        var my = mon.position ? mon.position.y : (mon.positionY || 0);
        var mw = mon.size.width;
        var mh = mon.size.height;
        if (pos.x >= mx && pos.x < mx + mw && pos.y >= my && pos.y < my + mh) {
          var m2 = {
            baseX: pos.x, baseY: pos.y,
            screenW: mw, screenH: mh,
          };
          console.log('[dance] metrics(availableMonitors[' + i + ']):', JSON.stringify(m2),
            'monitor@(' + mx + ',' + my + ') ' + mw + 'x' + mh);
          return m2;
        }
      }
    } catch (e) {
      console.warn('[dance] availableMonitors 也失败:', e.message || e);
    }

    // 策略 3：至少保留真实窗口位置，只用 monitorSize 兜底尺寸
    try {
      var sz = await win.monitorSize();
      var m3 = {
        baseX: pos.x, baseY: pos.y,
        screenW: sz.width, screenH: sz.height,
      };
      console.log('[dance] metrics(monitorSize fallback):', JSON.stringify(m3));
      return m3;
    } catch (e) {
      console.error('[dance] 所有策略都失败:', e.message || e);
      // 最后兜底：保留真实位置，不用 (0,0)
      return { baseX: pos.x, baseY: pos.y, screenW: 1920, screenH: 1080 };
    }
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
      if (performerHost && performerHost.hasActive()) {
        performerHost.update(dt);
      } else {
        // 正常模式：状态机驱动
        pet.update(dt);
        var visualState = pet.visualState();

        if (pet.state !== prevState) {
          syncStateClass(pet.state);
          flashSprite();
          Particles.onStateEnter(pet.state);
          prevState = pet.state;
        }

        SpriteRenderer.renderSprite(ctx, visualState, pet.frame, pet.facingRight, viewportRenderScale());
        Particles.tick(pet.state, dt);
      }
    } else {
      SpriteRenderer.renderMini(ctx, pet.state);
    }

    requestAnimationFrame(loop);
  }

  function applyPerformerOffset(player, offset) {
    var m = player.metrics;
    if (!m) return;

    var win = getCurrentWin();
    if (!win) return;

    var Pos = window.__TAURI__.window.PhysicalPosition;
    var ox = offset && Number.isFinite(offset.x) ? offset.x : 0;
    var oy = offset && Number.isFinite(offset.y) ? offset.y : 0;
    var nextX = Math.round(m.baseX + ox);
    var nextY = Math.round(m.baseY + oy);
    var now = performance.now();
    var minInterval = player && player.kind === 'music-reactive' ? 66 : 33;
    if (
      performerMoveState.x != null &&
      now - performerMoveState.at < minInterval &&
      Math.abs(nextX - performerMoveState.x) < 4 &&
      Math.abs(nextY - performerMoveState.y) < 4
    ) {
      return;
    }
    performerMoveState = { at: now, x: nextX, y: nextY };
    try {
      win.setPosition(new Pos(nextX, nextY));
    } catch (_) {}
  }

  async function notifyPerformanceFinished(sessionId, reason) {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    try {
      await window.__TAURI__.core.invoke('cmd_performance_finished', {
        sessionId: sessionId,
        reason: reason || 'finished'
      });
    } catch (err) {
      console.warn('[performance] notify finished failed:', err);
    }
  }

  async function resetPerformerPosition(player, reason) {
    if (!player) return;

    var m = player.metrics;
    var sessionId = player.sessionId;

    if (!m) {
      notifyPerformanceFinished(sessionId, reason);
      return;
    }

    var win = getCurrentWin();
    if (!win) {
      notifyPerformanceFinished(sessionId, reason);
      return;
    }

    try { var curPos = await win.outerPosition(); } catch (_) {
      notifyPerformanceFinished(sessionId, reason);
      return;
    }
    performerMoveState = { at: 0, x: null, y: null };

    var Pos = window.__TAURI__.window.PhysicalPosition;
    var duration = 250;
    var start = performance.now();
    var fromX = curPos.x, fromY = curPos.y;
    var toX = m.baseX, toY = m.baseY;

    function frame(now) {
      var t = Math.min((now - start) / duration, 1);
      var e = easeOutCubic(t);
      win.setPosition(new Pos(
        Math.round(fromX + (toX - fromX) * e),
        Math.round(fromY + (toY - fromY) * e)
      ));
      if (t < 1) {
        requestAnimationFrame(frame);
      } else {
        notifyPerformanceFinished(sessionId, reason);
      }
    }
    requestAnimationFrame(frame);
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

  function flashScreenshotFeedback() {
    flashSprite();
    playPetAction('observe');
    if (screenshotFeedbackTimer) {
      clearTimeout(screenshotFeedbackTimer);
      screenshotFeedbackTimer = null;
    }
    bodyEl.classList.remove('screenshot-capturing');
    void bodyEl.offsetWidth;
    bodyEl.classList.add('screenshot-capturing');
    screenshotFeedbackTimer = setTimeout(function() {
      bodyEl.classList.remove('screenshot-capturing');
      screenshotFeedbackTimer = null;
    }, 1400);
  }

  function triggerHoverActivity() {
    if (!bodyEl || isSnapWindow) return;
    bodyEl.classList.add('pet-hover-active');
    clearTimeout(hoverActivityTimer);
    hoverActivityTimer = setTimeout(function() {
      bodyEl.classList.remove('pet-hover-active');
      hoverActivityTimer = null;
    }, 900);

    var now = performance.now();
    if (now < hoverActionCooldownUntil) return;
    hoverActionCooldownUntil = now + 1800;
    playPetActionAny(['nudge', 'acknowledge', 'happy']);
  }

  function clearHoverActivity() {
    if (hoverActivityTimer) {
      clearTimeout(hoverActivityTimer);
      hoverActivityTimer = null;
    }
    if (bodyEl) bodyEl.classList.remove('pet-hover-active');
  }

  function attentionSessionsFromAgentSnapshot(snapshot) {
    var sessions = (snapshot && snapshot.sessions) || [];
    return sessions.filter(function(session) {
      if (!session || (session.display && session.display.quiet)) return false;
      var status = String(session.status || '').toLowerCase();
      var tone = String((session.display && session.display.tone) || '').toLowerCase();
      return !!session.needs_user || tone === 'needs_user' || status === 'waiting' || status === 'error';
    });
  }

  function attentionCountFromAgentSnapshot(snapshot) {
    return attentionSessionsFromAgentSnapshot(snapshot).length;
  }

  function renderPetBadgeCount() {
    var badge = document.getElementById('pet-badge');
    if (!badge || !bodyEl) return;
    var count = petBadgeCounts.agent + petBadgeCounts.screenshot;
    var value = Math.max(0, Number(count) || 0);
    badge.textContent = value > 99 ? '99+' : String(value);
    badge.title = petBadgeCounts.screenshot > 0
      ? '隐藏的截图分析：' + petBadgeCounts.screenshot
      : '';
    badge.hidden = value <= 0;
    badge.classList.toggle('has-agent-alert', petBadgeCounts.agent > 0);
    badge.setAttribute('aria-label', '待查看 ' + value + ' 项');
    bodyEl.classList.toggle('has-pet-badge', value > 0);
  }

  function setPetBadgeCount(count) {
    petBadgeCounts.agent = Math.max(0, Number(count) || 0);
    renderPetBadgeCount();
  }

  function setHiddenScreenshotCount(count) {
    petBadgeCounts.screenshot = Math.max(0, Number(count) || 0);
    renderPetBadgeCount();
  }

  async function refreshHiddenScreenshotCount() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    try {
      var count = await window.__TAURI__.core.invoke('cmd_get_hidden_screenshot_count');
      setHiddenScreenshotCount(count);
    } catch (_) {}
  }

  async function clearHiddenScreenshotCount() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    try {
      var count = await window.__TAURI__.core.invoke('cmd_clear_hidden_screenshot_count');
      setHiddenScreenshotCount(count);
    } catch (_) {}
  }

  async function refreshPetBadge() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    try {
      var snapshot = await window.__TAURI__.core.invoke('cmd_get_agent_sessions');
      setPetBadgeCount(attentionCountFromAgentSnapshot(snapshot));
    } catch (_) {}
  }

  async function openPetInbox() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    try {
      await window.__TAURI__.core.invoke('cmd_show_pet_inbox');
    } catch (_) {}
  }

  function setupPetBadgeButton() {
    var badge = document.getElementById('pet-badge');
    if (badge) {
      badge.addEventListener('pointerdown', function(e) {
        e.stopPropagation();
      });
      badge.addEventListener('click', function(e) {
        e.preventDefault();
        e.stopPropagation();
        openPetInbox();
      });
    }
  }

  function setupPetBadge() {
    setupPetBadgeButton();
    renderPetBadgeCount();
    refreshPetBadge();
    refreshHiddenScreenshotCount();
    setInterval(refreshPetBadge, PET_BADGE_REFRESH_MS);
    setInterval(refreshHiddenScreenshotCount, PET_BADGE_REFRESH_MS);
  }

  // ========== 右键菜单 ==========

  function contextMenuCanvasPoint(e) {
    var rect = canvas.getBoundingClientRect();
    if (!rect.width || !rect.height) return null;
    return {
      x: Math.round((e.clientX - rect.left) * canvas.width / rect.width),
      y: Math.round((e.clientY - rect.top) * canvas.height / rect.height),
    };
  }

  function isRenderedPetPixel(e) {
    var point = contextMenuCanvasPoint(e);
    if (!point) return true;
    var radius = 3;
    var left = Math.max(0, point.x - radius);
    var top = Math.max(0, point.y - radius);
    var width = Math.min(canvas.width - left, radius * 2 + 1);
    var height = Math.min(canvas.height - top, radius * 2 + 1);
    try {
      var data = ctx.getImageData(left, top, width, height).data;
      for (var i = 3; i < data.length; i += 4) {
        if (data[i] > 16) return true;
      }
      return false;
    } catch (_) {
      return true;
    }
  }

  function setupContextMenu() {
    canvas.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      if (!isRenderedPetPixel(e)) return;
      if (!window.__TAURI__ || !window.__TAURI__.core) return;
      window.__TAURI__.core.invoke('cmd_show_pet_context_menu', {
        x: e.clientX,
        y: e.clientY
      }).catch(function(err) {
        console.warn('[pet] 打开右键菜单失败:', err);
      });
    });
  }

  // ========== 折叠/展开（由 Rust 托盘事件驱动 + init pull）==========

  async function applyCollapse() {
    console.log('[pet] applyCollapse:', { collapsed, state: pet.state });

    const w = collapsed ? 48 : normalPetWidth;
    const h = collapsed ? 48 : normalPetHeight;
    await applyViewportSize(w, h, false);
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
      const payload = event.payload;
      if (payload && payload.type === 'play_dance' && payload.name) {
        try {
          window.__TAURI__.core.invoke('cmd_play_dance', { danceName: payload.name });
        } catch (err) {
          console.warn('[pet] play_dance 事件处理失败:', err);
        }
        return;
      }
      if (performerHost) {
        performerHost.handlePetEvent(payload);
      }
      if (payload && payload.type === 'notify' && payload.kind === 'screenshot_observing') {
        flashScreenshotFeedback();
      }
      pet.applyEvent(payload);
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

    window.__TAURI__.event.listen('pet-asset-config-changed', (event) => {
      const url = event.payload || '';
      try {
        if (url) window.sessionStorage.setItem('bitcat.petAssetUrl', url);
        else window.sessionStorage.removeItem('bitcat.petAssetUrl');
      } catch (_) {}
      window.location.reload();
    });

    window.__TAURI__.event.listen('screenshot-hidden-count-changed', (event) => {
      setHiddenScreenshotCount(event.payload);
    });

    window.__TAURI__.event.listen('performance-start', async (event) => {
      if (!performerHost) return;
      await performerHost.start(event.payload || {});
    });

    window.__TAURI__.event.listen('performance-frame', (event) => {
      if (!performerHost) return;
      performerHost.frame(event.payload || {});
    });

    window.__TAURI__.event.listen('performance-stop', (event) => {
      if (!performerHost) return;
      performerHost.stop(event.payload || {});
    });

    window.__TAURI__.event.listen('performance-error', (event) => {
      console.warn('[performance] error:', event.payload);
      if (!performerHost) return;
      performerHost.stop({
        session_id: event.payload && event.payload.session_id,
        reason: 'error',
      });
    });
  }

  document.addEventListener('DOMContentLoaded', init);

  // 暴露纯函数供测试访问
  window.PetApp = {
    hitPetHotspot: hitPetHotspot,
    getPetHotspots: getPetHotspots,
    normalizeHotspotRect: normalizeHotspotRect,
    flashPetHotspot: flashPetHotspot,
  };})();
