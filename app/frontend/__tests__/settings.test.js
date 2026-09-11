import { describe, expect, it } from 'vitest';
import { JSDOM } from 'jsdom';
import fs from 'node:fs';
import { resolve } from 'node:path';

// 用运行时本地时区把 Date 序列化成 RFC3339（带本地偏移）。
//
// 为什么需要它：formatDateTime 走 new Date(v).toLocaleString(...)，输出必然是
// “本地时区的墙上时间”。如果 fixture 写死 `+08:00`，在 TZ=UTC 的 CI runner 上
// 就会渲染成 01:30 而断言失败（只能靠在 CI 里钉 TZ 绕过）。让输入按本地时区
// 构造，期望值就能继续写死，测试也就与运行环境时区彻底解耦。
function localRfc3339(date) {
  const pad = (n) => String(n).padStart(2, '0');
  const offsetMin = -date.getTimezoneOffset(); // 东八区 → +480，UTC → 0，纽约 → -300
  const sign = offsetMin < 0 ? '-' : '+';
  const abs = Math.abs(offsetMin);
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}` +
    `${sign}${pad(Math.floor(abs / 60))}:${pad(abs % 60)}`
  );
}

function loadSettings(body = '') {
  const dom = new JSDOM(`<!doctype html><body>${body}</body>`, {
    url: 'http://localhost/settings.html',
    runScripts: 'outside-only',
  });
  const addEventListener = dom.window.document.addEventListener.bind(dom.window.document);
  dom.window.document.addEventListener = (type, listener, options) => {
    if (type === 'DOMContentLoaded') return;
    addEventListener(type, listener, options);
  };
  const script = fs.readFileSync(resolve(process.cwd(), 'js/settings.js'), 'utf8');
  dom.window.eval(script);
  return { dom, helpers: dom.window.__settingsTest };
}

describe('settings reminder formatting', () => {
  it('uses the in-app island confirmation instead of browser confirm', async () => {
    const { dom, helpers } = loadSettings(`
      <div id="confirm-layer" class="confirm-layer hidden" aria-hidden="true">
        <div class="confirm-island" role="dialog">
          <strong id="confirm-title"></strong>
          <span id="confirm-message"></span>
          <button id="confirm-cancel" type="button"></button>
          <button id="confirm-ok" type="button"></button>
        </div>
      </div>
    `);
    let browserConfirmCalled = false;
    dom.window.confirm = () => {
      browserConfirmCalled = true;
      return false;
    };

    const result = helpers.confirmDialog({
      title: '删除提醒',
      message: '这个提醒会被彻底删除。',
      okText: '删除',
    });
    expect(dom.window.document.getElementById('confirm-layer').classList.contains('hidden')).toBe(false);
    expect(dom.window.document.getElementById('confirm-title').textContent).toBe('删除提醒');
    dom.window.document.getElementById('confirm-ok').click();

    await expect(result).resolves.toBe(true);
    expect(browserConfirmCalled).toBe(false);
    expect(dom.window.document.getElementById('confirm-layer').classList.contains('hidden')).toBe(true);
  });

  it('keeps one-shot schedule compact and avoids repeating the fire time', () => {
    const { helpers } = loadSettings();
    const fireAt = localRfc3339(new Date(2026, 5, 2, 19, 45, 17));

    expect(
      helpers.formatReminderSchedule({
        schedule_label: `一次 · ${fireAt}`,
        next_fire_at: fireAt,
      }),
    ).toBe('一次');
  });

  it('does not use schedule text as the card description', () => {
    const { helpers } = loadSettings();
    const scheduleLabel = `一次 · ${localRfc3339(new Date(2026, 5, 2, 19, 45, 17))}`;

    expect(
      helpers.reminderDescription({
        message: null,
        schedule_label: scheduleLabel,
      }),
    ).toBe('');
    expect(
      helpers.reminderDescription({
        message: '  茶叶放温再喝  ',
        schedule_label: scheduleLabel,
      }),
    ).toBe('茶叶放温再喝');
  });

  it('formats embedded RFC3339 times without seconds or timezone noise', () => {
    const { helpers } = loadSettings();
    // 本地 06/02 09:30:17；偏移随运行环境时区变化，渲染结果不变
    const at = localRfc3339(new Date(2026, 5, 2, 9, 30, 17));

    const label = helpers.formatReminderSchedule({ schedule_label: `每天 · ${at}` });

    expect(label).toBe('每天 · 06/02 09:30');
    // 秒、原始 ISO 的 T、以及时区偏移都不应该出现在展示文案里
    expect(label).not.toContain(':17');
    expect(label).not.toContain('T');
    expect(label).not.toMatch(/[+-]\d{2}:?\d{2}/);
  });

  it('renders one-shot reminders without a duplicate schedule description', () => {
    const { dom, helpers } = loadSettings(`
      <div id="reminder-status"></div>
      <div id="reminder-review"></div>
    `);

    const fireAt = localRfc3339(new Date(2026, 5, 2, 19, 45, 17));

    helpers.renderReminders({
      generated_at: localRfc3339(new Date(2026, 5, 2, 20, 0, 0)),
      active_count: 1,
      total_entries: 1,
      store_path: '',
      events_path: '',
      entries: [{
        id: 'rem_test',
        title: '泡茶',
        message: null,
        status: 'active',
        schedule_label: `一次 · ${fireAt}`,
        next_fire_at: fireAt,
        last_fired_at: null,
        fire_count: 0,
      }],
    });

    const card = dom.window.document.querySelector('.reminder-entry');
    expect(card?.querySelector('p')).toBeNull();
    expect(card?.textContent).toContain('一次');
    expect(card?.textContent).toContain('下次 06/02 19:45');
    // 原始 RFC3339 的时区偏移、秒和 ISO 形式都不应该泄露到卡片上
    expect(card?.textContent).not.toMatch(/[+-]\d{2}:\d{2}/);
    expect(card?.textContent).not.toContain('2026-06-02T19:45:17');
  });
});

describe('onboarding wizard', () => {
  const WIZARD_HTML = `
    <div id="onboarding-wizard">
      <span data-dot="1"></span><span data-dot="2"></span><span data-dot="3"></span>
      <section class="wizard-step hidden" data-wizard-step="1"></section>
      <section class="wizard-step hidden" data-wizard-step="2"></section>
      <section class="wizard-step hidden" data-wizard-step="3"></section>
      <input id="wiz-screenshot" type="checkbox" />
      <input id="wiz-camera" type="checkbox" />
      <input id="wiz-tools" type="checkbox" />
    </div>
    <input id="perm-onboarding-completed" type="checkbox" />
    <input id="perm-steam-demo" type="checkbox" />
    <input id="perm-screenshot" type="checkbox" />
    <input id="perm-camera" type="checkbox" />
    <input id="perm-shell" type="checkbox" />
    <input id="perm-read-file" type="checkbox" />
    <input id="perm-clipboard" type="checkbox" />
    <input id="perm-foreground" type="checkbox" />
    <input id="perm-launch" type="checkbox" />
    <input id="perm-hotkey" type="checkbox" />
    <input id="perm-agent-remote" type="checkbox" />
    <input id="perm-diagnostics" type="checkbox" />
    <div id="perm-onboarding"><button id="perm-complete"></button></div>
    <strong id="perm-gate-title"></strong>
    <span id="perm-gate-summary"></span>
    <span id="perm-status-screenshot"></span>
    <span id="perm-status-camera"></span>
    <span id="perm-status-tools"></span>
    <span id="perm-status-remote"></span>
    <strong id="perm-tools-summary"></strong>
    <button id="perm-tools-jump"></button>
  `;

  it('steps forward and back with progress dots', () => {
    const { dom, helpers } = loadSettings(WIZARD_HTML);
    helpers.setWizardStep(2);
    const doc = dom.window.document;
    expect(doc.querySelector('[data-wizard-step="2"]').classList.contains('hidden')).toBe(false);
    expect(doc.querySelector('[data-wizard-step="1"]').classList.contains('hidden')).toBe(true);
    expect(doc.querySelector('[data-dot="1"]').classList.contains('done')).toBe(true);
    expect(doc.querySelector('[data-dot="2"]').classList.contains('active')).toBe(true);
    helpers.setWizardStep(0);
    expect(doc.querySelector('[data-wizard-step="1"]').classList.contains('hidden')).toBe(false);
  });

  it('finish merges wizard choices into existing permissions', async () => {
    const saved = [];
    const tauri = {
      core: {
        invoke: async (command, args) => {
          if (command === 'cmd_settings_save_permissions') saved.push(args.payload);
          return null;
        },
      },
    };
    const dom = new JSDOM(`<!doctype html><body>${WIZARD_HTML}<div id="toast"></div></body>`, {
      url: 'http://localhost/settings.html',
      runScripts: 'outside-only',
    });
    const addEventListener = dom.window.document.addEventListener.bind(dom.window.document);
    dom.window.document.addEventListener = (type, listener, options) => {
      if (type === 'DOMContentLoaded') return;
      addEventListener(type, listener, options);
    };
    dom.window.__TAURI__ = tauri;
    const script = fs.readFileSync(resolve(process.cwd(), 'js/settings.js'), 'utf8');
    dom.window.eval(script);
    const doc = dom.window.document;
    // 闭包内 SNAPSHOT 只能通过测试钩子写入（顶层 let 不在全局词法环境）。
    dom.window.__settingsTest.setSnapshot({
      permissions: { allow_agent_watch_remote: true, diagnostics_enabled: true },
    });
    doc.getElementById('wiz-screenshot').checked = true;
    doc.getElementById('wiz-tools').checked = true;
    doc.getElementById('wiz-camera').checked = false;
    await dom.window.__settingsTest.finishWizard();
    expect(saved).toHaveLength(1);
    const payload = saved[0];
    expect(payload.onboarding_completed).toBe(true);
    expect(payload.allow_screenshot_observation).toBe(true);
    expect(payload.allow_camera_observation).toBe(false);
    expect(payload.allow_shell_tool).toBe(true);
    expect(payload.allow_hotkey_tool).toBe(true);
    // 未在向导里出现的项保留原值。
    expect(payload.allow_agent_watch_remote).toBe(true);
    expect(payload.diagnostics_enabled).toBe(true);
    expect(doc.getElementById('onboarding-wizard').classList.contains('hidden')).toBe(true);
  });
});
