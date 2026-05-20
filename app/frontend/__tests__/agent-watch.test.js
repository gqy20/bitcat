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

  it('renders cards as one-line rows with one dismiss button', () => {
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
    expect(card.querySelector('[data-action="toggle"]')).toBeNull();
    expect(card.querySelectorAll('[data-action="dismiss"]')).toHaveLength(1);
    expect(card.querySelector('.task-summary')).toBeNull();
    expect(card.querySelector('.task-separator')).toBeNull();
  });

  it('expands a task row to reveal full agent detail', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's-detail',
        source: 'claude_code',
        machine: 'qy113',
        workspace_name: 'data',
        status: 'waiting',
        display: {
          action_label: 'Shell',
          headline: 'Needs your decision',
          detail: 'Full command output and request context should be visible here.',
          project: 'data',
          source_label: 'Claude Code',
          tone: 'needs_user',
        },
      }],
    });

    const card = dom.window.document.querySelector('.task-card');
    expect(card.querySelector('.task-detail')).toBeNull();

    card.querySelector('.task-main').click();

    expect(card.classList.contains('expanded')).toBe(false);
    const expandedCard = dom.window.document.querySelector('.task-card');
    expect(expandedCard.classList.contains('expanded')).toBe(true);
    expect(expandedCard.getAttribute('aria-expanded')).toBe('true');
    expect(expandedCard.querySelector('.task-detail')?.textContent).toContain('Full command output');
  });

  it('dismisses a processed task through the row x button', async () => {
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

  it('keeps task rows compact and reserves only dismiss action space', () => {
    const sheet = readFileSync(resolve(process.cwd(), 'css/agent_watch.css'), 'utf8');

    expect(sheet).toContain('.task-card:not(.collapsed) {');
    expect(sheet).toContain('grid-template-columns: 4px minmax(0, 1fr) 26px;');
    expect(sheet).toContain('height: 46px;');
    expect(sheet).toContain('max-height: 46px;');
    expect(sheet).toContain('overflow: hidden;');
    expect(sheet).toContain('padding: 0 2px 10px 0;');
    expect(sheet).not.toContain('.task-open');
    expect(sheet).not.toContain('.task-toggle');
    expect(sheet).toContain('.task-dismiss');
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
