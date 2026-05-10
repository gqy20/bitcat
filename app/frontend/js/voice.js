// voice.js — 可见的录音条 textarea，接收任意输入法注入的文字
//
// 后端取文本方式: eval 直接在 WebView2 中执行 invoke + 清空（不依赖事件握手）
// 前端仍监听 input 事件实时 pushText 作为兜底

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
    // 这些 pushText 作为 eval 取值的实时兜底（IME 注入过程中就同步文本）
    ['input', 'compositionend', 'keyup', 'change'].forEach((ev) => {
      ta.addEventListener(ev, pushText);
    });

    if (window.__TAURI__ && window.__TAURI__.event) {
      // 后端 open_voice_capture 时发此事件 → 清空 textarea 准备接收新语音
      window.__TAURI__.event.listen('voice-clear', () => {
        ta.value = '';
        pushText();
        refocus();
      });

      window.__TAURI__.event.listen('voice-focus', () => {
        refocus();
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
