// bubble.js — 独立气泡窗口前端
//
// 策略: 前端通过定时轮询 cmd_consume_bubble_text 拉取后端累积的文本,
// 完全不依赖逐 chunk 事件的到达时序。
// bubble-end 事件仅用于知道流式何时结束(停止轮询 + 启动隐藏定时)。
//
// 滚动: Tauri 透明无框窗口中 native scroll 经常失效,
//       用 wheel 事件手动 scrollTop 兜底 + 动态调整窗口高度。

(function() {
  'use strict';

  const HIDE_AFTER_MS = 15000;
  const PERFORMANCE_HIDE_AFTER_MS = 900;
  const POLL_INTERVAL_MS = 120;
  const HIDE_ANIM_MS = 180;
  const MIN_H = 120;
  const READING_H = 220;
  const EXPANDED_H = 320;
  const MAX_H = 340;
  const MIN_W = 220;
  const MAX_W = 420;
  const ABS_MAX_H = 680;      // 用户手动拖拽时的绝对最大高度
  const PADDING_TOTAL = 50;   // body top(6) + bubble padding-top(14) + padding-bottom(14) + body bottom(12) + 余量(4)
  const INPUT_ROW_H = 42;     // input-row 额外高度（含 padding-top:8）
  const AUTO_RESIZE_DEBOUNCE_MS = 140;
  let hideTimer = null;
  let performanceHideTimer = null;
  let contentEl = null;
  let bodyEl = null;           // #contentBody：唯一被 innerHTML 覆盖的节点
  let toolStatusEl = null;
  let pollTimer = null;
  let currentWinH = MIN_H;
  let currentWinW = 260;
  let lastRawText = '';       // 记录最近一次原始文本，用于最终渲染去光标
  let inputRowEl = null;
  let inputEl = null;
  let sendBtnEl = null;
  let resizeGripEl = null;      // resize 手柄元素
  let collapseBtnEl = null;
  let isComposing = false;    // IME 组合状态标记
  let userScrolledUp = false;  // 用户是否手动向上滚动了（锁定自动跟底）
  let streaming = false;        // 是否处于流式输出中（bubble-end 后为 false 拦截迟到的轮询）
  let autoSizeStage = 'compact'; // 'compact' | 'reading' | 'expanded'
  let autoResizeTimer = null;
  let lastAutoResizeAt = 0;

  // ---- Resize 状态 ----
  let resizeMode = 'auto';      // 'auto' | 'manual'
  let userPrefSize = null;      // { w, h } 用户手动设定的偏好尺寸
  let userResizeActive = false;
  let userResizeArmedUntil = 0;
  let programmaticResize = false;

  // ---- 诊断日志（通过 Rust cmd_pet_log 输出到后端 tracing） ----
  function diag(msg) {
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core.invoke('cmd_pet_log', { msg: '[bubble] ' + msg })
        .catch(function() {});
    }
  }

  function ensureVisible() {
    document.body.classList.remove('hidden');
    void document.body.offsetWidth;
    // 首次 show 时，如果有保存的偏好尺寸，先应用
    if (resizeMode === 'auto' && userPrefSize && window.__TAURI__ && window.__TAURI__.window) {
      diag('resize: ensureVisible applying pref w=' + userPrefSize.w + ' h=' + userPrefSize.h +
           ' current=' + currentWinW + 'x' + currentWinH);
      resizeMode = 'manual';
      resizeBubbleWindow(
        Math.max(MIN_W, Math.min(MAX_W, userPrefSize.w)),
        Math.max(MIN_H, Math.min(ABS_MAX_H, userPrefSize.h)),
        true
      );
    } else {
      diag('resize: ensureVisible no pref apply mode=' + resizeMode +
           ' hasPref=' + !!userPrefSize +
           ' hasTauriWindow=' + !!(window.__TAURI__ && window.__TAURI__.window) +
           ' current=' + currentWinW + 'x' + currentWinH);
    }
    document.body.classList.add('show');
  }

  function clearHideTimer() {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    if (performanceHideTimer) {
      clearTimeout(performanceHideTimer);
      performanceHideTimer = null;
    }
  }

  function startHideTimer() {
    clearHideTimer();
    hideTimer = setTimeout(hide, HIDE_AFTER_MS);
  }

  function startPerformanceHideTimer() {
    clearHideTimer();
    performanceHideTimer = setTimeout(hide, PERFORMANCE_HIDE_AFTER_MS);
  }

  function syncCssWidth(width) {
    document.documentElement.style.width = width + 'px';
    document.body.style.width = width + 'px';
  }

  function repositionBubbleWindow() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return Promise.resolve();
    return window.__TAURI__.core.invoke('cmd_reposition_bubble')
      .catch(function(e) {
        diag('reposition failed: ' + e);
      });
  }

  function resizeBubbleWindow(targetW, targetH, shouldReposition) {
    if (!window.__TAURI__ || !window.__TAURI__.window) return Promise.resolve(false);

    var win = window.__TAURI__.window.getCurrentWindow();
    diag('resize: setSize request w=' + targetW + ' h=' + targetH +
         ' shouldReposition=' + !!shouldReposition +
         ' mode=' + resizeMode +
         ' programmatic=' + programmaticResize);
    programmaticResize = true;
    return win.setSize(new window.__TAURI__.window.LogicalSize(targetW, targetH))
      .then(function() {
        currentWinW = targetW;
        currentWinH = targetH;
        syncCssWidth(targetW);
        diag('resize: setSize ok w=' + targetW + ' h=' + targetH +
             ' shouldReposition=' + !!shouldReposition);
        if (shouldReposition) return repositionBubbleWindow();
      })
      .then(function() { return true; })
      .catch(function(e) {
        diag('resize failed: w=' + targetW + ' h=' + targetH + ' err=' + e);
        return false;
      })
      .finally(function() {
        setTimeout(function() { programmaticResize = false; }, 250);
      });
  }

  function stageRank(stage) {
    switch (stage) {
      case 'expanded': return 2;
      case 'reading': return 1;
      default: return 0;
    }
  }

  function heightForStage(stage) {
    switch (stage) {
      case 'expanded': return EXPANDED_H;
      case 'reading': return READING_H;
      default: return MIN_H;
    }
  }

  function chooseAutoSizeStage(neededH, options) {
    var opts = options || {};
    var currentStage = opts.currentStage || 'compact';
    var hasText = !!opts.hasText;
    var isStreaming = !!opts.streaming;
    var inputOpen = !!opts.inputOpen;

    if (inputOpen) {
      return neededH > READING_H + 24 ? 'expanded' : 'reading';
    }

    if (isStreaming && hasText) {
      var desiredDuringStream = neededH > READING_H + 24 ? 'expanded' : 'reading';
      return stageRank(desiredDuringStream) > stageRank(currentStage)
        ? desiredDuringStream
        : currentStage;
    }

    if (neededH > READING_H + 24) return 'expanded';
    if (neededH > MIN_H) return 'reading';
    return 'compact';
  }

  function scheduleResize(targetW, targetH, shouldReposition) {
    if (autoResizeTimer) {
      clearTimeout(autoResizeTimer);
      autoResizeTimer = null;
    }

    var elapsed = Date.now() - lastAutoResizeAt;
    var delay = elapsed >= AUTO_RESIZE_DEBOUNCE_MS ? 0 : AUTO_RESIZE_DEBOUNCE_MS - elapsed;
    autoResizeTimer = setTimeout(function() {
      autoResizeTimer = null;
      lastAutoResizeAt = Date.now();
      resizeBubbleWindow(targetW, targetH, shouldReposition);
    }, delay);
  }

  function currentScaleFactor(win) {
    if (!win || typeof win.scaleFactor !== 'function') return Promise.resolve(1);
    return win.scaleFactor().catch(function() { return 1; });
  }

  function toLogicalSize(size, scale) {
    var factor = Math.max(scale || 1, 0.5);
    return {
      w: Math.round(size.width / factor),
      h: Math.round(size.height / factor),
    };
  }

  function clampManualSize(w, h) {
    return {
      w: Math.max(MIN_W, Math.min(MAX_W, w)),
      h: Math.max(MIN_H, Math.min(ABS_MAX_H, h)),
    };
  }

  /// 根据实际渲染高度动态调整窗口高度
  /// MANUAL 模式下：不收缩到用户设定以下，但内容多时仍可扩展
  function autoResize() {
    if (!contentEl) return;
    var contentH = contentEl.scrollHeight;
    var inputExtra = (inputRowEl && inputRowEl.style.display !== 'none') ? INPUT_ROW_H : 0;
    var neededH = Math.min(MAX_H, Math.max(MIN_H, contentH + PADDING_TOTAL + inputExtra));

    // MANUAL 模式：以用户偏好为下界，绝对最大高度也放宽
    var targetW = 260;
    if (resizeMode === 'manual' && userPrefSize) {
      var manualSize = clampManualSize(userPrefSize.w, userPrefSize.h);
      neededH = manualSize.h;
      targetW = manualSize.w;
      autoSizeStage = 'manual';
    } else {
      var inputOpen = inputRowEl && inputRowEl.style.display !== 'none';
      autoSizeStage = chooseAutoSizeStage(neededH, {
        currentStage: autoSizeStage,
        hasText: !!lastRawText,
        streaming: streaming,
        inputOpen: inputOpen,
      });
      neededH = Math.min(MAX_H, Math.max(MIN_H, heightForStage(autoSizeStage)));
    }

    var newH = Math.round(neededH);
    var sizeChanged = (newH !== currentWinH || targetW !== currentWinW);

    if (sizeChanged) {
      scheduleResize(targetW, newH, true);
    }
  }

  /// 检测是否已在底部（阈值 40px，避免浮点抖动）
  function isNearBottom() {
    if (!contentEl) return true;
    return contentEl.scrollHeight - contentEl.scrollTop - contentEl.clientHeight < 40;
  }

  /// 切换光标的可见性：唯一真相源，与 setText / bubble-end 事件解耦
  /// DOM 常驻节点 + class 切换，不依赖 lastRawText 是否为空
  function setStreamingClass(on) {
    if (!contentEl) return;
    if (on) {
      contentEl.classList.add('streaming');
      contentEl.classList.remove('idle');
    } else {
      contentEl.classList.add('idle');
      contentEl.classList.remove('streaming');
    }
  }

  function setText(text) {
    if (!bodyEl) return;
    var wasAtBottom = isNearBottom();
    lastRawText = text || '';

    // 隐藏思考指示器（有内容了）
    hideThinking();

    var html = typeof marked !== 'undefined'
      ? marked.parse(lastRawText) : lastRawText;

    // 仅写正文，光标完全由 CSS + class 驱动，永不拼接到 HTML 字符串里
    bodyEl.innerHTML = html;

    // 兜底：如果没有正在轮询，强制 idle（即便 bubble-end 事件丢失也能收回光标）
    if (!pollTimer) setStreamingClass(false);

    // 仅当用户未手动上滚 + 原本在底部时才跟底
    if (!userScrolledUp && wasAtBottom) {
      contentEl.scrollTop = contentEl.scrollHeight;
    }
    // 用户滚回底部 → 解锁
    if (isNearBottom()) {
      userScrolledUp = false;
    }
    autoResize();
  }

  function hide() {
    stopPolling();
    clearToolStatus();
    hideInput('hide-bubble');
    resizeMode = 'auto';
    autoSizeStage = 'compact';
    if (autoResizeTimer) {
      clearTimeout(autoResizeTimer);
      autoResizeTimer = null;
    }
    diag('resize: hide ' + (userPrefSize ? 'keep manual pref' : 'reset window to default') + ', pref=' +
         (userPrefSize ? (userPrefSize.w + 'x' + userPrefSize.h) : 'none') +
         ' current=' + currentWinW + 'x' + currentWinH);
    if (!userPrefSize) {
      resizeBubbleWindow(260, MIN_H, false);
    }
    document.body.classList.remove('show');
    document.body.classList.add('hidden');
    if (window.__TAURI__ && window.__TAURI__.core) {
      setTimeout(function() {
        window.__TAURI__.core.invoke('cmd_hide_bubble').catch(function(e) {
          diag('hide bubble failed: ' + e);
        });
      }, HIDE_ANIM_MS);
    }
  }

  // ---- 聊天输入框 ----

  var inputIdleTimer = null;
  const INPUT_IDLE_MS = 5000;

  /// 通知 Rust：进入 chat 模式（置位 chat_active=true，锁住截图/Vision）
  /// 任何展开输入框 / focus 输入框的路径都调此函数，幂等可多次触发
  function notifyChatEnter(source) {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    window.__TAURI__.core.invoke('cmd_enter_chat').then(function() {
      diag('cmd_enter_chat ✓ source=' + (source || 'unknown'));
    }).catch(function(e) {
      diag('cmd_enter_chat ✗ source=' + (source || 'unknown') + ' err=' + e);
    });
  }

  function resetInputIdleTimer() {
    if (inputIdleTimer) clearTimeout(inputIdleTimer);
    inputIdleTimer = setTimeout(function() {
      if (inputRowEl && inputRowEl.style.display !== 'none') {
        diag('⏰ idle timeout 触发 → hideInput (INPUT_IDLE_MS=' + INPUT_IDLE_MS + ')');
        hideInput('idle-timeout');
      }
    }, INPUT_IDLE_MS);
  }

  function showInput() {
    diag('showInput() called, inputRowEl=' + !!inputRowEl + ' inputEl=' + !!inputEl);
    if (!inputRowEl || !inputEl) { diag('ABORT: 元素不存在'); return; }

    // 第一时间通知后端锁住截图（哪怕 focus/click 尚未触发，也不会被 Vision 打断）
    notifyChatEnter('showInput');

    inputRowEl.style.display = 'flex';
    inputRowEl.classList.remove('hiding');
    inputRowEl.classList.add('visible');
    clearHideTimer();
    ensureVisible();
    requestAnimationFrame(function() {
      if (inputEl) {
        inputEl.focus();
        // 检查 focus 是否真的生效（DOM 层面）
        var hasFocus = (document.activeElement === inputEl);
        var hasWinFocus = document.hasFocus();
        diag('focus 尝试: activeElement==inputEl=' + hasFocus +
             ' document.hasFocus=' + hasWinFocus +
             ' display=' + inputEl.style.display +
             ' readOnly=' + inputEl.readOnly +
             ' disabled=' + inputEl.disabled);
      }
    });
    resetInputIdleTimer();
    autoResize();
    diag('showInput 完成');
  }

  function hideInput(source) {
    diag('hideInput() called, source=' + (source || 'unknown') +
         ' curValueLen=' + (inputEl ? inputEl.value.length : -1));
    if (!inputRowEl || !inputEl) return;
    inputRowEl.style.display = 'none';
    inputRowEl.classList.remove('visible');
    inputEl.value = '';
    if (inputIdleTimer) { clearTimeout(inputIdleTimer); inputIdleTimer = null; }
    autoResize();
    // 通知 Rust 退出 chat 模式
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core.invoke('cmd_exit_chat').catch(function() {});
    }
  }

  // ---- 思考指示器 ----

  var thinkingEl = null;

  function showThinking() {
    if (!thinkingEl) return;
    thinkingEl.style.display = 'flex';
    if (bodyEl) bodyEl.innerHTML = ''; // 清空正文区（光标由 class 控制，不必碰）
    autoResize();
  }

  function hideThinking() {
    if (!thinkingEl) return;
    thinkingEl.style.display = 'none';
  }

  function toggleInput() {
    var willShow = inputRowEl && inputRowEl.style.display === 'none';
    diag('toggleInput(): willShow=' + willShow);
    if (willShow) {
      showInput();
    } else {
      hideInput('toggleInput');
    }
  }

  function submitChat() {
    if (!inputEl || !window.__TAURI__ || !window.__TAURI__.core) {
      diag('submitChat ABORT: inputEl=' + !!inputEl + ' tauri=' + !!window.__TAURI__);
      return;
    }
    var text = inputEl.value.trim();
    diag('submitChat(): rawLen=' + inputEl.value.length + ' trimmedLen=' + text.length);
    if (!text) { diag('submitChat ABORT: 空文本'); return; }
    inputEl.value = '';
    // 发送后平滑收起输入框，流式结束后自动重新展开
    if (inputIdleTimer) { clearTimeout(inputIdleTimer); inputIdleTimer = null; }
    window.__TAURI__.core.invoke('cmd_submit_chat', { text: text })
      .then(function() {
        diag('submitChat ✓ cmd_submit_chat 成功，len=' + text.length);
        hideInputSmooth();       // 发送成功后优雅收起
        startPolling();          // 启动轮询等待 AI 流式回复
      })
      .catch(function(e) {
        diag('submitChat ✗ cmd_submit_chat 失败: ' + (e && e.toString ? e.toString() : e));
        console.error('[chat] submit failed:', e);
      });
  }

  /// 平滑收起输入框（CSS 过渡动画 → 再 display:none）
  function hideInputSmooth() {
    if (!inputRowEl) return;
    diag('hideInputSmooth() called');
    inputRowEl.classList.remove('visible');
    inputRowEl.classList.add('hiding');
    if (inputIdleTimer) { clearTimeout(inputIdleTimer); inputIdleTimer = null; }
    autoResize();
    // 等 CSS 过渡完成后再彻底隐藏
    setTimeout(function() {
      if (inputRowEl) {
        inputRowEl.style.display = 'none';
        inputRowEl.classList.remove('hiding');
      }
    }, 280);
  }

  function getToolStatusText(payload) {
    var label = payload && payload.label ? payload.label : '调用工具';
    var phase = payload && payload.phase ? payload.phase : 'planned';
    var kind = payload && payload.kind ? payload.kind : 'utility';
    var toolName = payload && payload.tool_name ? String(payload.tool_name) : '';
    var isDanceTool = toolName === 'perform_dance' || toolName === 'play_dance';
    if (kind === 'performance' && isDanceTool) {
      if (phase === 'blocked') {
        return '表演已拦截';
      }
      if (phase === 'failed') {
        return '编舞失败';
      }
      if (phase === 'finished' || (payload && payload.tool_name === 'play_dance')) {
        return '准备开跳';
      }
      return '正在编舞';
    }
    if (phase === 'blocked') {
      return label + '已拦截';
    }
    if (phase === 'failed') {
      return label + '失败';
    }
    if (phase === 'finished') {
      return label + '完成';
    }
    return '准备' + label;
  }

  function clearToolStatus() {
    if (!toolStatusEl) return;
    toolStatusEl.textContent = '';
    toolStatusEl.style.display = 'none';
    delete toolStatusEl.dataset.kind;
    delete toolStatusEl.dataset.phase;
  }

  function setToolStatus(payload) {
    if (!toolStatusEl) return;
    var phase = payload && payload.phase ? payload.phase : 'planned';
    var kind = payload && payload.kind ? payload.kind : 'utility';
    toolStatusEl.textContent = getToolStatusText(payload);
    toolStatusEl.dataset.kind = kind;
    toolStatusEl.dataset.phase = phase;
    toolStatusEl.style.display = 'block';
    hideThinking();
    ensureVisible();
    autoResize();
    if (kind === 'performance' && phase === 'finished' &&
        (payload && (payload.tool_name === 'perform_dance' || payload.tool_name === 'play_dance'))) {
      startPerformanceHideTimer();
    }
  }

  function onInputKeyDown(e) {
    diag('keydown key=' + e.key + ' code=' + e.code +
         ' composing=' + isComposing +
         ' valueLen=' + (inputEl ? inputEl.value.length : -1));
    switch (e.key) {
      case 'Enter':
        if (!isComposing) { e.preventDefault(); submitChat(); }
        break;
      case 'Escape':
        e.preventDefault(); hideInput('escape-key'); startHideTimer(); break;
    }
  }

  function onCompositionStart() {
    isComposing = true;
    diag('compositionstart (IME 开始)');
  }
  function onCompositionEnd() {
    isComposing = false;
    diag('compositionend (IME 结束), valueLen=' + (inputEl ? inputEl.value.length : -1));
  }

  function startPolling() {
    stopPolling();
    clearToolStatus();
    streaming = true;       // 标记流式开始
    autoSizeStage = 'compact';
    userScrolledUp = false; // 新流式开始，重置锁定
    // 🔧 不立即激活光标：等真正拉到非空文本再切 streaming，
    //    避免 bubble-end 事件丢失时光标常驻
    showThinking();         // 显示思考指示器
    pollTimer = setInterval(function() {
      pollPending().then(onPollResult);
    }, POLL_INTERVAL_MS);
    pollPending().then(onPollResult);
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
    setStreamingClass(false); // 任何 stopPolling 路径都收回光标
  }

  function pollPending() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return Promise.resolve('');
    return window.__TAURI__.core.invoke('cmd_consume_bubble_text')
      .then((result) => result || '')
      .catch(function() { return ''; });
  }

  /// 轮询回调：有新文本才渲染（流式模式）
  function onPollResult(txt) {
    // 流已结束 → 丢弃迟到的轮询结果（clearInterval 无法取消已在途的 IPC）
    if (!streaming) return;

    const len = (txt || '').length;

    if (len === 0) return;

    setText(txt);
    ensureVisible();
    // 🔧 拉到首个非空文本后才激活光标（避免 init 即残留）
    if (contentEl && !contentEl.classList.contains('streaming')) {
      setStreamingClass(true);
    }
  }

  function finishStreaming(finalText) {
    streaming = false;
    stopPolling();
    userScrolledUp = false;
    hideThinking();
    clearToolStatus();
    setStreamingClass(false);
    if (finalText && finalText.length > 0) {
      setText(finalText);
      ensureVisible();
    } else if (lastRawText) {
      setText(lastRawText);
      ensureVisible();
    }
    startHideTimer();
  }

  /// wheel 事件兜底：Tauri 透明窗口的 native scroll 不稳定，
  /// 手动控制 scrollTop 确保滚轮可用
  function onWheel(e) {
    if (!contentEl) return;
    e.preventDefault();
    contentEl.scrollTop += e.deltaY;
    // 用户向上滚动（看历史内容）→ 锁定跟底
    if (e.deltaY < 0) userScrolledUp = true;
    // 滚回底部附近 → 解锁
    else if (isNearBottom()) userScrolledUp = false;
  }

  /// 键盘滚动兜底：方向键/PageDown/PageUp/Home/End 控制滚动
  function onKeyDown(e) {
    if (!contentEl) return;
    var step = 40;
    switch (e.key) {
      case 'ArrowDown':
        contentEl.scrollTop += step; e.preventDefault(); break;
      case 'ArrowUp':
        contentEl.scrollTop -= step; userScrolledUp = true; e.preventDefault(); break;
      case 'PageDown':
        contentEl.scrollTop += contentEl.clientHeight; e.preventDefault(); break;
      case 'PageUp':
        contentEl.scrollTop -= contentEl.clientHeight; userScrolledUp = true; e.preventDefault(); break;
      case 'Home':
        contentEl.scrollTop = 0; userScrolledUp = true; e.preventDefault(); break;
      case 'End':
        contentEl.scrollTop = contentEl.scrollHeight; userScrolledUp = false; e.preventDefault(); break;
    }
    // 下翻后检测是否到底
    if (isNearBottom()) userScrolledUp = false;
  }

  function init() {
    contentEl = document.getElementById('content');
    bodyEl = document.getElementById('contentBody');
    toolStatusEl = document.getElementById('toolStatus');
    inputRowEl = document.getElementById('inputRow');
    inputEl = document.getElementById('chatInput');
    sendBtnEl = document.getElementById('chatSend');
    thinkingEl = document.getElementById('thinking');
    resizeGripEl = document.getElementById('resizeGrip');
    collapseBtnEl = document.getElementById('collapseBtn');

    // marked.js 配置：窄栏必须 breaks:true
    if (typeof marked !== 'undefined') {
      marked.setOptions({
        breaks: true,
        gfm: true,
        headerIds: false,
        mangle: false,
      });
    }

    // 诊断：确认 DOM 元素存在
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core.invoke('cmd_pet_log', {
        msg: '[bubble] init: content=' + !!contentEl +
          ' inputRow=' + !!inputRowEl + ' input=' + !!inputEl + ' sendBtn=' + !!sendBtnEl
      }).catch(function() {});
    }

    if (!contentEl) return;

    // 滚轮兜底：监听 content 和 bubble 容器的 wheel 事件
    contentEl.addEventListener('wheel', onWheel, { passive: false });
    var bubbleEl = contentEl.closest('.bubble');
    if (bubbleEl) {
      bubbleEl.addEventListener('wheel', onWheel, { passive: false });
      // 双击气泡展开输入框
      bubbleEl.addEventListener('dblclick', function(e) {
        // 双击输入框本身不触发 toggle
        if (e.target === inputEl || e.target === sendBtnEl ||
            inputEl && inputEl.contains(e.target)) return;
        diag('bubbleEl dblclick → toggleInput');
        toggleInput();
      });
    }

    // 键盘滚动：使 content 可聚焦，监听方向键/Page/Home/End
    contentEl.setAttribute('tabindex', '0');
    contentEl.addEventListener('keydown', onKeyDown);

    // 输入框事件绑定
    if (inputEl) {
      inputEl.addEventListener('keydown', onInputKeyDown);
      inputEl.addEventListener('compositionstart', onCompositionStart);
      inputEl.addEventListener('compositionend', onCompositionEnd);
      // 任何输入活动重置空闲定时器
      inputEl.addEventListener('input', function(e) {
        diag('input 事件: valueLen=' + inputEl.value.length +
             ' inputType=' + (e.inputType || 'n/a') +
             ' isComposing=' + isComposing);
        resetInputIdleTimer();
      });
      inputEl.addEventListener('focus', function() {
        diag('✓ inputEl focus (获得键盘焦点), docHasFocus=' + document.hasFocus());
        // 保底：用户 focus 输入框时再通知一次（showInput 已触发过也无妨，幂等）
        notifyChatEnter('focus');
      });
      inputEl.addEventListener('blur', function() {
        diag('✗ inputEl blur (失去键盘焦点), docHasFocus=' + document.hasFocus() +
             ' newActive=' + (document.activeElement && document.activeElement.tagName));
      });
      // 点击输入框也记录一下（用于区分"鼠标点进去后能否输入"）
      inputEl.addEventListener('click', function() {
        diag('inputEl click, activeElement==inputEl=' +
             (document.activeElement === inputEl));
        // 保底：用户点击时也确认锁住
        notifyChatEnter('click');
      });
    }
    if (sendBtnEl) {
      sendBtnEl.addEventListener('click', function() {
        diag('sendBtn click');
        submitChat();
      });
    }
    if (collapseBtnEl) {
      collapseBtnEl.addEventListener('click', function(e) {
        e.preventDefault();
        e.stopPropagation();
        hide();
      });
    }

    // ---- Resize Grip 交互 ----
    if (resizeGripEl && window.__TAURI__ && window.__TAURI__.window) {
      var win = window.__TAURI__.window.getCurrentWindow();

      // mousedown → 原生 resize 拖拽
      resizeGripEl.addEventListener('mousedown', function(e) {
        e.preventDefault();
        e.stopPropagation();
        resizeMode = 'manual';
        userResizeActive = true;
        userResizeArmedUntil = Date.now() + 1500;
        diag('resize: grip mousedown start x=' + e.clientX + ' y=' + e.clientY +
             ' current=' + currentWinW + 'x' + currentWinH);
        win.startResizeDragging('SouthEast').catch(function(e) {
          diag('startResizeDragging failed: ' + e);
        });
      });

      // 双击 grip → 回到 AUTO 模式
      resizeGripEl.addEventListener('dblclick', function(e) {
        e.preventDefault();
        e.stopPropagation();
        resizeMode = 'auto';
        userResizeActive = false;
        userPrefSize = null;
        autoSizeStage = 'compact';
        localStorage.removeItem('bubble_pref');
        diag('resize: double-click → reset to auto');
        autoResize();
      });

      // Window size can change either from user dragging or from auto content sizing.
      // Only user dragging should overwrite the persisted preference.
      win.onResized(function() {
        Promise.all([win.innerSize(), currentScaleFactor(win)]).then(function(results) {
          var size = results[0];
          var scale = results[1];
          var logical = toLogicalSize(size, scale);
          var w = logical.w;
          var h = logical.h;
          currentWinW = w;
          currentWinH = h;
          syncCssWidth(w);
          repositionBubbleWindow();
          var userResizeArmed = Date.now() <= userResizeArmedUntil;
          diag('resize: onResized w=' + w + ' h=' + h +
               ' physical=' + Math.round(size.width) + 'x' + Math.round(size.height) +
               ' scale=' + scale +
               ' mode=' + resizeMode +
               ' userActive=' + userResizeActive +
               ' userArmed=' + userResizeArmed +
               ' programmatic=' + programmaticResize +
               ' hasPref=' + !!userPrefSize);
          if (resizeMode === 'manual' && (userResizeActive || userResizeArmed) && !programmaticResize) {
            userPrefSize = clampManualSize(w, h);
            localStorage.setItem('bubble_pref', JSON.stringify(userPrefSize));
            userResizeArmedUntil = Date.now() + 1500;
            diag('resize: manual pref saved w=' + userPrefSize.w + ' h=' + userPrefSize.h);
          } else {
            diag('resize: pref not saved reason mode=' + resizeMode +
                 ' userActive=' + userResizeActive +
                 ' userArmed=' + userResizeArmed +
                 ' programmatic=' + programmaticResize);
          }
        }).catch(function(e) {
          diag('resize read failed: ' + e);
        });
      });
      window.addEventListener('mouseup', function() {
        if (userResizeActive) {
          diag('resize: mouseup stop current=' + currentWinW + 'x' + currentWinH +
               ' pref=' + (userPrefSize ? (userPrefSize.w + 'x' + userPrefSize.h) : 'none'));
        }
        userResizeActive = false;
        userResizeArmedUntil = Date.now() + 1500;
      });
      window.addEventListener('blur', function() {
        if (userResizeActive) {
          diag('resize: blur stop current=' + currentWinW + 'x' + currentWinH +
               ' pref=' + (userPrefSize ? (userPrefSize.w + 'x' + userPrefSize.h) : 'none'));
        }
        userResizeActive = false;
        userResizeArmedUntil = Date.now() + 1500;
      });
    }

    // 从 localStorage 恢复用户偏好
    try {
      var saved = JSON.parse(localStorage.getItem('bubble_pref') || 'null');
      if (saved && saved.w && saved.h) {
        userPrefSize = clampManualSize(saved.w, saved.h);
        // 不立即进入 manual 模式——等首次 show 时再应用
        diag('resize: restored pref from localStorage w=' + saved.w + ' h=' + saved.h +
             ' clamped=' + userPrefSize.w + 'x' + userPrefSize.h);
      }
    } catch (e) { /* ignore parse errors */ }

    if (!window.__TAURI__) return;
    var listen = window.__TAURI__.event.listen;

    listen('bubble-end', () => {
      pollPending()
        .then(function(txt) {
          finishStreaming(txt || lastRawText);
        })
        .catch(function() {
          finishStreaming(lastRawText);
        });
    });

    listen('bubble-tool-event', (event) => {
      setToolStatus(event.payload || {});
    });

    // 双击宠物 / cmd_open_chat → 展开输入框
    listen('chat-open', () => {
      diag('✓ 收到 chat-open 事件');
      showInput();
    });

    // 初始化时消费 pending_text：如果有内容（截图等非流式写入），
    // 直接渲染 + 启动隐藏定时器，不走轮询（避免稳定检测误判 showInput）
    pollPending().then(function(txt) {
      if (txt && txt.length > 0) {
        setText(txt);
        ensureVisible();
        startHideTimer();
      } else {
        startPolling();
      }
    });
  }

  document.addEventListener('DOMContentLoaded', init);

  // 暴露给 Rust eval 调用
  window.__bubble_showInput = showInput;
  window.__bubble_hideInput = hideInput;
  window.__bubble_getToolStatusText = getToolStatusText;
  // Rust 端通过 eval 直接触发此函数拉取 pending_text。
  window.__bubble_onShow = function() {
    pollPending().then(function(txt) {
      if (txt && txt.length > 0) {
        setText(txt);
        ensureVisible();
        startHideTimer();
      }
    });
  };
})();
