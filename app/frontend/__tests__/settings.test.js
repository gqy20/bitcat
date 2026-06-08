import { describe, expect, it } from 'vitest';
import { JSDOM } from 'jsdom';
import fs from 'node:fs';
import { resolve } from 'node:path';

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

    expect(
      helpers.formatReminderSchedule({
        schedule_label: '一次 · 2026-06-02T19:45:17+08:00',
        next_fire_at: '2026-06-02T19:45:17+08:00',
      }),
    ).toBe('一次');
  });

  it('does not use schedule text as the card description', () => {
    const { helpers } = loadSettings();

    expect(
      helpers.reminderDescription({
        message: null,
        schedule_label: '一次 · 2026-06-02T19:45:17+08:00',
      }),
    ).toBe('');
    expect(
      helpers.reminderDescription({
        message: '  茶叶放温再喝  ',
        schedule_label: '一次 · 2026-06-02T19:45:17+08:00',
      }),
    ).toBe('茶叶放温再喝');
  });

  it('formats embedded RFC3339 times without seconds or timezone noise', () => {
    const { helpers } = loadSettings();

    expect(
      helpers.formatReminderSchedule({
        schedule_label: '每天 · 2026-06-02T09:30:17+08:00',
      }),
    ).toBe('每天 · 06/02 09:30');
  });

  it('renders one-shot reminders without a duplicate schedule description', () => {
    const { dom, helpers } = loadSettings(`
      <div id="reminder-status"></div>
      <div id="reminder-review"></div>
    `);

    helpers.renderReminders({
      generated_at: '2026-06-02T20:00:00+08:00',
      active_count: 1,
      total_entries: 1,
      store_path: '',
      events_path: '',
      entries: [{
        id: 'rem_test',
        title: '泡茶',
        message: null,
        status: 'active',
        schedule_label: '一次 · 2026-06-02T19:45:17+08:00',
        next_fire_at: '2026-06-02T19:45:17+08:00',
        last_fired_at: null,
        fire_count: 0,
      }],
    });

    const card = dom.window.document.querySelector('.reminder-entry');
    expect(card?.querySelector('p')).toBeNull();
    expect(card?.textContent).toContain('一次');
    expect(card?.textContent).toContain('下次 06/02 19:45');
    expect(card?.textContent).not.toContain('+08:00');
    expect(card?.textContent).not.toContain('2026-06-02T19:45:17');
  });
});
