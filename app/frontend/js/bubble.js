// bubble.js — 独立气泡窗口前端
//
// 策略: 前端通过定时轮询 cmd_consume_bubble_text 拉取后端累积的文本,
// 完全不依赖 bubble-chunk 事件的到达时序。
// bubble-end 事件仅用于知道流式何时结束(停止轮询 + 启动隐藏定时)。
// bubble-update 用于兼容非流式一次性写入路径。
//
// 滚动: Tauri 透明无框窗口中 native scroll 经常失效,
//       用 wheel 事件手动 scrollTop 兜底 + 动态调整窗口高度。

(function() {
  'use strict';

  const HIDE_AFTER_MS = 15000;
  const POLL_INTERVAL_MS = 120;
  const MIN_H = 140;
  const MAX_H = 340;
  const MIN_W = 240;
  const MAX_W = 420;
  const ABS_MAX_H = 680;      // 用户手动拖拽时的绝对最大高度
  const PADDING_TOTAL = 60;   // body top(6) + bubble padding-top(14) + padding-bottom(14) + body bottom(22) + 余量(4)
  const INPUT_ROW_H = 42;     // input-row 额外高度（含 padding-top:8）
  // 防御性清理：连续 N 次 poll 文本长度无变化 → 视为流结束（兜底 Tauri bubble-end 事件丢失）
  // 8 * 120ms ≈ 960ms，略大于正常 LLM token 间隔
  const STABLE_TICKS_THRESHOLD = 8;

  let hideTimer = null;
  let contentEl = null;
  let bodyEl = null;           // #contentBody：唯一被 innerHTML 覆盖的节点
  let cursorEl = null;         // #typingCursor：常驻 DOM，仅通过 content 的 streaming class 控制
  let pollTimer = null;
  let currentWinH = MIN_H;
  let currentWinW = 280;
  let lastRawText = '';       // 记录最近一次原始文本，用于最终渲染去光标
  let inputRowEl = null;
  let inputEl = null;
  let sendBtnEl = null;
  let resizeGripEl = null;      // resize 手柄元素
  let isComposing = false;    // IME 组合状态标记
  let userScrolledUp = false;  // 用户是否手动向上滚动了（锁定自动跟底）
  let streaming = false;        // 是否处于流式输出中（bubble-end 后为 false 拦截迟到的轮询）
  let lastPollLen = -1;         // 上一次 poll 到的文本长度（用于稳定检测）
  let stableTicks = 0;          // 连续无变化的 tick 计数

  // ---- Resize 状态 ----
  let resizeMode = 'auto';      // 'auto' | 'manual'
  let userPrefSize = null;      // { w, h } 用户手动设定的偏好尺寸

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
      resizeMode = 'manual';
      var win = window.__TAURI__.window.getCurrentWindow();
      win.setSize(new window.__TAURI__.window.LogicalSize(
        Math.max(MIN_W, Math.min(MAX_W, userPrefSize.w)),
        Math.max(MIN_H, Math.min(ABS_MAX_H, userPrefSize.h))
      )).then(function() {
        currentWinW = userPrefSize.w;
        currentWinH = userPrefSize.h;
        document.documentElement.style.width = userPrefSize.w + 'px';
        document.body.style.width = userPrefSize.w + 'px';
      }).catch(function() {});
    }
    document.body.classList.add('show');
  }

  function clearHideTimer() {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
  }

  function startHideTimer() {
    clearHideTimer();
    hideTimer = setTimeout(hide, HIDE_AFTER_MS);
  }

  /// 根据实际渲染高度动态调整窗口高度
  /// MANUAL 模式下：不收缩到用户设定以下，但内容多时仍可扩展
  function autoResize() {
    if (!contentEl) return;
    var contentH = contentEl.scrollHeight;
    var inputExtra = (inputRowEl && inputRowEl.style.display !== 'none') ? INPUT_ROW_H : 0;
    var neededH = Math.min(MAX_H, Math.max(MIN_H, contentH + PADDING_TOTAL + inputExtra));

    // MANUAL 模式：以用户偏好为下界，绝对最大高度也放宽
    var targetW = 280;
    if (resizeMode === 'manual' && userPrefSize) {
      neededH = Math.max(neededH, userPrefSize.h);
      neededH = Math.min(ABS_MAX_H, neededH);
      targetW = Math.max(MIN_W, Math.min(MAX_W, userPrefSize.w));
    }

    var newH = Math.round(neededH);
    var sizeChanged = (newH !== currentWinH || targetW !== currentWinW);

    if (sizeChanged && window.__TAURI__ && window.__TAURI__.window) {
      var deltaH = newH - currentWinH;
      currentWinH = newH;
      currentWinW = targetW;
      var win = window.__TAURI__.window.getCurrentWindow();
      win.setSize(new window.__TAURI__.window.LogicalSize(targetW, newH))
        .then(function() {
          if (deltaH !== 0) {
            return win.outerPosition().then(function(pos) {
              return win.setPosition(
                new window.__TAURI__.window.LogicalPosition(pos.x, pos.y - deltaH)
              );
            });
          }
        })
        .catch(function() {});
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
    hideInput('hide-bubble'); // 隐藏时一并收起输入框
    // 隐藏时重置为 AUTO 模式 + 恢复默认尺寸（不清除 localStorage 偏好）
    resizeMode = 'auto';
    // 隐藏前恢复默认尺寸
    if (window.__TAURI__ && window.__TAURI__.window) {
      window.__TAURI__.window.getCurrentWindow()
        .setSize(new window.__TAURI__.window.LogicalSize(280, MIN_H))
        .catch(() => {});
      currentWinH = MIN_H;
      currentWinW = 280;
    }
    document.body.classList.remove('show');
    document.body.classList.add('hidden');
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core.invoke('cmd_hide_bubble').catch(() => {});
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
      // 通知 Rust 退出 chat 模式（让截图可以覆盖）
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('cmd_exit_chat').catch(function() {});
      }
    }, 280);
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
    streaming = true;       // 标记流式开始
    userScrolledUp = false; // 新流式开始，重置锁定
    // 🔧 不立即激活光标：等真正拉到非空文本再切 streaming，
    //    避免 bubble-end 事件丢失时光标常驻
    showThinking();         // 显示思考指示器
    lastPollLen = -1;
    stableTicks = 0;
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
    lastPollLen = -1;
    stableTicks = 0;
  }

  function pollPending() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return Promise.resolve('');
    return window.__TAURI__.core.invoke('cmd_consume_bubble_text')
      .then((result) => result || '')
      .catch(function() { return ''; });
  }

  /// 轮询回调：有新文本才渲染（流式模式）
  /// 同时做稳定检测：连续 STABLE_TICKS_THRESHOLD 次长度无变化 → 视为流式结束
  /// （兜底 Tauri bubble-end 事件丢失导致光标永不熄灭）
  function onPollResult(txt) {
    // 流已结束 → 丢弃迟到的轮询结果（clearInterval 无法取消已在途的 IPC）
    if (!streaming) return;

    const len = (txt || '').length;

    // 稳定检测：文本长度与上一次一致（且已有内容）→ 累加
    if (len === lastPollLen && len > 0) {
      stableTicks++;
      if (stableTicks >= STABLE_TICKS_THRESHOLD) {
        // 视为流式已结束（可能 bubble-end 丢了）→ 主动熄灭光标
        streaming = false;
        stopPolling();
        hideThinking();
        startHideTimer();
        showInput();
        return;
      }
    } else {
      stableTicks = 0;
      lastPollLen = len;
    }

    if (len === 0) return;

    setText(txt);
    ensureVisible();
    // 🔧 拉到首个非空文本后才激活光标（避免 init 即残留）
    if (contentEl && !contentEl.classList.contains('streaming')) {
      setStreamingClass(true);
    }
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
    cursorEl = document.getElementById('typingCursor');
    inputRowEl = document.getElementById('inputRow');
    inputEl = document.getElementById('chatInput');
    sendBtnEl = document.getElementById('chatSend');
    thinkingEl = document.getElementById('thinking');
    resizeGripEl = document.getElementById('resizeGrip');

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

    // ---- Resize Grip 交互 ----
    if (resizeGripEl && window.__TAURI__ && window.__TAURI__.window) {
      var win = window.__TAURI__.window.getCurrentWindow();

      // mousedown → 原生 resize 拖拽
      resizeGripEl.addEventListener('mousedown', function(e) {
        e.preventDefault();
        e.stopPropagation();
        resizeMode = 'manual';
        win.startResizeDragging('SouthEast').catch(function() {});
      });

      // 双击 grip → 回到 AUTO 模式
      resizeGripEl.addEventListener('dblclick', function(e) {
        e.preventDefault();
        e.stopPropagation();
        resizeMode = 'auto';
        userPrefSize = null;
        localStorage.removeItem('bubble_pref');
        diag('resize: double-click → reset to auto');
        autoResize();
      });

      // 窗口尺寸变化时更新 userPrefSize（拖拽中/拖拽后）
      win.onResized(function() {
        if (resizeMode === 'manual') {
          win.innerSize().then(function(size) {
            var w = Math.round(size.width);
            var h = Math.round(size.height);
            userPrefSize = { w: w, h: h };
            currentWinW = w;
            currentWinH = h;
            localStorage.setItem('bubble_pref', JSON.stringify(userPrefSize));
            // CSS 跟随宽度变化
            document.documentElement.style.width = w + 'px';
            document.body.style.width = w + 'px';
            diag('resize: manual size updated w=' + w + ' h=' + h);
          }).catch(function() {});
        }
      });
    }

    // 从 localStorage 恢复用户偏好
    try {
      var saved = JSON.parse(localStorage.getItem('bubble_pref') || 'null');
      if (saved && saved.w && saved.h) {
        userPrefSize = { w: saved.w, h: saved.h };
        // 不立即进入 manual 模式——等首次 show 时再应用
        diag('resize: restored pref from localStorage w=' + saved.w + ' h=' + saved.h);
      }
    } catch (e) { /* ignore parse errors */ }

    if (!window.__TAURI__) return;
    var listen = window.__TAURI__.event.listen;

    listen('bubble-end', () => {
      stopPolling();
      streaming = false;     // 立即标记结束，拦截后续迟到的 onPollResult
      userScrolledUp = false; // 流结束，解锁滚动
      hideThinking();
      setStreamingClass(false); // 显式收回光标（stopPolling 里已做，这里双保险）
      // lastRawText 已是最新（每次 setText 都更新了），直接同步最终渲染
      if (lastRawText) setText(lastRawText);
      startHideTimer();
      showInput();
    });

    listen('bubble-update', (event) => {
      stopPolling();
      streaming = false;     // 截图摘要非流式，标记结束
      setStreamingClass(false);
      var payload = event.payload || {};
      var text = typeof payload === 'string' ? payload : (payload.text || '');
      diag('bubble-update received, text_len=' + (text || '').length);
      setText(text);
      ensureVisible();
      startHideTimer();
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
  // emit_to 对 hide→show 窗口不可靠，Rust 端通过 eval 直接触发此函数拉取 pending_text
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
