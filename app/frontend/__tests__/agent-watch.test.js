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

  it('renders compact metadata as prioritized context instead of breadcrumbs', () => {
    const html = dom.window.__agentWatchTest.renderCompactMeta({
      machine: 'qy113',
      project: 'TrumanWorld',
      source: 'Claude Code',
      kind: 'Shell',
    }, { status: 'working' });

    expect(html).toContain('task-context-primary');
    expect(html).toContain('task-context-secondary');
    expect(html).toContain('qy113');
    expect(html).toContain('TrumanWorld');
    expect(html).toContain('Claude');
    expect(html).toContain('Shell');
    expect(html).not.toContain('task-separator');
    expect(html).not.toContain('qy...');
  });

  it('renders collapsed cards with readable primary context', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's1',
        source: 'codex',
        machine: 'qy113',
        workspace_name: 'TrumanWorld',
        status: 'working',
        display: {
          action_label: 'Shell',
          headline: '运行测试',
          detail: 'cargo test',
          project: 'TrumanWorld',
          source_label: 'Codex',
          tone: 'active',
        },
        age_sec: 2,
      }],
    });

    dom.window.document.querySelector('.task-card [data-action="toggle"]').click();
    const card = dom.window.document.querySelector('.task-card');

    expect(card.classList.contains('collapsed')).toBe(true);
    expect(card.querySelector('.task-context-primary')?.textContent).toContain('qy113');
    expect(card.querySelector('.task-context-primary')?.textContent).toContain('TrumanWorld');
    expect(card.querySelector('.task-separator')).toBeNull();
  });

  it('renders expanded cards without repeating command metadata', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's2',
        source: 'codex',
        machine: 'qy113',
        workspace_name: 'TrumanWorld',
        status: 'tool_running',
        display: {
          action_label: 'Shell',
          headline: "正在运行 sed -n '1,260p' frontend/components/scene-style.ts",
          detail: "sed -n '1,260p' frontend/components/scene-style.ts",
          project: 'TrumanWorld',
          source_label: 'Codex',
          age_label: '13s',
          tone: 'active',
        },
      }],
    });

    const card = dom.window.document.querySelector('.task-card');

    expect(card.classList.contains('collapsed')).toBe(false);
    expect(card.querySelector('.task-title')?.textContent).toBe('正在运行 Shell');
    expect(card.querySelector('.task-summary')?.textContent).toContain("sed -n '1,260p'");
    expect(card.querySelector('.task-context-primary')?.textContent).toContain('qy113');
    expect(card.querySelector('.task-context-primary')?.textContent).toContain('TrumanWorld');
    expect(card.querySelector('.task-context-secondary')?.textContent).toContain('Codex');
    expect(card.querySelector('.task-separator')).toBeNull();
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
