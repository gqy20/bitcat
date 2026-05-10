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
  const MIN_H = 140;          // 最小窗口高度
  const MAX_H = 340;          // 最大窗口高度（超出后内部滚动）
  const LINE_H = 13 * 1.55;   // 单行约 20px（font-size 13px × line-height 1.55）
  const PADDING_V = 22;       // .bubble padding(10×2) + 上下间距(~12)

  let hideTimer = null;
  let contentEl = null;
  let pollTimer = null;
  let currentWinH = MIN_H;

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

  /// 根据文本行数动态调整窗口高度，避免长回答被截断或无法滚动
  function autoResize(text) {
    if (!contentEl || !text) return;

    var lines = Math.ceil(text.length / 28);
    var neededH = Math.min(MAX_H, Math.max(MIN_H, lines * LINE_H + PADDING_V));
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

  function setText(text) {
    if (!contentEl) return;
    contentEl.textContent = text || '';
    contentEl.scrollTop = contentEl.scrollHeight;
    autoResize(text);
  }

  function hide() {
    stopPolling();
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

  function startPolling() {
    stopPolling();
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
          setText(txt);
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
  }

  /// 键盘滚动兜底：方向键/PageDown/PageUp/Home/End 控制滚动
  function onKeyDown(e) {
    if (!contentEl) return;
    var step = 40;
    switch (e.key) {
      case 'ArrowDown':
        contentEl.scrollTop += step; e.preventDefault(); break;
      case 'ArrowUp':
        contentEl.scrollTop -= step; e.preventDefault(); break;
      case 'PageDown':
        contentEl.scrollTop += contentEl.clientHeight; e.preventDefault(); break;
      case 'PageUp':
        contentEl.scrollTop -= contentEl.clientHeight; e.preventDefault(); break;
      case 'Home':
        contentEl.scrollTop = 0; e.preventDefault(); break;
      case 'End':
        contentEl.scrollTop = contentEl.scrollHeight; e.preventDefault(); break;
    }
  }

  function init() {
    contentEl = document.getElementById('content');
    if (!contentEl) return;

    // 滚轮兜底：监听 content 和 bubble 容器的 wheel 事件
    contentEl.addEventListener('wheel', onWheel, { passive: false });
    var bubbleEl = contentEl.closest('.bubble');
    if (bubbleEl) {
      bubbleEl.addEventListener('wheel', onWheel, { passive: false });
    }

    // 键盘滚动：使 content 可聚焦，监听方向键/Page/Home/End
    contentEl.setAttribute('tabindex', '0');
    contentEl.addEventListener('keydown', onKeyDown);

    if (!window.__TAURI__) return;
    var listen = window.__TAURI__.event.listen;

    listen('bubble-end', () => {
      stopPolling();
      pollPending();
      startHideTimer();
    });

    listen('bubble-update', (event) => {
      stopPolling();
      var payload = event.payload || {};
      var text = typeof payload === 'string' ? payload : (payload.text || '');
      setText(text);
      ensureVisible();
      startHideTimer();
    });

    startPolling();
  }

  document.addEventListener('DOMContentLoaded', init);
})();
