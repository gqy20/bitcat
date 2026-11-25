import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const scriptPath = resolve(process.cwd(), 'js/agent_watch.js');
const script = readFileSync(scriptPath, 'utf8');
// agent_watch.js 依赖 agent_sources.js 提供的 window.AgentSources（与页面加载顺序一致）。
const sourcesScript = readFileSync(resolve(process.cwd(), 'js/agent_sources.js'), 'utf8');
const iconsScript = readFileSync(resolve(process.cwd(), 'js/agent_icons.js'), 'utf8');

function createDom(invoke) {
  const dom = new JSDOM(`<!doctype html>
    <section id="agent-watch">
      <header id="watch-header" tabindex="0">
        <h1 id="watch-title"></h1>
        <div class="watch-actions"><span id="watch-count"></span></div>
      </header>
      <div id="stack"></div>
      <div id="watch-tooltip" class="watch-tooltip"></div>
    </section>`, {
    url: 'http://localhost/agent_watch.html',
    runScripts: 'outside-only',
  });
  dom.window.__TAURI__ = invoke ? { core: { invoke } } : {};
  dom.window.localStorage.clear();
  dom.window.setInterval = () => 0;
  dom.window.eval(sourcesScript);
  dom.window.eval(iconsScript);
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

  it('summarizes header state with a symbol and count', () => {
    const summary = dom.window.__agentWatchTest.summaryState;
    expect(summary([{ status: 'working' }, { status: 'tool_running' }])).toMatchObject({
      symbol: '▶', count: 2, state: 'active',
    });
    expect(summary([{ status: 'waiting' }, { status: 'error' }])).toMatchObject({
      symbol: '×', count: 1, state: 'error',
    });
    expect(summary([{ status: 'done' }])).toMatchObject({
      symbol: '✓', count: 1, state: 'success',
    });
    expect(summary([])).toMatchObject({
      symbol: '–', count: 0, state: 'muted',
    });
  });

  it('labels all supported agent sources', () => {
    const label = dom.window.__agentWatchTest.agentSourceLabel;
    expect(label('claude_code')).toBe('Claude Code');
    expect(label('codex')).toBe('Codex');
    expect(label('pi')).toBe('pi');
    expect(label('opencode')).toBe('opencode');
    expect(label('zcode')).toBe('ZCode');
    expect(label('unknown-x')).toBe('unknown-x');
    expect(label(undefined)).toBe('Agent');
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

    expect(dom.window.document.getElementById('watch-title')?.textContent).toBe('Agent Watch');
    expect(dom.window.document.getElementById('watch-count')?.textContent).toBe('▶ 1');
    expect(dom.window.document.getElementById('watch-count')?.dataset.state).toBe('active');
    expect(card.querySelector('.task-title')?.textContent).toBe('TrumanWorld');
    expect(card.querySelector('.task-source-badge')?.textContent).toBe('CX');
    expect(card.querySelector('.task-device-badge')?.textContent).toBe('QY113');
    expect(card.querySelector('.task-icon svg')).not.toBeNull();
    expect(card.querySelector('.task-status-text')?.textContent).toBe('Running tests');
    expect(card.querySelector('.task-action-preview')?.textContent).toBe('cargo test');
    expect(card.querySelector('.task-age')?.textContent).toBe('2s');
    expect(card.querySelectorAll('[data-action="dismiss"]')).toHaveLength(1);
    expect(card.querySelector('.task-detail')).toBeNull();
  });

  it('shows the full action preview in a tooltip on hover', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's-tooltip',
        source: 'codex',
        workspace_name: 'bitcat',
        status: 'tool_running',
        display: {
          action_label: 'Shell',
          headline: '执行',
          detail: 'cargo test --release --all-features',
          project: 'bitcat',
          source_label: 'Codex',
          tone: 'active',
        },
      }],
    });
    const preview = dom.window.document.querySelector('.task-action-preview');
    preview.dispatchEvent(new dom.window.Event('pointerover', { bubbles: true }));

    const tooltip = dom.window.document.getElementById('watch-tooltip');
    expect(tooltip.textContent).toBe('cargo test --release --all-features');
    expect(tooltip.classList.contains('visible')).toBe(true);
  });

  it('updates an existing card in place when only its status changes', () => {
    const base = {
      session_id: 's-stable',
      source: 'claude_code',
      workspace_name: 'bitcat',
      display: {
        action_label: 'Shell',
        headline: '执行',
        detail: 'cargo test',
        project: 'bitcat',
        source_label: 'Claude Code',
        tone: 'active',
      },
    };
    dom.window.__agentWatchTest.render({ sessions: [{ ...base, status: 'tool_running', age_sec: 2, display: { ...base.display, age_label: '2s' } }] });
    const card = dom.window.document.querySelector('.task-card');
    const titleNode = card.querySelector('.task-title');

    dom.window.__agentWatchTest.render({
      sessions: [{ ...base, status: 'tool_running', age_sec: 3, display: { ...base.display, age_label: '3s' } }],
    });
    expect(card.querySelector('.task-title')).toBe(titleNode);
    expect(card.querySelector('.task-age')?.textContent).toBe('3s');

    dom.window.__agentWatchTest.render({
      sessions: [{ ...base, status: 'done', display: { ...base.display, headline: '完成', tone: 'done' } }],
    });
    expect(dom.window.document.querySelector('.task-card')).toBe(card);
    expect(card.querySelector('.task-status-text')?.textContent).toBe('完成');
  });

  it('keeps an explicit completion label on collapsed done cards', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's-done',
        source: 'claude_code',
        workspace_name: 'bitcat',
        status: 'done',
        display: {
          action_label: 'Shell',
          headline: '完成',
          detail: '',
          project: 'bitcat',
          source_label: 'Claude Code',
          tone: 'done',
        },
      }],
    });

    const card = dom.window.document.querySelector('.task-card');
    expect(card.querySelector('.task-status-text')?.textContent).toBe('完成');
    expect(card.querySelector('.task-icon svg')).not.toBeNull();
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
    expect(expandedCard.querySelector('.task-context')?.textContent).toContain('Claude');
    expect(expandedCard.querySelector('.task-context')?.textContent).toContain('qy113');
    expect(expandedCard.querySelector('.task-meta')).toBeNull();
    expect(expandedCard.querySelector('[data-action="open"]')).toBeNull();
    expect(expandedCard.querySelector('.task-expanded-actions')).toBeNull();
    expect(expandedCard.querySelector('[data-action="open-workspace"]')).not.toBeNull();
  });

  it('shows usage label in expanded context when the backend provides one', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's-usage',
        source: 'pi',
        workspace_name: 'bitcat',
        status: 'working',
        tokens_in: 12000,
        tokens_out: 345,
        display: {
          action_label: 'Shell',
          headline: 'Running',
          detail: 'cargo test',
          project: 'bitcat',
          source_label: 'pi',
          tone: 'active',
          usage_label: '12.3k',
        },
      }],
    });

    dom.window.document.querySelector('.task-card').click();
    const usage = dom.window.document.querySelector('.task-card .task-usage');
    expect(usage?.textContent).toBe('12.3k');
    expect(usage?.dataset.tooltip).toContain('12,345');
  });

  it('omits the usage chip when there is no usage data', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's-none',
        source: 'codex',
        workspace_name: 'proj',
        status: 'working',
        display: {
          action_label: 'Shell',
          headline: 'Running',
          project: 'proj',
          source_label: 'Codex',
          tone: 'active',
        },
      }],
    });

    expect(dom.window.document.querySelector('.task-card .task-usage')).toBeNull();
  });

  it('renders running status and command as separate detail layers', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's-shell',
        source: 'codex',
        machine: 'qy113',
        workspace_name: 'Whispering_Town',
        status: 'tool_running',
        display: {
          action_label: 'Shell',
          headline: '执行',
          detail: 'npm mvp:play',
          project: 'Whispering_Town',
          source_label: 'Codex',
          tone: 'active',
          age_label: '3m',
        },
      }],
    });

    dom.window.document.querySelector('.task-card').click();

    const detail = dom.window.document.querySelector('.task-detail')?.textContent || '';
    expect(detail).toBe('执行\nnpm mvp:play');
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
        workspace_name: 'bitcat',
        status: 'done',
        display: {
          action_label: 'Shell',
          project: 'bitcat',
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
        workspace_name: 'bitcat',
        status: 'working',
        display: { action_label: 'Patch', project: 'bitcat', source_label: 'Codex' },
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

  it('shows an explicit done label without technical detail on collapsed rows', () => {
    dom.window.__agentWatchTest.render({
      sessions: [{
        session_id: 's2',
        source: 'codex',
        machine: '',
        workspace_name: 'bitcat',
        status: 'done',
        display: {
          action_label: 'Shell',
          headline: 'Done',
          detail: 'Task finished',
          project: 'bitcat',
          source_label: 'Codex',
          age_label: '17s',
          tone: 'done',
        },
      }],
    });

    const card = dom.window.document.querySelector('.task-card');

    expect(card.querySelector('.task-title')?.textContent).toBe('bitcat');
    expect(card.querySelector('.task-status-text')?.textContent).toBe('Done');
    expect(card.querySelector('.task-action-preview')?.textContent).toBe('Task finished');
    expect(card.textContent).not.toContain('Shell');
  });

  it('keeps the CSS shaped like an island header with notification rows', () => {
    const sheet = readFileSync(resolve(process.cwd(), 'css/agent_watch.css'), 'utf8');

    expect(sheet).toContain('.watch-header {');
    expect(sheet).toContain('border-radius: 18px;');
    expect(sheet).toContain('.task-card.expanded');
    expect(sheet).toContain('overflow-y: auto;');
    expect(sheet).toContain('.watch-shell.folded .task-stack');
    expect(sheet).toContain('.task-card:hover .task-dismiss');
    expect(sheet).toContain('.task-identity');
    expect(sheet).toContain('.task-activity');
    expect(sheet).toContain('.watch-tooltip.visible');
    expect(sheet).toContain('scrollbar-width: none;');
    expect(sheet).toContain('.task-detail::-webkit-scrollbar { display: none; }');
    expect(sheet).toContain('background: transparent;');
    expect(sheet).not.toContain('.task-expanded-actions');
    expect(sheet).not.toContain('.task-open');
    expect(sheet).not.toContain('.task-toggle');
  });

  it('keeps specific task context in the view model', () => {
    const view = dom.window.__agentWatchTest.viewOf({
      source: 'codex',
      machine: 'qy113',
      workspace_name: 'bitcat',
      status: 'working',
      status_label: 'Working',
      display: {
        action_label: 'Shell',
        headline: 'Run remote install self-check',
        detail: 'scripts/remote-install.sh',
        project: 'bitcat',
      },
      age_sec: 12,
    });

    expect(view.machine).toBe('qy113');
    expect(view.project).toBe('bitcat');
    expect(view.source).toBe('Codex');
    expect(view.kind).toBe('Shell');
    expect(view.detail).toBe('scripts/remote-install.sh');
    expect(view.statusText).toBe('Run remote install self-check');
    expect(view.iconName).toBe('shell');
    expect(view.sourceBadge).toBe('CX');
    expect(view.deviceBadge).toBe('QY113');
    expect(view.actionPreview).toBe('scripts/remote-install.sh');
  });
});

describe('agent watch subtask grouping', () => {
  let dom;

  beforeEach(() => {
    dom = createDom();
  });

  afterEach(() => {
    dom?.window?.close();
  });

  it('groups background subtasks into the parent card with a count chip', () => {
    const parent = {
      session_id: 'parent',
      source: 'claude_code',
      workspace_name: 'bitcat',
      status: 'working',
      display: {
        action_label: 'Shell',
        headline: 'Running tests',
        project: 'bitcat',
        source_label: 'Claude Code',
        tone: 'active',
        subtask_label: '2 运行中',
      },
    };
    const childA = {
      ...parent,
      session_id: 'claude:parent:agent:a',
      background: true,
      parent_session_id: 'parent',
      status: 'tool_running',
      age_sec: 3,
      display: { ...parent.display, subtask_label: '', action_label: 'Search' },
    };
    const childB = {
      ...parent,
      session_id: 'claude:parent:agent:b',
      background: true,
      parent_session_id: 'parent',
      status: 'waiting',
      age_sec: 2,
      display: { ...parent.display, subtask_label: '', action_label: 'Read' },
    };
    dom.window.__agentWatchTest.render({ sessions: [parent, childA, childB] });

    const doc = dom.window.document;
    // 只渲染主卡片；子会话聚合进去。
    expect(doc.querySelectorAll('.task-card')).toHaveLength(1);
    expect(doc.querySelector('.task-card .task-subtask-count')?.textContent).toBe('2 运行中');

    // 展开后显示子任务明细行。
    doc.querySelector('.task-card').click();
    const rows = doc.querySelectorAll('.task-subtask');
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain('Search');
    expect(rows[1].textContent).toContain('Read');
  });

  it('shows an orphan subtask independently when its parent is unknown', () => {
    const orphan = {
      session_id: 'claude:ghost:agent:x',
      source: 'claude_code',
      workspace_name: 'proj',
      background: true,
      parent_session_id: 'ghost',
      status: 'working',
      display: {
        action_label: 'Search',
        headline: 'Searching',
        project: 'proj',
        source_label: 'Claude Code',
        tone: 'active',
      },
    };
    dom.window.__agentWatchTest.render({ sessions: [orphan] });
    expect(dom.window.document.querySelectorAll('.task-card')).toHaveLength(1);
  });
});
