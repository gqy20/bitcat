// bubble.js — 独立气泡窗口前端
//
// 策略: 前端通过定时轮询 cmd_consume_bubble_text 拉取后端累积的文本,
// 完全不依赖 bubble-chunk 事件的到达时序。
// bubble-end 事件仅用于知道流式何时结束(停止轮询 + 启动隐藏定时)。
// bubble-update 用于兼容非流式一次性写入路径。

(function() {
  'use strict';

  const HIDE_AFTER_MS = 5500;
  const POLL_INTERVAL_MS = 120;

  let hideTimer = null;
  let contentEl = null;
  let pollTimer = null;

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

  function setText(text) {
    if (!contentEl) return;
    contentEl.textContent = text || '';
    contentEl.scrollTop = contentEl.scrollHeight;
  }

  function hide() {
    stopPolling();
    document.body.classList.remove('show');
    document.body.classList.add('hidden');
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core.invoke('cmd_hide_bubble').catch(() => {});
    }
  }

  function startPolling() {
    stopPolling();
    pollTimer = setInterval(pollPending, POLL_INTERVAL_MS);
    // 立即拉一次
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
          setText(txt);
          ensureVisible();
        }
      })
      .catch(() => {});
  }

  function init() {
    contentEl = document.getElementById('content');
    if (!window.__TAURI__) return;
    const listen = window.__TAURI__.event.listen;

    // 流式结束: 停止轮询,最后一次拉取,启动隐藏定时
    listen('bubble-end', () => {
      stopPolling();
      pollPending();  // 确保拿到最终文本
      startHideTimer();
    });

    // 兼容: 非流式一次性写入
    listen('bubble-update', (event) => {
      stopPolling();
      const payload = event.payload || {};
      const text = typeof payload === 'string' ? payload : (payload.text || '');
      setText(text);
      ensureVisible();
      startHideTimer();
    });

    // 窗口显示后 JS 可能刚加载完,此时后端可能已经开始流式
    // 立即开始轮询拉取已有内容
    startPolling();
  }

  document.addEventListener('DOMContentLoaded', init);
})();
