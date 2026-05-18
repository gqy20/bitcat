import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const scriptPath = resolve(process.cwd(), 'js/agent_watch.js');
const script = readFileSync(scriptPath, 'utf8');

function createDom() {
  const dom = new JSDOM(`<!doctype html>
    <section id="agent-watch">
      <header id="watch-header" tabindex="0">
        <h1 id="watch-title"></h1>
        <div class="watch-actions">
          <button id="watch-expand-toggle" data-watch-action="toggle-all"></button>
        </div>
      </header>
      <span id="watch-count"></span>
      <div id="stack"></div>
    </section>`, {
    url: 'http://localhost/agent_watch.html',
    runScripts: 'outside-only',
  });
  dom.window.__TAURI__ = {};
  dom.window.setInterval = () => 0;
  dom.window.eval(script);
  return dom;
}

describe('agent watch metadata', () => {
  let dom;

  beforeEach(() => {
    dom = createDom();
  });

  afterEach(() => {
    dom?.window?.close();
  });

  it('renders remote machine before project and agent source', () => {
    const html = dom.window.__agentWatchTest.renderMeta({
      machine: 'qy113',
      project: '2605',
      source: 'Claude Code',
      kind: 'Patch',
    }, { status: 'working' }, false);

    expect(html.indexOf('qy113')).toBeLessThan(html.indexOf('2605'));
    expect(html.indexOf('2605')).toBeLessThan(html.indexOf('Claude Code'));
    expect(html).toContain('task-device');
    expect(html).toContain('task-project');
    expect(html).toContain('task-source');
  });

  it('keeps specific task context in the view model', () => {
    const view = dom.window.__agentWatchTest.viewOf({
      source: 'codex',
      machine: 'qy113',
      workspace_name: '8bit',
      status_label: 'Working',
      display: {
        action_label: 'Shell',
        headline: '运行远程安装自检',
        detail: 'scripts/remote-install.sh',
        project: '8bit',
      },
      age_sec: 12,
    });

    expect(view.machine).toBe('qy113');
    expect(view.project).toBe('8bit');
    expect(view.source).toBe('Codex');
    expect(view.kind).toBe('Shell');
    expect(view.detail).toBe('scripts/remote-install.sh');
  });
});
