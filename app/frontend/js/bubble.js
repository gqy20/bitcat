// bubble.js — 独立气泡窗口前端
//
// 策略: 前端通过定时轮询 cmd_consume_bubble_text 拉取后端累积的文本,
// 完全不依赖逐 chunk 事件的到达时序。
// bubble-start/end 表达生成生命周期；主动会话保留输入与草稿，短回应按时退场。
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
  const COMPOSE_W = 400;
  const CHAT_H = 340;
  const CHAT_MIN_H = 240;
  const NOTICE_W = 300;
  const MIN_W = 260;
  const MAX_W = 480;
  const ABS_MAX_H = 680;      // 用户手动拖拽时的绝对最大高度
  const SHELL_PADDING_H = 50;
  const HEADER_MIN_H = 32;
  const HEADER_GAP_H = 6;
  const INPUT_ROW_H = 50;     // input-row extra height, including divider and spacing
  const AUTO_RESIZE_DEBOUNCE_MS = 140;
  const LONG_REPLY_CHARS = 280;
  let hideTimer = null;
  let performanceHideTimer = null;
  let contentEl = null;
  let bodyEl = null;           // #contentBody：唯一被 innerHTML 覆盖的节点
  let toolStatusEl = null;
  let toolProgressEl = null;
  let toolProgressLabelEl = null;
  let thinkingEl = null;
  let headerStatus = 'none'; // 'none' | 'thinking' | 'tool' | 'stopped'
  let pollTimer = null;
  let currentWinH = MIN_H;
  let currentWinW = NOTICE_W;
  let lastRawText = '';       // 记录最近一次原始文本，用于最终渲染去光标
  let inputRowEl = null;
  let inputEl = null;
  let sendBtnEl = null;
  let resizeGripEl = null;      // resize 手柄元素
  let collapseBtnEl = null;
  let bubbleHeaderEl = null;
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
  let chatSessionActive = false;
  let hiddenByUser = false;
  let submitting = false;
  let stopping = false;
  let awaitingStreamStart = false;
  let submissionStarted = false;
  let pollEpoch = 0;
  let viewEpoch = 0;
  let hideWindowTimer = null;
  let userMessageEl = null;
  let chatFeedbackEl = null;
  let initialized = false;
  let conversationHistoryEl = null;
  let conversationHistory = [];
  let lastConversationText = '';
  let lastConversationUser = '';
  let conversationScrollTop = 0;
  let conversationReadingMode = false;
  let readingAnchorPending = false;
  let lastReplyStopped = false;
  let pendingDraft = null;
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
    if (readingMode) {
      chatSessionActive = true;
      document.body.classList.add('chat-session');
      clearHideTimer();
      notifyChatEnter('reading');
    }
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
    if (hiddenByUser && !opts.userInitiated) return;
    if (opts.userInitiated) hiddenByUser = false;
    if (hideWindowTimer) { clearTimeout(hideWindowTimer); hideWindowTimer = null; }
    var wasHidden = document.body.classList.contains('hidden');
    document.body.classList.remove('hidden');
    if (wasHidden) void document.body.offsetWidth;
    document.body.classList.add('show');
    if (opts.applyUserPref && resizeMode === 'auto' && userPrefSize) {
      resizeMode = 'manual';
      var size = clampManualSize(userPrefSize.w, userPrefSize.h);
      resizeBubbleWindow(size.w, Math.max(CHAT_MIN_H, size.h), true);
    }
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
    if (readingMode || chatSessionActive || submitting) return;
    hideTimer = setTimeout(hide, HIDE_AFTER_MS);
  }

  function updateComposer() {
    if (!sendBtnEl) return;
    sendBtnEl.disabled = !streaming && (submitting || stopping || !(inputEl && inputEl.value.trim()));
    sendBtnEl.textContent = streaming ? '停止' : (stopping ? '稍等' : (submitting ? (cancelled ? '稍等' : '发送中') : '发送'));
    sendBtnEl.title = streaming ? '停止回复' : '发送消息';
    sendBtnEl.setAttribute('aria-label', streaming ? '停止回复' : '发送消息');
    sendBtnEl.classList.toggle('stop', streaming);
    var stopText = stopping ? '正在停止' : '已停止';
    if (controlNoteEl && controlNoteEl.textContent !== stopText) controlNoteEl.textContent = stopText;
    if (inputRowEl) inputRowEl.setAttribute('aria-busy', submitting || stopping ? 'true' : 'false');
    renderHeaderStatus();
  }

  // 普通过程与停止共用一处；失败说明独立留在正文，避免窄栏截断。
  function setHeaderStatus(kind) {
    headerStatus = kind;
    renderHeaderStatus();
  }

  function renderHeaderStatus() {
    if (thinkingEl) thinkingEl.style.display = headerStatus === 'thinking' ? 'flex' : 'none';
    if (toolProgressEl) toolProgressEl.style.display = headerStatus === 'tool' ? 'flex' : 'none';
    if (controlNoteEl) controlNoteEl.style.display = headerStatus === 'stopped' ? 'inline-flex' : 'none';
  }

  function setFeedback(text, kind) {
    if (!chatFeedbackEl) return;
    chatFeedbackEl.textContent = text || '';
    chatFeedbackEl.hidden = !text;
    chatFeedbackEl.dataset.kind = kind || 'info';
    chatFeedbackEl.setAttribute('role', kind === 'error' ? 'alert' : 'status');
  }

  function setUserMessage(text, follow) {
    if (!userMessageEl) return;
    userMessageEl.textContent = text || '';
    userMessageEl.hidden = !text;
    if (follow) scrollToBottomSoon();
  }

  function archiveConversationTurn() {
    if (!lastConversationText) return;
    conversationHistory.push({ user: lastConversationUser, reply: lastConversationText, stopped: lastReplyStopped });
    conversationHistory = conversationHistory.slice(-2);
    if (conversationHistoryEl) {
      conversationHistoryEl.innerHTML = conversationHistory.map(function(turn) {
        return '<section class="conversation-turn">' +
          (turn.user ? '<div class="user-message">' + escapeHtml(turn.user) + '</div>' : '') +
          '<div class="previous-reply">' + renderMarkdownText(turn.reply) + '</div>' +
          (turn.stopped ? '<span class="previous-status">已停止</span>' : '') +
          '</section>';
      }).join('');
    }
  }

  function syncInputLayout() {
    if (inputEl && inputEl.tagName === 'TEXTAREA') {
      inputEl.style.height = 'auto';
      inputEl.style.height = Math.max(32, Math.min(74, inputEl.scrollHeight)) + 'px';
    }
    updateComposer();
    autoResize();
  }

  function setChatControls(state) {
    if (!chatControlsEl) return;
    var mode = state || 'hidden';
    chatControlState = mode;
    var canReply = isReplyControlState(mode);
    var canRead = canReply && hasLongReplyText();
    var canCopy = canReply && hasReplyText();
    var legacyStop = mode === 'streaming' && !chatSessionActive;
    var legacyReply = canReply && !chatSessionActive;
    chatControlsEl.style.display = canRead || canCopy || legacyStop || legacyReply ? 'flex' : 'none';
    if (stopBtnEl) stopBtnEl.style.display = legacyStop ? 'inline-flex' : 'none';
    if (replyChipEl) replyChipEl.style.display = legacyReply ? 'inline-flex' : 'none';
    if (readChipEl) {
      readChipEl.style.display = canRead ? 'inline-flex' : 'none';
      setChipLabel(readChipEl, readingMode ? '返回聊天' : '展开阅读');
      readChipEl.setAttribute('aria-label', readingMode ? '返回聊天' : '展开阅读');
      readChipEl.title = readingMode ? '返回聊天' : '展开阅读';
    }
    if (copyChipEl) {
      copyChipEl.style.display = canCopy ? 'inline-flex' : 'none';
      if (copyChipEl.dataset.state !== 'copied') setChipLabel(copyChipEl, '复制');
    }
    if (mode === 'stopped') setHeaderStatus('stopped');
    else if (headerStatus === 'stopped') setHeaderStatus('none');
    updateComposer();
  }

  function setChipLabel(chip, text) {
    if (!chip) return;
    var label = chip.querySelector('.control-label');
    if (label) {
      label.textContent = text;
    } else if (!chip.classList.contains('icon-only')) {
      chip.textContent = text;
    }
    chip.title = text;
    chip.setAttribute('aria-label', text === '复制' ? '复制回复' : (text === '已复制' ? '已复制回复' : text));
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
        alignReadingAnchor();
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
    var targetW;
    var neededH;
    var headerH = bubbleMode === 'notice' ? 0 : Math.max(HEADER_MIN_H, bubbleHeaderEl ? bubbleHeaderEl.offsetHeight : 0) + HEADER_GAP_H;
    var paddingH = SHELL_PADDING_H + headerH;
    if (chatSessionActive) {
      var size = userPrefSize && resizeMode === 'manual' ? clampManualSize(userPrefSize.w, userPrefSize.h) : null;
      var composerH = inputRowEl && inputRowEl.style.display !== 'none' ? Math.max(INPUT_ROW_H, inputRowEl.offsetHeight + 10) : 0;
      var feedbackH = chatFeedbackEl && !chatFeedbackEl.hidden ? chatFeedbackEl.offsetHeight + 8 : 0;
      // 手动缩小时仍留出可读正文；草稿和错误提示按真实高度保留空间。
      var minimumChatH = Math.max(CHAT_MIN_H, paddingH + 72 + composerH + feedbackH);
      targetW = readingMode ? READER_W : (size ? size.w : COMPOSE_W);
      neededH = readingMode ? READER_MAX_H : Math.max(minimumChatH, size ? size.h : CHAT_H);
      autoSizeStage = readingMode ? 'expanded' : 'reading';
      setManualSizeClass(!!size);
    } else {
      var contentH = contentEl.scrollHeight;
      var inputExtra = inputRowEl && inputRowEl.style.display !== 'none' ? INPUT_ROW_H : 0;
      var rawNeededH = contentH + paddingH + inputExtra;
      neededH = Math.min(MAX_H, Math.max(MIN_H, rawNeededH));
      autoSizeStage = chooseAutoSizeStage(neededH, { currentStage: autoSizeStage, hasText: !!lastRawText, streaming: streaming, inputOpen: !!inputExtra, mode: bubbleMode });
      neededH = Math.min(MAX_H, Math.max(heightForStage(autoSizeStage), neededH));
      targetW = widthForStage(bubbleMode, autoSizeStage, !!inputExtra);
    }
    var newH = Math.round(neededH);
    if (newH !== currentWinH || targetW !== currentWinW) scheduleResize(targetW, newH, true);
    else alignReadingAnchor();
  }

  function alignReadingAnchor() {
    if (!readingAnchorPending || !readingMode || !contentEl || hiddenByUser) return;
    readingAnchorPending = false;
    var anchor = userMessageEl && !userMessageEl.hidden ? userMessageEl : bodyEl;
    if (!anchor) return;
    contentEl.scrollTop += anchor.getBoundingClientRect().top - contentEl.getBoundingClientRect().top;
  }

  /// 检测是否已在底部。

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
        <div class="compose-empty-actions">
          <button class="compose-empty-chip" type="button" data-prompt="5分钟后提醒我">设个提醒</button>
          <button class="compose-empty-chip" type="button" data-prompt="看看我最近在做什么">看看最近</button>
          ${conversationHistory.length ? '<button class="compose-empty-chip" type="button" data-prompt="继续刚才的话题">继续聊</button>' : ''}
        </div>
      </div>`;
  }

  function maybeRenderComposeEmptyState() {
    if (!bodyEl || lastRawText) return;
    if (bodyEl.textContent && bodyEl.textContent.trim()) return;
    renderComposeEmptyState();
  }

  function scrollToBottomSoon() {
    if (!contentEl || hiddenByUser || userScrolledUp) return;
    var epoch = viewEpoch;
    contentEl.scrollTop = contentEl.scrollHeight;
    requestAnimationFrame(function() {
      if (contentEl && epoch === viewEpoch && !hiddenByUser && !userScrolledUp) contentEl.scrollTop = contentEl.scrollHeight;
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
    if (chatSessionActive || submitting || streaming) return;
    hiddenByUser = false;
    setUserMessage('');
    setFeedback('');
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
    var oldScrollTop = contentEl.scrollTop;
    var shouldFollowBottom = !options.preserveScroll && (options.forceScrollBottom || (!userScrolledUp && isNearBottom()));
    lastRawText = text || '';
    if (bubbleMode !== 'notice') lastConversationText = lastRawText;
    hideThinking();
    bodyEl.innerHTML = renderMarkdownText(lastRawText);
    if (!pollTimer) setStreamingClass(false);
    autoResize();
    if (shouldFollowBottom && !userScrolledUp) scrollToBottomSoon();
    else contentEl.scrollTop = oldScrollTop;
  }

  function hide() {
    if (bubbleMode !== 'notice') {
      conversationScrollTop = contentEl ? contentEl.scrollTop : 0;
      conversationReadingMode = readingMode;
    }
    hiddenByUser = true;
    viewEpoch += 1;
    stopPolling();
    clearHideTimer();
    hideThinking();
    clearToolStatus();
    hideChatControls();
    hideInput('hide-bubble');
    if (autoResizeTimer) { clearTimeout(autoResizeTimer); autoResizeTimer = null; }
    document.body.classList.remove('show');
    document.body.classList.add('hidden');
    if (hideWindowTimer) clearTimeout(hideWindowTimer);
    if (window.__TAURI__ && window.__TAURI__.core) {
      hideWindowTimer = setTimeout(function() {
        hideWindowTimer = null;
        if (!hiddenByUser) return;
        window.__TAURI__.core.invoke('cmd_hide_bubble').catch(function(e) { diag('hide bubble failed: ' + e); });
      }, HIDE_ANIM_MS);
    }
  }

  /// 同步用户正在输入或阅读的交互保护，与后台生成状态分开。

  function notifyChatEnter(source) {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    window.__TAURI__.core.invoke('cmd_enter_chat').then(function() {
      diag('cmd_enter_chat ✓ source=' + (source || 'unknown'));
    }).catch(function(e) {
      diag('cmd_enter_chat ✗ source=' + (source || 'unknown') + ' err=' + e);
    });
  }

  function handleInputActivity() {
    // 主动会话由用户收起；输入暂停不会清空草稿或退出交互。
    clearHideTimer();
    syncInputLayout();
  }

  function showInput() {
    if (!inputRowEl || !inputEl) return;
    var restoreConversation = !streaming && !!lastConversationText && (hiddenByUser || bubbleMode === 'notice');
    if (restoreConversation) {
      lastRawText = lastConversationText;
      setReadingMode(conversationReadingMode);
      setUserMessage(lastConversationUser);
    } else if (!streaming && bubbleMode === 'notice') {
      // 短通知属于陪伴，不成为首次聊天的问答历史。
      lastRawText = '';
      if (bodyEl) bodyEl.innerHTML = '';
      setUserMessage('');
    }
    chatSessionActive = true;
    hiddenByUser = false;
    viewEpoch += 1;
    document.body.classList.add('chat-session');
    notifyChatEnter('showInput');
    inputRowEl.style.display = 'flex';
    inputRowEl.classList.remove('hiding');
    inputRowEl.classList.add('visible');
    setBubbleMode(streaming ? 'stream' : 'compose');
    clearHideTimer();
    if (lastRawText) setText(lastRawText, { preserveScroll: true });
    else maybeRenderComposeEmptyState();
    if (restoreConversation && contentEl) contentEl.scrollTop = conversationScrollTop;
    ensureVisible({ applyUserPref: true, userInitiated: true });
    setChatControls(streaming ? 'streaming' : (cancelled ? 'stopped' : (hasReplyText() ? 'reply' : 'hidden')));
    if (streaming && !pollTimer) startPolling({ resume: true });
    requestAnimationFrame(function() {
      if (!hiddenByUser && inputEl) inputEl.focus();
    });
    syncInputLayout();
  }

  function hideInput(source) {
    if (!inputRowEl || !inputEl) return;
    inputRowEl.style.display = 'none';
    inputRowEl.classList.remove('visible', 'hiding');
    chatSessionActive = false;
    document.body.classList.remove('chat-session');
    if (window.__TAURI__ && window.__TAURI__.core) window.__TAURI__.core.invoke('cmd_exit_chat').catch(function() {});
    diag('input hidden, draft retained; source=' + (source || 'unknown'));
  }

  function showThinking() {
    if (!thinkingEl) return;
    setBubbleMode('stream');
    hideChatControls();
    setHeaderStatus('thinking');
    if (bodyEl) bodyEl.innerHTML = ''; // 清空正文区（光标由 class 控制，不必碰）
    autoResize();
  }

  function hideThinking() {
    if (headerStatus === 'thinking') setHeaderStatus('none');
  }

  function submitChat() {
    if (!inputEl || !window.__TAURI__ || !window.__TAURI__.core || submitting || stopping || streaming) return;
    var draft = inputEl.value;
    var text = draft.trim();
    if (!text) return;
    submitting = true;
    awaitingStreamStart = true;
    submissionStarted = false;
    cancelled = false;
    pendingDraft = { text: text, draft: draft };
    setFeedback('');
    updateComposer();
    window.__TAURI__.core.invoke('cmd_submit_chat', { text: text })
      .then(function() {
        submitting = false;
        if (inputEl.value === draft) inputEl.value = '';
        setFeedback('');
        syncInputLayout();
        if (!streaming && !cancelled && !submissionStarted) startPolling({ keepSuppressed: hiddenByUser });
        lastConversationUser = text;
        setUserMessage(text, !hiddenByUser);
        pendingDraft = null;
        updateComposer();
      })
      .catch(function(e) {
        submitting = false;
        awaitingStreamStart = false;
        pendingDraft = null;
        // 没有修改输入时恢复原文；用户继续编辑的文字优先保留。
        if (!inputEl.value) inputEl.value = draft;
        setFeedback('这句话没有发出去，原文已保留。请稍后再点发送。', 'error');
        updateComposer();
        syncInputLayout();
        diag('submit failed: ' + String(e));
      });
  }

  function submitPrompt(text) {
    if (!inputEl) return;
    inputEl.value = text || '';
    submitChat();
  }

  function cancelChat() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    stopping = true;
    cancelled = true;
    lastReplyStopped = true;
    streaming = false;
    pollEpoch += 1;
    stopPolling();
    hideThinking();
    clearToolStatus();
    setStreamingClass(false);
    setChatControls('stopped');
    autoResize();
    clearHideTimer();
    setFeedback('');
    updateComposer();
    window.__TAURI__.core.invoke('cmd_cancel_chat').catch(function(e) {
      diag('cmd_cancel_chat failed: ' + e);
      cancelled = false;
      lastReplyStopped = false;
      streaming = true;
      startPolling({ resume: true });
      setFeedback('停止没有成功。应用连接遇到问题，请再点停止。', 'error');
    }).finally(function() {
      stopping = false;
      updateComposer();
    });
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

  function clearToolStatus(options) {
    var preserveFailure = options && options.keepFailure && toolStatusEl &&
      (toolStatusEl.dataset.phase === 'failed' || toolStatusEl.dataset.phase === 'blocked');
    if (toolStatusEl && !preserveFailure) {
      toolStatusEl.textContent = '';
      toolStatusEl.style.display = 'none';
      delete toolStatusEl.dataset.kind;
      delete toolStatusEl.dataset.phase;
    }
    if (toolProgressEl) {
      if (toolProgressLabelEl) toolProgressLabelEl.textContent = '';
      toolProgressEl.removeAttribute('title');
      delete toolProgressEl.dataset.kind;
      delete toolProgressEl.dataset.phase;
    }
    if (headerStatus === 'tool') setHeaderStatus('none');
  }

  function setToolStatus(payload) {
    if (!toolStatusEl || !toolProgressEl || hiddenByUser || (cancelled && !streaming)) return;
    var phase = payload && payload.phase ? payload.phase : 'planned';
    var kind = payload && payload.kind ? payload.kind : 'utility';
    setBubbleMode('stream');
    hideThinking();
    clearToolStatus();
    var text = getToolStatusText(payload);
    var isPerformanceHandoff = kind === 'performance' && phase === 'finished' &&
      payload && (payload.tool_name === 'perform_dance' || payload.tool_name === 'play_dance');
    if (phase === 'failed' || phase === 'blocked') {
      toolStatusEl.textContent = text;
      toolStatusEl.dataset.kind = kind;
      toolStatusEl.dataset.phase = phase;
      toolStatusEl.style.display = 'block';
    } else if (phase !== 'finished' || isPerformanceHandoff) {
      if (toolProgressLabelEl) toolProgressLabelEl.textContent = text;
      toolProgressEl.title = text;
      toolProgressEl.dataset.kind = kind;
      toolProgressEl.dataset.phase = phase;
      setHeaderStatus('tool');
    }
    ensureVisible();
    autoResize();
    if (kind === 'performance' && phase === 'finished' &&
        (payload && (payload.tool_name === 'perform_dance' || payload.tool_name === 'play_dance'))) {
      startPerformanceHideTimer();
    }
  }

  function onInputKeyDown(e) {
    if (isComposing || e.isComposing || e.keyCode === 229) return;
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (streaming) setFeedback('猫还在回复，你可以先写下一句。');
      else submitChat();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      hide();
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

  function startPolling(options) {
    var opts = options || {};
    if (streaming && pollTimer) return;
    stopPolling();
    if (!opts.resume) {
      archiveConversationTurn();
      pollEpoch += 1;
      streaming = true;
      cancelled = false;
      if (!opts.keepSuppressed) hiddenByUser = false;
      userScrolledUp = false;
      lastRawText = '';
      lastConversationText = '';
      lastConversationUser = '';
      lastReplyStopped = false;
      setReadingMode(false);
      setBubbleMode('stream');
      clearToolStatus();
      if (!hiddenByUser) showThinking();
    }
    setChatControls('streaming');
    updateComposer();
    if (hiddenByUser) return;
    var epoch = pollEpoch;
    function read() {
      var view = viewEpoch;
      pollPending().then(function(txt) { onPollResult(txt, epoch, view); });
    }
    pollTimer = setInterval(read, POLL_INTERVAL_MS);
    read();
  }

  function watchForStream() {
    stopPolling();
    var epoch = ++pollEpoch;
    pollTimer = setInterval(function() {
      var view = viewEpoch;
      pollPending().then(function(txt) {
        if (!txt || epoch !== pollEpoch || view !== viewEpoch || hiddenByUser) return;
        stopPolling();
        streaming = true;
        setBubbleMode('stream');
        setChatControls('streaming');
        onPollResult(txt, pollEpoch, viewEpoch);
        startPolling({ resume: true });
      });
    }, POLL_INTERVAL_MS);
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
      .then((result) => result == null ? null : result)
      .catch(function() { return ''; });
  }

  /// 轮询回调：有新文本才渲染（流式模式）
  function onPollResult(txt, epoch, view) {
    if (!streaming || hiddenByUser || epoch !== pollEpoch || view !== viewEpoch || !txt) return;
    if (txt === lastRawText) return;
    setText(txt);
    ensureVisible();
    setStreamingClass(true);
  }

  function finishStreaming(finalText, epoch, view) {
    if (cancelled || epoch !== pollEpoch) return;
    streaming = false;
    awaitingStreamStart = false;
    stopPolling();
    hideThinking();
    clearToolStatus({ keepFailure: true });
    setStreamingClass(false);
    var text = typeof finalText === 'string' ? finalText : lastRawText;
    if (hiddenByUser || view !== viewEpoch) {
      lastRawText = text || '';
      lastConversationText = lastRawText;
      updateComposer();
      return;
    }
    if (text) {
      setText(text, { preserveScroll: userScrolledUp });
      ensureVisible();
      setChatControls('reply');
    } else {
      setText('');
      setChatControls('hidden');
    }
    setFeedback('');
    updateComposer();
    autoResize();
    startHideTimer();
  }

  function showNoticeText(text) {
    if (chatSessionActive || submitting || streaming) return;
    hiddenByUser = false;
    setUserMessage('');
    setFeedback('');
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
    var target = e.target instanceof Element ? e.target : null;
    var nested = target && target.closest('pre, table');
    if (nested) {
      if (e.deltaX || e.shiftKey) return;
      var canScroll = nested.scrollHeight > nested.clientHeight &&
        ((e.deltaY > 0 && nested.scrollTop + nested.clientHeight < nested.scrollHeight) ||
         (e.deltaY < 0 && nested.scrollTop > 0));
      if (canScroll) return;
    }
    e.preventDefault();
    contentEl.scrollTop += e.deltaY;
    if (e.deltaY < 0) userScrolledUp = true;
    else if (isNearBottom()) userScrolledUp = false;
  }

  /// 键盘滚动兜底。

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
    if (initialized) return;
    initialized = true;
    userMessageEl = document.getElementById('userMessage');
    chatFeedbackEl = document.getElementById('chatFeedback');
    contentEl = document.getElementById('content');
    conversationHistoryEl = document.getElementById('conversationHistory');
    bodyEl = document.getElementById('contentBody');
    toolStatusEl = document.getElementById('toolStatus');
    toolProgressEl = document.getElementById('toolProgress');
    toolProgressLabelEl = toolProgressEl && toolProgressEl.querySelector('.progress-label');
    inputRowEl = document.getElementById('inputRow');
    inputEl = document.getElementById('chatInput');
    sendBtnEl = document.getElementById('chatSend');
    thinkingEl = document.getElementById('thinking');
    resizeGripEl = document.getElementById('resizeGrip');
    collapseBtnEl = document.getElementById('collapseBtn');
    bubbleHeaderEl = document.querySelector('.bubble-header');
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

    // 只在内容层处理一次，避免父容器重复处理冒泡事件。
    contentEl.addEventListener('wheel', onWheel, { passive: false });

    // 键盘滚动：使 content 可聚焦，监听方向键/Page/Home/End
    contentEl.setAttribute('tabindex', '0');
    contentEl.addEventListener('keydown', onKeyDown);

    // 输入框事件绑定
    if (inputEl) {
      inputEl.addEventListener('keydown', onInputKeyDown);
      inputEl.addEventListener('compositionstart', onCompositionStart);
      inputEl.addEventListener('compositionend', onCompositionEnd);
      // 用户输入只更新布局和草稿；主动会话不按空闲时间退场
      inputEl.addEventListener('input', function(e) {
        diag('input 事件: valueLen=' + inputEl.value.length +
             ' inputType=' + (e.inputType || 'n/a') +
             ' isComposing=' + isComposing);
        setFeedback('');
        handleInputActivity();
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
        if (streaming) cancelChat();
        else submitChat();
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
        readingAnchorPending = readingMode;
        if (!readingMode) showInput();
        setChatControls(chatControlState === 'stopped' ? 'stopped' : 'reply');
        clearHideTimer();
        autoResize();
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
          syncInputLayout();
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

    listen('bubble-start', () => {
      if (awaitingStreamStart) {
        awaitingStreamStart = false;
        submissionStarted = true;
        if (cancelled) return;
        if (!streaming) startPolling({ keepSuppressed: hiddenByUser });
        if (pendingDraft) {
          lastConversationUser = pendingDraft.text;
          setUserMessage(pendingDraft.text, !hiddenByUser);
          if (inputEl && inputEl.value === pendingDraft.draft) inputEl.value = '';
          syncInputLayout();
        }
        if (hiddenByUser) window.__TAURI__.core.invoke('cmd_hide_bubble').catch(function() {});
      } else if (!streaming) {
        setUserMessage('');
        hiddenByUser = false;
        startPolling();
      }
    });

    listen('bubble-end', (event) => {
      // 停止后立即再发时，上一轮结束可迟于新提交确认，但早于新开始。
      if (event.payload && typeof event.payload.text === 'string' && awaitingStreamStart && !submissionStarted) return;
      const epoch = pollEpoch;
      const view = viewEpoch;
      // 结束快照独立于共享通知缓存，收起后的短通知不能替换回复。
      if (event.payload && typeof event.payload.text === 'string') {
        finishStreaming(event.payload.text, epoch, view);
        return;
      }
      pollPending()
        .then(function(txt) {
          finishStreaming(txt || lastRawText, epoch, view);
        })
        .catch(function() {
          finishStreaming(lastRawText, epoch, view);
        });
    });

    listen('bubble-tool-event', (event) => {
      setToolStatus(event.payload || {});
    });

    listen('bubble-cancelled', () => {
      if (!stopping && awaitingStreamStart && !submissionStarted) return;
      pollEpoch += 1;
      cancelled = true;
      lastReplyStopped = true;
      streaming = false;
      stopPolling();
      hideThinking();
      clearToolStatus();
      setStreamingClass(false);
      setChatControls('stopped');
      updateComposer();
      autoResize();
    });

    // 双击宠物 / cmd_open_chat → 展开输入框
    listen('chat-open', () => {
      diag('✓ 收到 chat-open 事件');
      showInput();
    });

    // 冷窗口：已有内容按短回应显示；空内容只等待，不伪装成正在生成。
    var initialEpoch = pollEpoch;
    pollPending().then(function(txt) {
      if (initialEpoch !== pollEpoch || hiddenByUser) return;
      if (txt && txt.length > 0) {
        if (chatSessionActive) setText(txt, { preserveScroll: true });
        else showNoticeText(txt);
      } else if (txt !== null) {
        watchForStream();
      }
    });
    updateComposer();
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
