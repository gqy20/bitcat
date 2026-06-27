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
  const READING_H = 230;
  const EXPANDED_H = 390;
  const MAX_H = 440;
  const READER_W = 440;
  const READER_MAX_H = 560;
  const STREAM_READING_W = 360;
  const STREAM_EXPANDED_W = 400;
  const COMPOSE_W = 360;
  const NOTICE_W = 300;
  const MIN_W = 260;
  const MAX_W = 480;
  const ABS_MAX_H = 680;      // 用户手动拖拽时的绝对最大高度
  const PADDING_TOTAL = 50;   // body top(6) + bubble padding-top(14) + padding-bottom(14) + body bottom(12) + 余量(4)
  const INPUT_ROW_H = 50;     // input-row extra height, including divider and spacing
  const CHAT_CONTROLS_H = 42;  // chat-controls extra height, including divider and spacing
  const AUTO_RESIZE_DEBOUNCE_MS = 140;
  const LONG_REPLY_CHARS = 280;
  let hideTimer = null;
  let performanceHideTimer = null;
  let contentEl = null;
  let bodyEl = null;           // #contentBody：唯一被 innerHTML 覆盖的节点
  let toolStatusEl = null;
  let pollTimer = null;
  let currentWinH = MIN_H;
  let currentWinW = NOTICE_W;
  let lastRawText = '';       // 记录最近一次原始文本，用于最终渲染去光标
  let inputRowEl = null;
  let inputEl = null;
  let sendBtnEl = null;
  let resizeGripEl = null;      // resize 手柄元素
  let collapseBtnEl = null;
  let chatControlsEl = null;
  let replyChipEl = null;
  let readChipEl = null;
  let copyChipEl = null;
  let stopBtnEl = null;
  let controlNoteEl = null;
  let isComposing = false;    // IME 组合状态标记
  let userScrolledUp = false;  // 用户是否手动向上滚动了（锁定自动跟底）
  let streaming = false;        // 是否处于流式输出中（bubble-end 后为 false 拦截迟到的轮询）
  let cancelled = false;
  let autoSizeStage = 'compact'; // 'compact' | 'reading' | 'expanded'
  let autoResizeTimer = null;
  let lastAutoResizeAt = 0;
  let bubbleMode = 'notice';     // 'notice' | 'stream' | 'compose'
  let readingMode = false;
  let chatControlState = 'hidden';

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

  function setBubbleMode(mode) {
    bubbleMode = mode || 'notice';
    document.body.classList.toggle('notice', bubbleMode === 'notice');
    document.body.classList.toggle('stream', bubbleMode === 'stream');
    document.body.classList.toggle('compose', bubbleMode === 'compose');
  }

  function setReadingMode(enabled) {
    readingMode = !!enabled;
    document.body.classList.toggle('reading-mode', readingMode);
  }

  function isReplyControlState(mode) {
    return mode === 'reply' || mode === 'stopped';
  }

  function hasReplyText() {
    return !!lastRawText;
  }

  function hasLongReplyText() {
    return lastRawText && lastRawText.length > LONG_REPLY_CHARS;
  }

  const TOOL_STATUS_COPY = {
    create_reminder: {
      planned: '正在设置提醒',
      finished: '提醒已设置',
      failed: '提醒没设置成功',
      blocked: '提醒设置被拦截',
    },
    list_reminders: {
      planned: '正在查看提醒',
      finished: '提醒列表已更新',
      failed: '提醒列表读取失败',
      blocked: '查看提醒被拦截',
    },
    cancel_reminder: {
      planned: '正在取消提醒',
      finished: '提醒已取消',
      failed: '提醒取消失败',
      blocked: '取消提醒被拦截',
    },
    shell: {
      planned: '正在执行命令',
      finished: '命令执行完了',
      failed: '命令执行失败',
      blocked: '命令被拦截',
    },
    read_file: {
      planned: '正在看文件',
      finished: '文件看完了',
      failed: '文件读取失败',
      blocked: '读取文件被拦截',
    },
    recent_screenshots: {
      planned: '正在回看屏幕',
      finished: '屏幕记录看完了',
      failed: '屏幕记录读取失败',
      blocked: '回看屏幕被拦截',
    },
    search_memory: {
      planned: '正在找记忆',
      finished: '找到了相关记忆',
      failed: '记忆检索失败',
      blocked: '检索记忆被拦截',
    },
    remember: {
      planned: '正在记住这件事',
      finished: '已经记住了',
      failed: '保存记忆失败',
      blocked: '保存记忆被拦截',
    },
    read_clipboard: {
      planned: '正在看剪贴板',
      finished: '剪贴板看完了',
      failed: '剪贴板读取失败',
      blocked: '读取剪贴板被拦截',
    },
    get_time: {
      planned: '正在确认时间',
      finished: '时间已确认',
      failed: '时间读取失败',
      blocked: '查看时间被拦截',
    },
    launch_program: {
      planned: '正在启动程序',
      finished: '程序已启动',
      failed: '程序启动失败',
      blocked: '启动程序被拦截',
    },
    send_hotkey: {
      planned: '正在发送快捷键',
      finished: '快捷键已发送',
      failed: '快捷键发送失败',
      blocked: '发送快捷键被拦截',
    },
    force_foreground: {
      planned: '正在切换窗口',
      finished: '窗口已切换',
      failed: '窗口切换失败',
      blocked: '切换窗口被拦截',
    },
  };

  function ensureVisible(options) {
    var opts = options || {};
    var applyUserPref = Object.prototype.hasOwnProperty.call(opts, 'applyUserPref')
      ? opts.applyUserPref
      : bubbleMode === 'compose';
    document.body.classList.remove('hidden');
    void document.body.offsetWidth;
    // 只有主动聊天/输入态应用用户手动尺寸；普通提醒保持轻量 toast。
    if (applyUserPref && resizeMode === 'auto' && userPrefSize && window.__TAURI__ && window.__TAURI__.window) {
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
    if (readingMode) return;
    hideTimer = setTimeout(hide, HIDE_AFTER_MS);
  }

  function setChatControls(state) {
    if (!chatControlsEl) return;
    var mode = state || 'hidden';
    chatControlState = mode;
    var visible = mode !== 'hidden';
    chatControlsEl.style.display = visible ? 'flex' : 'none';
    if (stopBtnEl) stopBtnEl.style.display = mode === 'streaming' ? 'inline-flex' : 'none';
    if (replyChipEl) replyChipEl.style.display = isReplyControlState(mode) ? 'inline-flex' : 'none';
    if (readChipEl) {
      var canRead = isReplyControlState(mode) && hasLongReplyText();
      readChipEl.style.display = canRead ? 'inline-flex' : 'none';
      setChipLabel(readChipEl, readingMode ? '收起' : '展开阅读');
      readChipEl.setAttribute('aria-label', readingMode ? '收起阅读' : '展开阅读');
      readChipEl.title = readingMode ? '收起' : '展开阅读';
    }
    if (copyChipEl) {
      copyChipEl.style.display = isReplyControlState(mode) && hasReplyText() ? 'inline-flex' : 'none';
      if (copyChipEl.dataset.state !== 'copied') setChipLabel(copyChipEl, '复制');
    }
    if (controlNoteEl) controlNoteEl.style.display = mode === 'stopped' ? 'inline-flex' : 'none';
  }

  function setChipLabel(chip, text) {
    if (!chip) return;
    var label = chip.querySelector('.control-label');
    if (label) {
      label.textContent = text;
    } else {
      chip.textContent = text;
    }
  }

  function copyLastReply() {
    if (!lastRawText) return;
    var text = lastRawText;
    var done = function() {
      if (!copyChipEl) return;
      copyChipEl.dataset.state = 'copied';
      setChipLabel(copyChipEl, '已复制');
      setTimeout(function() {
        if (!copyChipEl) return;
        delete copyChipEl.dataset.state;
        setChipLabel(copyChipEl, '复制');
      }, 1200);
    };
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(done).catch(function() {});
    } else {
      var ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand('copy'); done(); } catch (e) {}
      ta.remove();
    }
  }

  function hideChatControls() {
    setChatControls('hidden');
  }

  function startPerformanceHideTimer() {
    clearHideTimer();
    performanceHideTimer = setTimeout(hide, PERFORMANCE_HIDE_AFTER_MS);
  }

  function syncCssSize(width, height) {
    document.documentElement.style.width = width + 'px';
    document.body.style.width = width + 'px';
    if (height) {
      document.documentElement.style.minHeight = height + 'px';
      document.body.style.minHeight = height + 'px';
    }
  }

  function setManualSizeClass(enabled) {
    document.body.classList.toggle('manual-size', !!enabled);
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
        syncCssSize(targetW, targetH);
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

  function heightForStage(stage) {
    switch (stage) {
      case 'expanded': return EXPANDED_H;
      case 'reading': return READING_H;
      default: return MIN_H;
    }
  }

  function widthForStage(mode, stage, inputOpen) {
    if (readingMode) return READER_W;
    if (mode === 'notice') return NOTICE_W;
    if (mode === 'compose' || inputOpen) return COMPOSE_W;
    if (mode === 'stream') {
      return stage === 'expanded' ? STREAM_EXPANDED_W : STREAM_READING_W;
    }
    return NOTICE_W;
  }

  function chooseAutoSizeStage(neededH, options) {
    var opts = options || {};
    var currentStage = opts.currentStage || 'compact';
    var hasText = !!opts.hasText;
    var isStreaming = !!opts.streaming;
    var inputOpen = !!opts.inputOpen;
    var mode = opts.mode || 'notice';

    if (mode === 'notice') {
      return 'compact';
    }

    if (isStreaming && hasText) {
      if (currentStage === 'expanded' && neededH > READING_H - 12) return 'expanded';
      return neededH > READING_H + 24 ? 'expanded' : 'reading';
    }

    if (inputOpen) {
      if (currentStage === 'expanded' && neededH > READING_H - 12) return 'expanded';
      return neededH > READING_H + 24 ? 'expanded' : 'reading';
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
    var controlsExtra = (chatControlsEl && chatControlsEl.style.display !== 'none') ? CHAT_CONTROLS_H : 0;
    var rawNeededH = contentH + PADDING_TOTAL + inputExtra + controlsExtra;
    var neededH = Math.min(MAX_H, Math.max(MIN_H, rawNeededH));

    // MANUAL 模式：以用户偏好为下界，绝对最大高度也放宽
    var targetW = NOTICE_W;
    if (readingMode) {
      neededH = Math.min(READER_MAX_H, Math.max(EXPANDED_H, rawNeededH));
      targetW = READER_W;
      autoSizeStage = 'expanded';
    } else if (resizeMode === 'manual' && userPrefSize) {
      var manualSize = clampManualSize(userPrefSize.w, userPrefSize.h);
      neededH = manualSize.h;
      targetW = manualSize.w;
      autoSizeStage = 'manual';
      setManualSizeClass(true);
    } else {
      setManualSizeClass(false);
      var inputOpen = inputRowEl && inputRowEl.style.display !== 'none';
      autoSizeStage = chooseAutoSizeStage(neededH, {
        currentStage: autoSizeStage,
        hasText: !!lastRawText,
        streaming: streaming,
        inputOpen: inputOpen,
        mode: bubbleMode,
      });
      var stageH = heightForStage(autoSizeStage);
      neededH = Math.min(MAX_H, Math.max(MIN_H, stageH, rawNeededH));
      targetW = widthForStage(bubbleMode, autoSizeStage, inputOpen);
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

  function escapeHtml(value) {
    return String(value ?? '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function renderMarkdownText(text) {
    return typeof marked !== 'undefined'
      ? marked.parse(text) : escapeHtml(text);
  }

  function renderComposeEmptyState() {
    if (!bodyEl) return;
    bodyEl.innerHTML = `
      <div class="compose-empty">
        <div class="compose-empty-title">想做点什么？</div>
        <div class="compose-empty-subtitle">直接说就好，或者先选一个开头。</div>
        <div class="compose-empty-actions">
          <button class="compose-empty-chip" type="button" data-prompt="5分钟后提醒我">设个提醒</button>
          <button class="compose-empty-chip" type="button" data-prompt="看看我最近在做什么">看看最近</button>
          <button class="compose-empty-chip" type="button" data-prompt="继续刚才的话题">继续聊</button>
        </div>
      </div>`;
  }

  function maybeRenderComposeEmptyState() {
    if (!bodyEl || lastRawText) return;
    if (bodyEl.textContent && bodyEl.textContent.trim()) return;
    renderComposeEmptyState();
  }

  function fillInputDraft(text) {
    if (!inputEl) return;
    inputEl.value = text || '';
    inputEl.focus();
    resetInputIdleTimer();
  }

  function scrollToBottomSoon() {
    if (!contentEl) return;
    contentEl.scrollTop = contentEl.scrollHeight;
    requestAnimationFrame(function() {
      if (!contentEl) return;
      contentEl.scrollTop = contentEl.scrollHeight;
      requestAnimationFrame(function() {
        if (contentEl) contentEl.scrollTop = contentEl.scrollHeight;
      });
    });
  }

  function openAgentWatch() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    window.__TAURI__.core.invoke('cmd_agent_watch_refresh').catch(function(e) {
      diag('agent watch open failed: ' + e);
    });
  }

  function isGenericToastDetail(value) {
    var text = String(value || '').trim();
    return !text ||
      text === '任务已完成' ||
      text === '打开看管查看详情' ||
      text === '查看详情';
  }

  function compactAgentToastLine(payload) {
    var title = payload && payload.title ? String(payload.title).trim() : '';
    var detail = payload && payload.detail ? String(payload.detail).trim() : '';
    var context = payload && payload.context ? String(payload.context).trim() : '';
    var contextParts = context ? context.split(/\s*[·•路]\s*/).filter(Boolean) : [];
    var subject = contextParts[0] || '';
    var titleRest = title;
    if (subject && titleRest.indexOf(subject) === 0) {
      titleRest = titleRest.slice(subject.length).trim();
    }
    if (context && title) {
      if (!isGenericToastDetail(detail)) {
        return context + ' · ' + detail;
      }
      return titleRest ? context + ' · ' + titleRest : context;
    }
    if (title && !isGenericToastDetail(detail)) {
      return title + ' · ' + detail;
    }
    if (title) return title;
    if (detail) return detail;
    if (context) return context;
    return 'Agent 更新';
  }

  function showAgentToast(payload) {
    streaming = false;
    stopPolling();
    clearToolStatus();
    hideChatControls();
    resizeMode = 'auto';
    autoSizeStage = 'compact';
    setBubbleMode('notice');
    if (inputRowEl) {
      inputRowEl.style.display = 'none';
      inputRowEl.classList.remove('visible', 'hiding');
    }
    if (inputEl) inputEl.value = '';
    hideThinking();
    resizeBubbleWindow(NOTICE_W, 68, true);
    lastRawText = '';
    if (bodyEl) {
      var tone = payload && payload.tone ? String(payload.tone) : 'info';
      var line = compactAgentToastLine(payload);
      bodyEl.innerHTML = `
        <button class="agent-toast tone-${escapeHtml(tone)}" type="button" id="agentToastOpen">
          <span class="agent-toast-mark" aria-hidden="true"></span>
          <span class="agent-toast-copy">
            <span class="agent-toast-line">${escapeHtml(line)}</span>
          </span>
        </button>`;
      var openBtn = document.getElementById('agentToastOpen');
      if (openBtn) openBtn.addEventListener('click', openAgentWatch);
    }
    ensureVisible();
    clearHideTimer();
    hideTimer = setTimeout(hide, 8000);
  }

  function setText(text, options) {
    if (!bodyEl) return;
    options = options || {};
    var wasAtBottom = isNearBottom();
    lastRawText = text || '';

    // 隐藏思考指示器（有内容了）
    hideThinking();

    var html = renderMarkdownText(lastRawText);

    // 仅写正文，光标完全由 CSS + class 驱动，永不拼接到 HTML 字符串里
    bodyEl.innerHTML = html;

    // 兜底：如果没有正在轮询，强制 idle（即便 bubble-end 事件丢失也能收回光标）
    if (!pollTimer) setStreamingClass(false);

    var shouldFollowBottom = options.forceScrollBottom || (!userScrolledUp && wasAtBottom);
    // 用户滚回底部 → 解锁
    if (isNearBottom()) {
      userScrolledUp = false;
    }
    autoResize();
    if (shouldFollowBottom) {
      scrollToBottomSoon();
    }
  }

  function hide() {
    stopPolling();
    clearToolStatus();
    hideChatControls();
    hideInput('hide-bubble');
    resizeMode = 'auto';
    autoSizeStage = 'compact';
    setReadingMode(false);
    setManualSizeClass(false);
    if (autoResizeTimer) {
      clearTimeout(autoResizeTimer);
      autoResizeTimer = null;
    }
    diag('resize: hide ' + (userPrefSize ? 'keep manual pref' : 'reset window to default') + ', pref=' +
         (userPrefSize ? (userPrefSize.w + 'x' + userPrefSize.h) : 'none') +
         ' current=' + currentWinW + 'x' + currentWinH);
    if (!userPrefSize) {
      resizeBubbleWindow(NOTICE_W, MIN_H, false);
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
    hideChatControls();
    inputRowEl.classList.remove('hiding');
    inputRowEl.classList.add('visible');
    setBubbleMode('compose');
    clearHideTimer();
    maybeRenderComposeEmptyState();
    ensureVisible({ applyUserPref: true });
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
    setBubbleMode('stream');
    hideChatControls();
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

  function submitPrompt(text) {
    if (!inputEl) return;
    inputEl.value = text || '';
    submitChat();
  }

  function cancelChat() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    cancelled = true;
    streaming = false;
    stopPolling();
    hideThinking();
    clearToolStatus();
    setStreamingClass(false);
    setChatControls('stopped');
    autoResize();
    clearHideTimer();
    window.__TAURI__.core.invoke('cmd_cancel_chat').catch(function(e) {
      diag('cmd_cancel_chat failed: ' + e);
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

  function getPerformanceToolStatusText(payload, phase, toolName) {
    var isDanceTool = toolName === 'perform_dance' || toolName === 'play_dance';
    if (!isDanceTool) return null;
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

  function getFallbackToolStatusText(label, phase) {
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

  function getToolStatusText(payload) {
    var label = payload && payload.label ? payload.label : '调用工具';
    var phase = payload && payload.phase ? payload.phase : 'planned';
    var kind = payload && payload.kind ? payload.kind : 'utility';
    var toolName = payload && payload.tool_name ? String(payload.tool_name) : '';
    if (TOOL_STATUS_COPY[toolName] && TOOL_STATUS_COPY[toolName][phase]) {
      return TOOL_STATUS_COPY[toolName][phase];
    }
    if (kind === 'performance') {
      var performanceText = getPerformanceToolStatusText(payload, phase, toolName);
      if (performanceText) return performanceText;
    }
    return getFallbackToolStatusText(label, phase);
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
    setBubbleMode('stream');
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
    cancelled = false;
    setReadingMode(false);
    setBubbleMode('stream');
    setChatControls('streaming');
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
    if (cancelled) {
      return;
    }
    streaming = false;
    stopPolling();
    userScrolledUp = false;
    hideThinking();
    clearToolStatus();
    setStreamingClass(false);
    if (finalText && finalText.length > 0) {
      setText(finalText, { forceScrollBottom: true });
      ensureVisible();
    } else if (lastRawText) {
      setText(lastRawText, { forceScrollBottom: true });
      ensureVisible();
    }
    if (lastRawText) {
      setChatControls('reply');
      autoResize();
    }
    startHideTimer();
  }

  function showNoticeText(text) {
    streaming = false;
    stopPolling();
    clearToolStatus();
    resizeMode = 'auto';
    autoSizeStage = 'compact';
    setReadingMode(false);
    setBubbleMode('notice');
    hideChatControls();
    hideInput('notice');
    resizeBubbleWindow(NOTICE_W, MIN_H, true);
    setText(text, { forceScrollBottom: true });
    ensureVisible();
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
    chatControlsEl = document.getElementById('chatControls');
    replyChipEl = document.getElementById('replyChip');
    readChipEl = document.getElementById('readChip');
    copyChipEl = document.getElementById('copyChip');
    stopBtnEl = document.getElementById('stopBtn');
    controlNoteEl = document.getElementById('controlNote');

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
    if (bodyEl) {
      bodyEl.addEventListener('click', function(e) {
        var chip = e.target && e.target.closest ? e.target.closest('.compose-empty-chip') : null;
        if (!chip) return;
        e.preventDefault();
        e.stopPropagation();
        submitPrompt(chip.dataset.prompt || chip.textContent || '');
      });
    }
    if (replyChipEl) {
      replyChipEl.addEventListener('click', function(e) {
        e.preventDefault();
        e.stopPropagation();
        showInput();
      });
    }
    if (readChipEl) {
      readChipEl.addEventListener('click', function(e) {
        e.preventDefault();
        e.stopPropagation();
        setReadingMode(!readingMode);
        setChatControls(chatControlState === 'stopped' ? 'stopped' : 'reply');
        clearHideTimer();
        autoResize();
        if (contentEl && readingMode) {
          contentEl.scrollTop = 0;
        }
      });
    }
    if (copyChipEl) {
      copyChipEl.addEventListener('click', function(e) {
        e.preventDefault();
        e.stopPropagation();
        copyLastReply();
        clearHideTimer();
        if (!readingMode) startHideTimer();
      });
    }
    if (stopBtnEl) {
      stopBtnEl.addEventListener('click', function(e) {
        e.preventDefault();
        e.stopPropagation();
        cancelChat();
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
        setManualSizeClass(true);
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
        setManualSizeClass(false);
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
          syncCssSize(w, h);
          setManualSizeClass(resizeMode === 'manual');
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

    listen('bubble-cancelled', () => {
      cancelled = true;
      streaming = false;
      stopPolling();
      hideThinking();
      clearToolStatus();
      setStreamingClass(false);
      setChatControls('stopped');
      autoResize();
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
        showNoticeText(txt);
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
  window.__bubble_showAgentToast = showAgentToast;
  window.__bubble_compactAgentToastLine = compactAgentToastLine;
  // Rust 端通过 eval 直接触发此函数拉取 pending_text。
  window.__bubble_onShow = function() {
    pollPending().then(function(txt) {
      if (txt && txt.length > 0) {
        showNoticeText(txt);
      }
    });
  };
})();
