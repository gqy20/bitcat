// particles.js — 粒子效果（happy 心、sleep Z、confused ?）

(function() {
  'use strict';

  const SLEEP_INTERVAL_MS = 1200;
  const CONFUSED_INTERVAL_MS = 850;

  let sleepCounter = 0;
  let confusedCounter = 0;

  function getContainer() {
    return document.getElementById('particles');
  }

  function spawn(text, className, x, y) {
    const container = getContainer();
    if (!container) return;
    const el = document.createElement('span');
    el.textContent = text;
    el.className = `particle ${className}`;
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
    container.appendChild(el);
    // 动画结束后自动清理
    el.addEventListener('animationend', () => el.remove(), { once: true });
    // 兜底：1.5×animation-duration 仍在 → 强制清理
    setTimeout(() => {
      if (el.parentNode) el.remove();
    }, 2500);
  }

  // 进入新状态时一次性触发（happy 喷心）
  function onStateEnter(state) {
    if (state === 'happy') {
      // 喷 5 颗心，错开生成时间
      for (let i = 0; i < 5; i++) {
        const dx = (Math.random() - 0.5) * 56;
        const delay = i * 70;
        setTimeout(() => spawn('❤', 'p-heart', 64 + dx - 7, 28), delay);
      }
    }
    // 切到非 sleep/confused 时计数器清零
    if (state !== 'sleep')    sleepCounter = 0;
    if (state !== 'confused') confusedCounter = 0;
  }

  // 每帧调用（持续粒子：sleep Z、confused ?）
  function tick(state, dtMs) {
    if (state === 'sleep') {
      sleepCounter += dtMs;
      if (sleepCounter >= SLEEP_INTERVAL_MS) {
        sleepCounter = 0;
        spawn('Z', 'p-zzz', 78, 32);
      }
    }
    if (state === 'confused') {
      confusedCounter += dtMs;
      if (confusedCounter >= CONFUSED_INTERVAL_MS) {
        confusedCounter = 0;
        spawn('?', 'p-confused', 60, 24);
      }
    }
  }

  // 测试函数（test.html 调用）
  function runParticleTests() {
    const results = [];
    function assert(name, cond) { results.push({ name, pass: !!cond }); }

    // 测：onStateEnter 'idle' 不应生成粒子
    onStateEnter('idle');
    assert('idle_no_spawn', document.querySelectorAll('.p-heart').length === 0);

    // tick 测：sleep 状态 1200ms 后生成 1 个 Z
    sleepCounter = 0;
    tick('sleep', 1199);
    const before = document.querySelectorAll('.p-zzz').length;
    tick('sleep', 1);
    const after = document.querySelectorAll('.p-zzz').length;
    assert('sleep_spawns_zzz_at_1200ms', after === before + 1);

    // confused tick
    confusedCounter = 0;
    tick('confused', 849);
    const beforeQ = document.querySelectorAll('.p-confused').length;
    tick('confused', 1);
    const afterQ = document.querySelectorAll('.p-confused').length;
    assert('confused_spawns_q_at_850ms', afterQ === beforeQ + 1);

    // 状态切换重置 sleep 计数器
    sleepCounter = 999;
    onStateEnter('idle');
    assert('state_change_resets_sleep_counter', sleepCounter === 0);

    // 状态切换重置 confused 计数器
    confusedCounter = 999;
    onStateEnter('idle');
    assert('state_change_resets_confused_counter', confusedCounter === 0);

    // 非 sleep/confused 状态不累积
    sleepCounter = 0;
    tick('idle', 5000);
    assert('idle_does_not_spawn_zzz', sleepCounter === 0);

    return results;
  }

  if (typeof window !== 'undefined') {
    window.Particles = { onStateEnter, tick, runParticleTests };
  }
})();
