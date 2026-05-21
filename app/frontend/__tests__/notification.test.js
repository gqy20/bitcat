import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const script = readFileSync(resolve(process.cwd(), 'js/notification.js'), 'utf8');

function createDom(invoke = vi.fn().mockResolvedValue(undefined)) {
  const dom = new JSDOM(`<!doctype html>
    <main id="notification" class="notification hidden">
      <div class="notification-dot"></div>
      <section class="notification-copy">
        <div class="notification-title" id="notificationTitle"></div>
        <div class="notification-body" id="notificationBody"></div>
        <div class="notification-actions" id="notificationActions"></div>
      </section>
    </main>`, {
    url: 'http://localhost/notification.html',
    runScripts: 'outside-only',
  });
  dom.window.__TAURI__ = {
    core: { invoke },
    event: { listen: vi.fn().mockResolvedValue(() => {}) },
  };
  dom.window.setTimeout = (fn) => {
    dom.window.__lastTimeout = fn;
    return 1;
  };
  dom.window.clearTimeout = vi.fn();
  dom.window.eval(script);
  return { dom, invoke };
}

describe('notification island', () => {
  let dom;
  let invoke;

  beforeEach(() => {
    ({ dom, invoke } = createDom());
  });

  afterEach(() => {
    dom?.window?.close();
  });

  it('renders reminder payload with calm expanded actions', () => {
    dom.window.__notificationShow({
      title: '喝水',
      body: '该喝水了',
      tone: 'warning',
      ttl_ms: 12000,
      reminder_id: 'rem_1',
      actions: [{ id: 'snooze_10', label: '10 分钟后' }],
    });

    const root = dom.window.document.getElementById('notification');
    expect(root.classList.contains('hidden')).toBe(false);
    expect(root.classList.contains('expanded')).toBe(true);
    expect(root.classList.contains('tone-warning')).toBe(true);
    expect(dom.window.document.getElementById('notificationTitle').textContent).toBe('喝水');
    expect(dom.window.document.querySelector('.notification-action').textContent).toBe('10 分钟后');
  });

  it('sends reminder action through ipc and fades locally', () => {
    dom.window.__notificationShow({
      title: '喝水',
      tone: 'warning',
      reminder_id: 'rem_1',
      actions: [{ id: 'complete', label: '完成' }],
    });
    dom.window.document.querySelector('.notification-action').click();

    expect(invoke).toHaveBeenCalledWith('cmd_notification_action', {
      action: 'complete',
      reminderId: 'rem_1',
    });
    expect(dom.window.document.getElementById('notification').classList.contains('hidden')).toBe(true);
  });
});
