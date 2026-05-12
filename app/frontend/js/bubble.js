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

  const HIDE_AFTER_MS = 5500;
  const POLL_INTERVAL_MS = 120;
  const MIN_H = 140;
  const MAX_H = 340;
  const PADDING_TOTAL = 48;   // body top(6) + bubble padding(20) + body bottom(22)
  const INPUT_ROW_H = 40;     // input-row 额外高度

  let hideTimer = null;
  let contentEl = null;
  let pollTimer = null;
  let currentWinH = MIN_H;
  let lastRawText = '';       // 记录最近一次原始文本，用于最终渲染去光标
  let inputRowEl = null;
  let inputEl = null;
  let sendBtnEl = null;
  let isComposing = false;    // IME 组合状态标记
  let userScrolledUp = false;  // 用户是否手动向上滚动了（锁定自动跟底）

  function ensureVisible() {
    document.body.classList.remove('hidden');
    void document.body.offsetWidth;
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
  function autoResize() {
    if (!contentEl) return;
    var contentH = contentEl.scrollHeight;
    var inputExtra = (inputRowEl && inputRowEl.style.display !== 'none') ? INPUT_ROW_H : 0;
    var neededH = Math.min(MAX_H, Math.max(MIN_H, contentH + PADDING_TOTAL + inputExtra));
    var newH = Math.round(neededH);

    if (newH !== currentWinH && window.__TAURI__ && window.__TAURI__.window) {
      var delta = newH - currentWinH;
      currentWinH = newH;
      var win = window.__TAURI__.window.getCurrentWindow();
      win.setSize(new window.__TAURI__.window.LogicalSize(280, newH))
        .then(function() {
          // 窗口变高了 → 整体上移，保持底部对齐宠物顶部
          if (delta !== 0) {
            return win.outerPosition().then(function(pos) {
              return win.setPosition(
                new window.__TAURI__.window.LogicalPosition(pos.x, pos.y - delta)
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

  function setText(text, streaming) {
    if (!contentEl) return;
    var wasAtBottom = isNearBottom();
    lastRawText = text || '';
    var src = streaming ? lastRawText + '█' : lastRawText;
    contentEl.innerHTML = typeof marked !== 'undefined'
      ? marked.parse(src) : src;
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
    hideInput(); // 隐藏时一并收起输入框
    // 隐藏前恢复默认高度
    if (currentWinH !== MIN_H && window.__TAURI__ && window.__TAURI__.window) {
      window.__TAURI__.window.getCurrentWindow()
        .setSize(new window.__TAURI__.window.LogicalSize(280, MIN_H))
        .catch(() => {});
      currentWinH = MIN_H;
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

  function resetInputIdleTimer() {
    if (inputIdleTimer) clearTimeout(inputIdleTimer);
    inputIdleTimer = setTimeout(function() {
      if (inputRowEl && inputRowEl.style.display !== 'none') {
        hideInput();
      }
    }, INPUT_IDLE_MS);
  }

  function showInput() {
    var diag = function(msg) {
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('cmd_pet_log', { msg: '[bubble] ' + msg }).catch(function() {});
      }
    };

    diag('showInput() called, inputRowEl=' + !!inputRowEl + ' inputEl=' + !!inputEl);
    if (!inputRowEl || !inputEl) { diag('ABORT: 元素不存在'); return; }

    inputRowEl.style.display = 'flex';
    inputRowEl.classList.add('visible');
    clearHideTimer();
    ensureVisible();
    requestAnimationFrame(function() {
      if (inputEl) inputEl.focus();
    });
    resetInputIdleTimer();
    autoResize();
    diag('showInput 完成');
  }

  function hideInput() {
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

  function toggleInput() {
    if (inputRowEl && inputRowEl.style.display === 'none') {
      showInput();
    } else {
      hideInput();
    }
  }

  function submitChat() {
    if (!inputEl || !window.__TAURI__ || !window.__TAURI__.core) return;
    var text = inputEl.value.trim();
    if (!text) return;
    inputEl.value = '';
    // 不隐藏输入框，等流式回复开始后自然覆盖 content 区
    resetInputIdleTimer();
    window.__TAURI__.core.invoke('cmd_submit_chat', { text: text })
      .catch(function(e) { console.error('[chat] submit failed:', e); });
  }

  function onInputKeyDown(e) {
    switch (e.key) {
      case 'Enter':
        if (!isComposing) { e.preventDefault(); submitChat(); }
        break;
      case 'Escape':
        e.preventDefault(); hideInput(); startHideTimer(); break;
    }
  }

  function onCompositionStart() { isComposing = true; }
  function onCompositionEnd() { isComposing = false; }

  function startPolling() {
    stopPolling();
    userScrolledUp = false; // 新流式开始，重置锁定
    pollTimer = setInterval(pollPending, POLL_INTERVAL_MS);
    pollPending();
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  function pollPending() {
    if (!window.__TAURI__ || !window.__TAURI__.core) return;
    window.__TAURI__.core.invoke('cmd_consume_bubble_text')
      .then((result) => {
        const txt = result || '';
        if (txt.length > 0) {
          setText(txt, true);
          ensureVisible();
        }
      })
      .catch(() => {});
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
    inputRowEl = document.getElementById('inputRow');
    inputEl = document.getElementById('chatInput');
    sendBtnEl = document.getElementById('chatSend');

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
      inputEl.addEventListener('input', function() { resetInputIdleTimer(); });
    }
    if (sendBtnEl) {
      sendBtnEl.addEventListener('click', submitChat);
    }

    if (!window.__TAURI__) return;
    var listen = window.__TAURI__.event.listen;

    listen('bubble-end', () => {
      stopPolling();
      pollPending();
      // 最终渲染：去掉打字光标
      if (lastRawText) setText(lastRawText, false);
      startHideTimer();
    });

    listen('bubble-update', (event) => {
      stopPolling();
      var payload = event.payload || {};
      var text = typeof payload === 'string' ? payload : (payload.text || '');
      setText(text, false);
      ensureVisible();
      startHideTimer();
    });

    // 双击宠物 / cmd_open_chat → 展开输入框
    listen('chat-open', () => {
      if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('cmd_pet_log', { msg: '[bubble] ✓ 收到 chat-open 事件' }).catch(function() {});
      }
      showInput();
    });

    startPolling();
  }

  document.addEventListener('DOMContentLoaded', init);

  // 暴露给 Rust eval 调用（cmd_open_chat 通过 window.eval 触发）
  window.__bubble_showInput = showInput;
  window.__bubble_hideInput = hideInput;
})();
