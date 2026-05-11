// glow.js — 监听 pet 窗口发来的边缘切换事件，动态切换 right class

(function() {
  'use strict';

  function init() {
    if (!window.__TAURI__) return;
    window.__TAURI__.event.listen('glow-edge', (event) => {
      const edge = event.payload;
      if (edge === 'right') {
        document.body.classList.add('right');
      } else {
        document.body.classList.remove('right');
      }
    });
  }

  document.addEventListener('DOMContentLoaded', init);
})();
