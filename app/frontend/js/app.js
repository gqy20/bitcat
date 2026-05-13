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

  // 舞蹈播放器（非 null 时劫持渲染循环）
  let dancePlayer = null;  // { steps, index, repeatIndex, time, loop }

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
      var normalized = ['left', 'right', 'top', 'bottom'].includes(edge) ? edge : 'left';
      bar.classList.remove('direction-left', 'direction-right', 'direction-top', 'direction-bottom');
      bar.classList.add('direction-' + normalized);
      document.body.classList.remove('snap-left', 'snap-right', 'snap-top', 'snap-bottom');
      document.body.classList.add('snap-' + normalized);
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
        try { await window.__TAURI__.core.invoke('cmd_screenshot_now'); }
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

  function stepRepeat(step) {
    var repeat = Number(step && step.repeat);
    if (!Number.isFinite(repeat) || repeat < 1) return 1;
    return Math.max(1, Math.floor(repeat));
  }

  function advanceDanceStep() {
    var step = dancePlayer.steps[dancePlayer.index];
    var repeat = stepRepeat(step);

    if (dancePlayer.repeatIndex + 1 < repeat) {
      dancePlayer.repeatIndex++;
      console.log('[dance] 重复步骤', dancePlayer.index, 'repeat', dancePlayer.repeatIndex + 1, '/', repeat);
      return true;
    }

    dancePlayer.repeatIndex = 0;
    dancePlayer.index++;
    console.log('[dance] 切换到步骤', dancePlayer.index, '/', dancePlayer.steps.length);

    if (dancePlayer.index < dancePlayer.steps.length) {
      return true;
    }

    if (dancePlayer.loop_) {
      dancePlayer.index = 0;
      console.log('[dance] 循环，从头开始');
      return true;
    }

    console.log('[dance] 舞蹈播放完毕');
    resetDancePosition('finished');
    return false;
  }

  function updateDance(dt) {
    dancePlayer.time += dt;
    dancePlayer.elapsed += dt;

    // 硬上限：总累计时长超过 max_duration_ms 则停止，即便 loop_ 为 true
    if (dancePlayer.maxDurationMs != null && dancePlayer.elapsed >= dancePlayer.maxDurationMs) {
      console.log('[dance] 达到 max_duration_ms=' + dancePlayer.maxDurationMs + '，停止');
      resetDancePosition('max_duration');
      return;
    }

    var step = dancePlayer.steps[dancePlayer.index];
    while (step && dancePlayer.time >= step.duration_ms) {
      dancePlayer.time -= step.duration_ms;
      if (!advanceDanceStep()) return;
      step = dancePlayer.steps[dancePlayer.index];
    }

    var currentAction = step.action;
    var progress = dancePlayer.time / step.duration_ms;  // 0..1 当前步骤内进度
    var opts = {};

    // 窗口级大幅度动画（基于屏幕百分比）
    applyDanceWindowMove(currentAction, progress, dancePlayer.time);

    // 精灵内小幅补充动画（叠加在窗口移动之上）
    switch (currentAction) {
      case 'jump':
        var jumpH = -Math.sin(progress * Math.PI) * 18;
        opts.offsetY = jumpH;
        break;
      case 'spin':
        var flipCount = Math.floor(dancePlayer.time / 60);
        pet.facingRight = flipCount % 2 === 0;
        break;
      case 'wave':
        opts.offsetY = -Math.abs(Math.sin(progress * Math.PI * 4)) * 9;
        break;
      case 'shake':
        opts.offsetX = Math.sin(dancePlayer.time * 0.06) * 10;
        break;
    }

    SpriteRenderer.renderSprite(ctx, currentAction, 0, pet.facingRight, 8, opts);
  }

  /// 基于屏幕百分比计算窗口偏移并移动窗口
  function applyDanceWindowMove(action, progress, time) {
    var m = dancePlayer.metrics;
    if (!m) return;

    var win = getCurrentWin();
    if (!win) return;

    var Pos = window.__TAURI__.window.PhysicalPosition;
    var ox = 0, oy = 0;

    switch (action) {
      case 'jump': {
        // 大跳跃：窗口沿弧线上移屏幕高度约 22%，并带一点横向冲刺
        var jumpRange = m.screenH * 0.22;
        oy = -Math.sin(progress * Math.PI) * jumpRange;
        // 跳跃时横向位移增加舞台感
        ox = Math.sin(progress * Math.PI) * (m.screenW * 0.08);
        break;
      }
      case 'spin': {
        // 旋转时窗口做明显椭圆摆动（模拟旋转离心力）
        ox = Math.sin(time * 0.03) * (m.screenW * 0.12);
        oy = Math.cos(time * 0.025) * (m.screenH * 0.04);
        break;
      }
      case 'wave': {
        // 挥手节奏：上下浮动屏幕高度约 8%
        oy = -Math.abs(Math.sin(progress * Math.PI * 4)) * (m.screenH * 0.08);
        break;
      }
      case 'shake': {
        // 大幅左右抖动：单侧约 20% 屏宽，左右总摆幅约 40%
        ox = Math.sin(time * 0.05) * (m.screenW * 0.20);
        // Y 轴抖动增加不稳定感
        oy = Math.sin(time * 0.07) * (m.screenH * 0.025);
        break;
      }
    }

    try {
      win.setPosition(new Pos(
        Math.round(m.baseX + ox),
        Math.round(m.baseY + oy)
      ));
    } catch (_) {}
  }

  /// 舞舞结束：平滑归位到基准位置
  async function notifyDanceFinished(reason) {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    try {
      await window.__TAURI__.core.invoke('cmd_dance_finished', {
        reason: reason || 'finished'
      });
    } catch (err) {
      console.warn('[dance] 通知后端舞蹈结束失败:', err);
    }
  }

  async function resetDancePosition(reason) {
    if (!dancePlayer) return;

    var m = dancePlayer.metrics;
    dancePlayer = null;
    bodyEl.classList.remove('dancing');

    if (!m) {
      notifyDanceFinished(reason);
      return;
    }

    var win = getCurrentWin();
    if (!win) {
      notifyDanceFinished(reason);
      return;
    }

    // 获取当前实际位置作为起点（舞蹈过程中可能已漂移）
    try { var curPos = await win.outerPosition(); } catch (_) {
      notifyDanceFinished(reason);
      return;
    }

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
        notifyDanceFinished(reason);
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
    window.__TAURI__.event.listen('play-dance', async (event) => {
      var payload = event.payload;
      console.log('[dance] 收到播放指令:', payload.name, '-', payload.steps.length, '步, loop=', payload.loop_, 'max_ms=', payload.max_duration_ms);

      var win = getCurrentWin();
      var metrics = win ? await getDanceScreenMetrics(win) : null;

      dancePlayer = {
        steps: payload.steps,
        index: 0,
        repeatIndex: 0,
        time: 0,
        elapsed: 0,
        maxDurationMs: typeof payload.max_duration_ms === 'number' ? payload.max_duration_ms : null,
        loop_: payload.loop_ !== false,
        metrics: metrics,
      };
      bodyEl.classList.add('dancing');
      console.log('[dance] ▶ 舞蹈播放器启动, 屏幕:', metrics ? metrics.screenW + 'x' + metrics.screenH : '未知');
    });
  }

  document.addEventListener('DOMContentLoaded', init);

  // 暴露纯函数供测试访问
  window.PetApp = {
    isMouthHotzone: isMouthHotzone,
    isLeftEyeHotzone: isLeftEyeHotzone,
  };
})();
