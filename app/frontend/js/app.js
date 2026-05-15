// app.js — Tauri 事件监听 + 主循环入口 + 右键菜单 + 折叠/展开 + 拖拽

import { PerformerHost } from './performance/performer-host.js';

(function() {
  'use strict';

  const SpriteRenderer = window.SpriteRenderer;
  const PetState = window.PetState;
  const Particles = window.Particles;

  // ---- 嘴巴热区判定（纯函数，可独立测试）----
  // 热区覆盖精灵嘴巴/腮红区域：正常态 x[32,96] y[56,96] (canvas 坐标)
  // 折叠态按 canvasSize 比例缩放
  function isMouthHotzone(x, y, canvasSize) {
    var ratio = canvasSize / 128;
    return x >= 32 * ratio && x <= 96 * ratio && y >= 56 * ratio && y <= 96 * ratio;
  }

  // 左眼热区：对应 16x16 精灵的左眼像素附近（col 3-4, row 6），
  // 适度放大一点，降低双击像素精灵时的手感门槛。
  function isLeftEyeHotzone(x, y, canvasSize) {
    var ratio = canvasSize / 128;
    if (ratio <= 0) return false;
    return x >= 22 * ratio && x <= 44 * ratio && y >= 44 * ratio && y <= 62 * ratio;
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
  let snapEdge = null;  // 'left' | 'right' | 'top' | 'bottom'

  // 当前窗口是否为吸附竖条窗口
  let isSnapWindow = false;

  // 表现播放器：统一接管固定舞蹈、音乐响应舞动和后续 AI 即兴表演。
  let performerHost = null;
  let performerMoveState = { at: 0, x: null, y: null };
  let screenshotFeedbackTimer = null;

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
    performerHost = new PerformerHost({
      getMetrics: async function() {
        var win = getCurrentWin();
        return win ? await getDanceScreenMetrics(win) : null;
      },
      applyOffset: applyPerformerOffset,
      resetPosition: resetPerformerPosition,
      renderSprite: function(action, opts, scale) {
        SpriteRenderer.renderSprite(ctx, action, 0, pet.facingRight, scale || 8, opts);
      },
      setFacingRight: function(facingRight) {
        pet.facingRight = facingRight;
      },
      setActiveClass: function(active) {
        bodyEl.classList.toggle('dancing', active);
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
        scale: scale,
      };
    }

    root.addEventListener('dblclick', async function(e) {
      if (e.button !== 0) return;
      var coord = eventToCanvasCoord(e);
      if (!isLeftEyeHotzone(coord.cx, coord.cy, coord.logicW)) return;

      e.preventDefault();
      e.stopPropagation();

      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('cmd_pet_log', { msg: '✓ 左眼双击命中 → cmd_screenshot_now' }).catch(function() {});
        try {
          await window.__TAURI__.core.invoke('cmd_screenshot_now');
          flashScreenshotFeedback();
        }
        catch (err) {
          window.__TAURI__.core.invoke('cmd_pet_log', { msg: 'cmd_screenshot_now 失败: ' + err }).catch(function() {});
        }
      }
    });

    // 坐标分区：嘴巴区域 → 聊天，其余 → 拖拽
    root.addEventListener('mousedown', async function(e) {
      if (e.button !== 0) return;
      var win = getCurrentWin();
      if (!win) return;

      var coord = eventToCanvasCoord(e);
      var cx = coord.cx;
      var cy = coord.cy;
      var logicW = coord.logicW;

      var diag = JSON.stringify({
        raw: { x: Math.round(coord.rawX), y: Math.round(coord.rawY), target: e.target.id || e.target.className },
        cssSize: { w: Math.round(coord.cssW), h: Math.round(coord.cssH) },
        logicSize: logicW,
        scale: coord.scale.toFixed(2),
        logicCoord: { x: Math.round(cx), y: Math.round(cy) },
        hotzone: isMouthHotzone(cx, cy, logicW) ? 'MOUTH' : (isLeftEyeHotzone(cx, cy, logicW) ? 'LEFT_EYE' : 'DRAG'),
      });
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('cmd_pet_log', { msg: 'mousedown 坐标诊断 ' + diag }).catch(function() {});
      }

      if (isLeftEyeHotzone(cx, cy, logicW)) {
        e.preventDefault();
        if (window.__TAURI__ && window.__TAURI__.core) {
          window.__TAURI__.core.invoke('cmd_pet_log', { msg: '左眼热区按下，等待 dblclick' }).catch(function() {});
        }
      } else if (isMouthHotzone(cx, cy, logicW)) {
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
      const payload = event.payload;
      if (payload && payload.type === 'play_dance' && payload.name) {
        try {
          window.__TAURI__.core.invoke('cmd_play_dance', { danceName: payload.name });
        } catch (err) {
          console.warn('[pet] play_dance 事件处理失败:', err);
        }
        return;
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
    isMouthHotzone: isMouthHotzone,
    isLeftEyeHotzone: isLeftEyeHotzone,
  };
})();
