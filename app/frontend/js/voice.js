// voice.js — 可见的录音条 textarea，接收任意输入法注入的文字
// 关键: IME composition 过程中 input 事件可能延迟,需要 input + compositionend 双监听
// 后端取文本前会 emit voice-flush 强制把当前 ta.value 上报一次

(function() {
  'use strict';

  let ta = null;

  function refocus() {
    if (ta) {
      ta.focus();
      const len = ta.value.length;
      ta.setSelectionRange(len, len);
    }
  }

  function pushText() {
    if (!ta) return Promise.resolve();
    if (window.__TAURI__ && window.__TAURI__.core) {
      return window.__TAURI__.core.invoke('cmd_voice_update_text', { text: ta.value }).catch((e) => {
        console.error('[voice] invoke 失败:', e);
      });
    } else {
      console.warn('[voice] __TAURI__.core 不可用,无法上报文本');
      return Promise.resolve();
    }
  }

  function init() {
    ta = document.getElementById('vox');
    if (!ta) {
      console.error('[voice] 找不到 #vox textarea');
      return;
    }
    refocus();

    ta.addEventListener('blur', () => {
      setTimeout(refocus, 50);
    });

    // 多事件监听: 不同输入法/不同合成阶段触发的事件不一样
    ['input', 'compositionend', 'keyup', 'change'].forEach((ev) => {
      ta.addEventListener(ev, pushText);
    });

    if (window.__TAURI__ && window.__TAURI__.event) {
      window.__TAURI__.event.listen('voice-clear', () => {
        ta.value = '';
        pushText();
        refocus();
      });

      window.__TAURI__.event.listen('voice-focus', () => {
        refocus();
      });

      // 后端取文本前广播: 前端 invoke 上报 → 完成后再 emit voice-ready 通知后端
      window.__TAURI__.event.listen('voice-flush', () => {
        pushText().then(() => {
          if (window.__TAURI__ && window.__TAURI__.event) {
            window.__TAURI__.event.emit('voice-ready');
          }
        });
      });
    } else {
      console.warn('[voice] __TAURI__.event 不可用');
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
