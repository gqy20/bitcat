// bubble.js — 独立气泡窗口前端
//
// 事件：
//   bubble-start   清空内容、显示窗口、取消旧定时
//   bubble-chunk   追加文本块（流式）
//   bubble-end     启动自动隐藏定时
//   bubble-update  一次性写入文本（兼容非流式路径）

(function() {
  'use strict';

  const HIDE_AFTER_MS = 5500;
  let hideTimer = null;
  let contentEl = null;

  function ensureVisible() {
    document.body.classList.remove('hidden');
    void document.body.offsetWidth;  // 强制 reflow
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

  function appendText(chunk) {
    if (!contentEl || !chunk) return;
    contentEl.textContent += chunk;
    // 自动滚到底,跟随生成
    contentEl.scrollTop = contentEl.scrollHeight;
  }

  function startStream() {
    setText('');
    ensureVisible();
    clearHideTimer();
  }

  function show(text) {
    setText(text);
    ensureVisible();
    startHideTimer();
  }

  function hide() {
    document.body.classList.remove('show');
    document.body.classList.add('hidden');
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core.invoke('cmd_hide_bubble').catch(() => {});
    }
  }

  function init() {
    contentEl = document.getElementById('content');
    if (!window.__TAURI__) return;
    const listen = window.__TAURI__.event.listen;

    // 流式事件
    listen('bubble-start', () => {
      startStream();
    });
    listen('bubble-chunk', (event) => {
      const payload = event.payload || {};
      const chunk = typeof payload === 'string' ? payload : (payload.chunk || '');
      ensureVisible();
      clearHideTimer();
      appendText(chunk);
    });
    listen('bubble-end', () => {
      startHideTimer();
    });

    // 兼容: 非流式一次性写入
    listen('bubble-update', (event) => {
      const payload = event.payload || {};
      const text = typeof payload === 'string' ? payload : (payload.text || '');
      show(text);
    });

    // 主动拉取后端 pending: 解决首次创建窗口时 emit 早于 listen 注册的时序
    if (window.__TAURI__.core) {
      window.__TAURI__.core.invoke('cmd_consume_bubble_text')
        .then((txt) => {
          if (txt && txt.length > 0) {
            // 有累积内容: 显示并视作流式中(等 bubble-end 触发隐藏)
            ensureVisible();
            setText(txt);
            // 不启动隐藏定时,等 bubble-end
          }
        })
        .catch(() => {});
    }
  }

  document.addEventListener('DOMContentLoaded', init);
})();
