// 前端全局错误守卫（L4）：把 WebView 内未捕获的异常落到 Rust 侧 app.log。
// 所有窗口在 <head> 最先引入本文件；诊断包（diagnostics.rs）会把这些错误
// 带回来，弥补"前端 console 不落盘"导致的渲染问题无据可查。
(function () {
  'use strict';
  var invoke = window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke;
  var label =
    (window.__TAURI__ && window.__TAURI__.window && window.__TAURI__.window.getCurrentWindow &&
      (function () {
        try { return window.__TAURI__.window.getCurrentWindow().label; } catch (_) { return 'unknown'; }
      })()) ||
    'unknown';

  function report(kind, message, source) {
    if (!invoke) return;
    try {
      invoke('cmd_frontend_error', {
        windowLabel: label,
        kind: kind,
        message: String(message).slice(0, 500),
        source: String(source || '').slice(0, 300),
      }).catch(function () {});
    } catch (_) { /* 守卫自身永不抛错 */ }
  }

  window.addEventListener('error', function (event) {
    if (event.error || event.message) {
      var source = event.filename ? (event.filename + ':' + (event.lineno || 0)) : '';
      report('error', (event.message || String(event.error)) + (event.error && event.error.stack ? '\n' + String(event.error.stack).slice(0, 400) : ''), source);
    }
  });

  window.addEventListener('unhandledrejection', function (event) {
    var reason = event.reason;
    report('unhandledrejection', reason && reason.message ? reason.message : String(reason), reason && reason.stack ? String(reason.stack).slice(0, 300) : '');
  });
})();
