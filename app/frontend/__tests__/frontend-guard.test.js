import { describe, it, expect, beforeEach } from 'vitest';
import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function loadGuard() {
  const dom = new JSDOM('<!doctype html><body></body>', {
    url: 'http://localhost/panel.html',
    runScripts: 'outside-only',
  });
  const calls = [];
  dom.window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        if (cmd === 'cmd_frontend_error') calls.push(args);
        return null;
      },
    },
    window: {
      getCurrentWindow: () => ({ label: 'test-window' }),
    },
  };
  const script = readFileSync(resolve(process.cwd(), 'js/frontend_guard.js'), 'utf8');
  dom.window.eval(script);
  return { dom, calls };
}

describe('frontend guard', () => {
  let dom;
  let calls;

  beforeEach(() => {
    ({ dom, calls } = loadGuard());
  });

  it('captures uncaught errors with window label', async () => {
    dom.window.dispatchEvent(
      new dom.window.ErrorEvent('error', {
        message: 'boom is not defined',
        filename: 'http://localhost/js/app.js',
        lineno: 42,
      }),
    );
    await new Promise((r) => setTimeout(r, 10));
    expect(calls).toHaveLength(1);
    expect(calls[0].windowLabel).toBe('test-window');
    expect(calls[0].kind).toBe('error');
    expect(calls[0].message).toContain('boom');
    expect(calls[0].source).toContain('app.js');
  });

  it('captures unhandled rejections', async () => {
    dom.window.dispatchEvent(
      new dom.window.Event('unhandledrejection'),
    );
    // jsdom 的事件没有 reason，守卫应安全处理 undefined。
    await new Promise((r) => setTimeout(r, 10));
    expect(calls).toHaveLength(1);
    expect(calls[0].kind).toBe('unhandledrejection');
  });

  it('never throws when invoke is unavailable', () => {
    const bare = new JSDOM('<!doctype html><body></body>', {
      runScripts: 'outside-only',
    });
    bare.window.__TAURI__ = { core: {} };
    expect(() => {
      bare.window.dispatchEvent(
        new bare.window.ErrorEvent('error', { message: 'x' }),
      );
    }).not.toThrow();
  });
});
