// voice.js - visible recording textarea for IME/text injection fallback.

(function() {
  'use strict';

  let ta = null;

  function log(msg, error) {
    const text = error ? `[voice] ${msg}: ${error.message || error}` : `[voice] ${msg}`;
    if (window.__TAURI__ && window.__TAURI__.core) {
      window.__TAURI__.core.invoke('cmd_pet_log', { msg: text }).catch(() => {});
    }
    console.warn(text);
  }

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
        log('invoke failed', e);
      });
    }
    log('__TAURI__.core unavailable; cannot report text');
    return Promise.resolve();
  }

  function init() {
    ta = document.getElementById('vox');
    if (!ta) {
      log('missing #vox textarea');
      return;
    }
    refocus();

    ta.addEventListener('blur', () => {
      setTimeout(refocus, 50);
    });

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
    } else {
      log('__TAURI__.event unavailable');
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
