import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const scriptPath = resolve(process.cwd(), 'js/agent_watch.js');
const script = readFileSync(scriptPath, 'utf8');

function createDom(invoke) {
  const dom = new JSDOM(`<!doctype html>
    <section id="agent-watch">
      <header id="watch-header" tabindex="0">
        <h1 id="watch-title"></h1>
        <div class="watch-actions"><span id="watch-count"></span></div>
      </header>
      <div id="stack"></div>
    </section>`, {
    url: 'http://localhost/agent_watch.html',
    runScripts: 'outside-only',
  });
  dom.window.__TAURI__ = invoke ? { core: { invoke } } : {};
  dom.window.localStorage.clear();
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

  it('builds compact row metadata from machine, source, kind, and status text', () => {
    const parts = dom.window.__agentWatchTest.lineParts({
      machine: 'qy113',
      project: '2605',
      source: 'Claude Code',
      kind: 'Patch',
      statusText: '正在修改文件',
    }, { status: 'working' });
    const values = parts.map((part) => part.value);

    expect(values).toEqual(['qy113', 'Claude', 'Patch', '正在修改文件']);
    expect(parts.map((part) => part.className)).toContain('task-device');
    expect(parts.map((part) => part.className)).toContain('task-source');
    expect(parts.map((part) => part.className)).toContain('task-status-text');
  });

  it('renders notification rows with title, meta, age, and hover dismiss affordance', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's1',
        source: 'codex',
        machine: 'qy113',
        workspace_name: 'TrumanWorld',
        status: 'working',
        display: {
          action_label: 'Shell',
          headline: 'Running tests',
          detail: 'cargo test',
          project: 'TrumanWorld',
          source_label: 'Codex',
          tone: 'active',
          age_label: '2s',
        },
        age_sec: 2,
      }],
    });

    const card = dom.window.document.querySelector('.task-card');

    expect(dom.window.document.getElementById('watch-title')?.textContent).toBe('Agent Watch 1');
    expect(card.querySelector('.task-title')?.textContent).toBe('TrumanWorld');
    expect(card.querySelector('.task-meta')?.textContent).toContain('qy113');
    expect(card.querySelector('.task-meta')?.textContent).toContain('Codex');
    expect(card.querySelector('.task-age')?.textContent).toBe('2s');
    expect(card.querySelectorAll('[data-action="dismiss"]')).toHaveLength(1);
    expect(card.querySelector('.task-detail')).toBeNull();
  });

  it('expands a task row to reveal detail and contextual actions', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's-detail',
        source: 'claude_code',
        machine: 'qy113',
        workspace_name: 'data',
        status: 'waiting',
        display: {
          action_label: 'Shell',
          headline: '等待确认',
          detail: '将执行 cargo test',
          project: 'data',
          source_label: 'Claude Code',
          tone: 'needs_user',
        },
      }],
    });

    const card = dom.window.document.querySelector('.task-card');
    expect(card.querySelector('.task-detail')).toBeNull();

    card.click();

    const expandedCard = dom.window.document.querySelector('.task-card');
    expect(expandedCard.classList.contains('expanded')).toBe(true);
    expect(expandedCard.getAttribute('aria-expanded')).toBe('true');
    expect(expandedCard.querySelector('.task-detail')?.textContent).toContain('将执行 cargo test');
    expect(expandedCard.querySelector('.task-meta')?.textContent).not.toContain('将执行 cargo test');
    expect(expandedCard.querySelector('[data-action="open"]')).toBeNull();
    expect(expandedCard.querySelector('.task-expanded-actions')).toBeNull();
  });

  it('dismisses a processed task through an action button', async () => {
    const calls = [];
    dom?.window?.close();
    dom = createDom(async (name, payload) => {
      calls.push({ name, payload });
      if (name === 'cmd_dismiss_agent_session') return { sessions: [] };
      return {};
    });
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's3',
        source: 'codex',
        workspace_name: '8bit',
        status: 'done',
        display: {
          action_label: 'Shell',
          project: '8bit',
          source_label: 'Codex',
          age_label: '17s',
          tone: 'done',
        },
      }],
    });

    dom.window.document.querySelector('[data-action="dismiss"]').click();
    await new Promise((resolve) => dom.window.setTimeout(resolve, 0));

    expect(calls).toContainEqual({
      name: 'cmd_dismiss_agent_session',
      payload: { sessionId: 's3' },
    });
    expect(dom.window.document.querySelector('.task-card')).toBeNull();
  });

  it('clicking the header folds the task list without dismissing sessions', async () => {
    const calls = [];
    dom?.window?.close();
    dom = createDom(async (name, payload) => {
      calls.push({ name, payload });
      return {};
    });
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's-header',
        source: 'codex',
        workspace_name: '8bit',
        status: 'working',
        display: { action_label: 'Patch', project: '8bit', source_label: 'Codex' },
      }],
    });

    dom.window.document.getElementById('watch-header').click();
    await new Promise((resolve) => dom.window.setTimeout(resolve, 0));

    expect(calls).toContainEqual({
      name: 'cmd_agent_watch_set_folded',
      payload: { folded: true },
    });
    expect(calls.some((call) => call.name === 'cmd_dismiss_agent_session')).toBe(false);
  });

  it('does not spell out completed detail inside collapsed done rows', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's2',
        source: 'codex',
        machine: '',
        workspace_name: '8bit',
        status: 'done',
        display: {
          action_label: 'Shell',
          headline: 'Done',
          detail: 'Task finished',
          project: '8bit',
          source_label: 'Codex',
          age_label: '17s',
          tone: 'done',
        },
      }],
    });

    const card = dom.window.document.querySelector('.task-card');

    expect(card.querySelector('.task-title')?.textContent).toBe('8bit');
    expect(card.querySelector('.task-meta')?.textContent).toContain('Shell');
    expect(card.textContent).not.toContain('Done');
    expect(card.textContent).not.toContain('Task finished');
  });

  it('keeps the CSS shaped like an island header with notification rows', () => {
    const sheet = readFileSync(resolve(process.cwd(), 'css/agent_watch.css'), 'utf8');

    expect(sheet).toContain('.watch-header {');
    expect(sheet).toContain('border-radius: 18px;');
    expect(sheet).toContain('.task-card.expanded');
    expect(sheet).toContain('overflow-y: auto;');
    expect(sheet).toContain('.watch-shell.folded .task-stack');
    expect(sheet).toContain('.task-card:hover .task-dismiss');
    expect(sheet).not.toContain('.task-expanded-actions');
    expect(sheet).not.toContain('.task-open');
    expect(sheet).not.toContain('.task-toggle');
  });

  it('keeps specific task context in the view model', () => {
    const view = dom.window.__agentWatchTest.viewOf({
      source: 'codex',
      machine: 'qy113',
      workspace_name: '8bit',
      status: 'working',
      status_label: 'Working',
      display: {
        action_label: 'Shell',
        headline: 'Run remote install self-check',
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
    expect(view.statusText).toBe('scripts/remote-install.sh');
  });
});
