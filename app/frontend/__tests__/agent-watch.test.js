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

  it('builds one-line task metadata from machine, project, source, and kind', () => {
    const parts = dom.window.__agentWatchTest.lineParts({
      machine: 'qy113',
      project: '2605',
      source: 'Claude Code',
      kind: 'Patch',
    }, { status: 'working' }, false);
    const values = parts.map((part) => part.value);

    expect(values).toEqual(['qy113', '2605', 'Claude', 'Patch']);
    expect(parts.map((part) => part.className)).toContain('task-device');
    expect(parts.map((part) => part.className)).toContain('task-project');
    expect(parts.map((part) => part.className)).toContain('task-source');
  });

  it('renders cards as one-line rows without open buttons', () => {
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

    const card = dom.window.document.querySelector('.task-card');

    expect(card.querySelector('.task-meta')?.textContent).toContain('qy113');
    expect(card.querySelector('.task-meta')?.textContent).toContain('TrumanWorld');
    expect(card.querySelector('[data-action="open"]')).toBeNull();
    expect(card.querySelector('.task-summary')).toBeNull();
    expect(card.querySelector('.task-separator')).toBeNull();
  });

  it('does not spell out completed state inside done rows', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's2',
        source: 'codex',
        machine: '',
        workspace_name: '8bit',
        status: 'done',
        display: {
          action_label: 'Shell',
          headline: '已完成',
          detail: '任务已完成',
          project: '8bit',
          source_label: 'Codex',
          age_label: '17s',
          tone: 'done',
        },
      }],
    });

    const card = dom.window.document.querySelector('.task-card');

    expect(card.querySelector('.task-meta')?.textContent).toContain('8bit');
    expect(card.querySelector('.task-meta')?.textContent).toContain('Shell');
    expect(card.textContent).not.toContain('已完成');
  });

  it('keeps task rows compact and reserves only toggle action space', () => {
    const sheet = readFileSync(resolve(process.cwd(), 'css/agent_watch.css'), 'utf8');

    expect(sheet).toContain('.task-card:not(.collapsed) {');
    expect(sheet).toContain('grid-template-columns: 4px minmax(0, 1fr) 26px;');
    expect(sheet).toContain('height: 46px;');
    expect(sheet).toContain('max-height: 46px;');
    expect(sheet).toContain('overflow: hidden;');
    expect(sheet).toContain('padding: 0 2px 10px 0;');
    expect(sheet).not.toContain('.task-open');
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
