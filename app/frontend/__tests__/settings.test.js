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

function loadSettings(body = '', invoke = null) {
  const dom = new JSDOM(`<!doctype html><body>${body}</body>`, {
    url: 'http://localhost/settings.html',
    runScripts: 'outside-only',
  });
  const addEventListener = dom.window.document.addEventListener.bind(dom.window.document);
  dom.window.document.addEventListener = (type, listener, options) => {
    if (type === 'DOMContentLoaded') return;
    addEventListener(type, listener, options);
  };
  if (invoke) dom.window.__TAURI__ = { core: { invoke } };
  // settings.js 依赖 agent_sources.js 提供的 window.AgentSources（与页面加载顺序一致）。
  dom.window.eval(fs.readFileSync(resolve(process.cwd(), 'js/agent_sources.js'), 'utf8'));
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

describe('settings pet preview', () => {
  it('keeps the latest cat when an earlier image finishes loading later', async () => {
    const { dom, helpers } = loadSettings('<canvas id="preview" width="192" height="192"></canvas>');
    const pending = new Map();
    dom.window.fetch = (url) => new Promise(resolve => pending.set(url, resolve));
    dom.window.Image = class {
      decode() { return Promise.resolve(); }
    };
    const drawn = [];
    const canvas = dom.window.document.getElementById('preview');
    canvas.getContext = () => ({
      clearRect() {},
      drawImage(image) { drawn.push(image.src); },
    });
    const oldRequest = helpers.renderPetAssetPreview(canvas, '/old-cat');
    const newRequest = helpers.renderPetAssetPreview(canvas, '/new-cat');
    const response = () => ({ ok: true, json: async () => ({ sprite: { frameWidth: 32, frameHeight: 32, columns: 1 } }) });
    pending.get('/new-cat/manifest.json')(response());
    await newRequest;
    pending.get('/old-cat/manifest.json')(response());
    await oldRequest;
    expect(drawn).toEqual(['/new-cat/spritesheet.webp']);
    dom.window.close();
  });

  it('fits the sprite to a large preview without changing the thumbnail size', () => {
    const { dom, helpers } = loadSettings();
    const sizes = [];
    const asset = { manifest: { sprite: { frameWidth: 64, frameHeight: 64, columns: 1 } }, image: {} };
    for (const size of [38, 192]) {
      const canvas = { width: size, height: size, getContext: () => ({
        clearRect() {},
        drawImage(...args) { sizes.push(args.slice(-2)); },
      }) };
      helpers.drawPetAssetPreview(canvas, asset);
    }
    expect(sizes).toEqual([[32, 32], [186, 186]]);
    dom.window.close();
  });
});

describe('memory importance accessibility', () => {
  it.each([1, 3, 5])('exposes level %s while keeping the score visually compact', (level) => {
    const { dom, helpers } = loadSettings();
    dom.window.document.body.innerHTML = helpers.renderMemoryImportance(level);
    const meter = dom.window.document.querySelector('[role="meter"]');
    expect(meter.getAttribute('aria-valuenow')).toBe(String(level));
    expect(meter.getAttribute('aria-valuemin')).toBe('1');
    expect(meter.getAttribute('aria-valuemax')).toBe('5');
    expect(meter.textContent).toBe('');
    expect(meter.querySelectorAll('.filled')).toHaveLength(level);
    dom.window.close();
  });

  it.each([null, undefined, 0, 6, '3'])('does not misrepresent an unscored value (%s) as a rating', (value) => {
    const { dom, helpers } = loadSettings();
    dom.window.document.body.innerHTML = helpers.renderMemoryImportance(value);
    expect(dom.window.document.querySelector('[role="meter"]')).toBeNull();
    expect(dom.window.document.querySelector('[role="img"]').getAttribute('aria-label')).toContain('尚未评估');
    expect(dom.window.document.querySelectorAll('.filled')).toHaveLength(0);
    dom.window.close();
  });
});

describe('usage distribution chart', () => {
  it('shows a neutral empty ring before any usage is recorded', () => {
    const { dom, helpers } = loadSettings('<div id="usage-breakdown"></div>');
    helpers.renderUsageBreakdown({});
    expect(dom.window.document.querySelector('.usage-donut').getAttribute('aria-label')).toBe('今日暂无用量');
    expect(dom.window.document.querySelectorAll('circle')).toHaveLength(1);
    expect([...dom.window.document.querySelectorAll('.usage-percent')].map(x => x.textContent)).toEqual(['0%', '0%', '0%', '0%']);
    dom.window.close();
  });

  it('accounts for usage outside the known categories', () => {
    const { dom, helpers } = loadSettings('<div id="usage-breakdown"></div>');
    helpers.renderUsageBreakdown({ total_tokens: 100, chat_total_tokens: 60, vision_total_tokens: 20 });
    const rows = [...dom.window.document.querySelectorAll('.usage-legend li')];
    expect(rows.at(-1).querySelector('.usage-category').textContent).toBe('其他');
    expect(rows.at(-1).querySelector('strong').textContent).toBe('20');
    const segments = [...dom.window.document.querySelectorAll('circle[pathLength]')];
    expect(segments.reduce((sum, el) => sum + Number(el.getAttribute('stroke-dasharray').split(' ')[0]), 0)).toBe(100);
    dom.window.close();
  });
});

describe('AI connection draft lifecycle', () => {
  const ai = () => ({
    overlay: { base_url: 'https://api.anthropic.com', model: 'saved-model' },
    effective: { base_url: 'https://api.anthropic.com', model: 'saved-model', max_tokens: 256000 },
    has_effective_key: true, has_saved_key: true,
  });
  function connectionPage(invoke) {
    const html = fs.readFileSync(resolve(process.cwd(), 'settings.html'), 'utf8');
    const loaded = loadSettings(html, invoke);
    loaded.helpers.setSnapshot({ ai: ai() });
    loaded.helpers.renderAi(ai());
    loaded.helpers.bindConnection();
    return loaded;
  }

  it('keeps a saved secret out of the input and preserves it in the edit payload', () => {
    const { dom, helpers } = connectionPage();
    expect(dom.window.document.getElementById('ai-key').value).toBe('');
    expect(helpers.collectConnectionDraft().api_key).toBeNull();
    expect(helpers.collectConnectionDraft().clear_saved_key).toBe(false);
    dom.window.document.getElementById('ai-clear-key').click();
    expect(helpers.collectConnectionDraft().clear_saved_key).toBe(true);
    dom.window.document.getElementById('ai-cancel').click();
    expect(helpers.collectConnectionDraft().clear_saved_key).toBe(false);
    dom.window.close();
  });

  it('saves only the connection and retains another section’s unsaved edits', async () => {
    const calls = [];
    const { dom, helpers } = connectionPage(async (command, args) => {
      calls.push([command, args]);
      return command === 'cmd_settings_load' ? { ai: ai() } : null;
    });
    helpers.markDirty('user');
    const model = dom.window.document.getElementById('ai-model');
    model.value = 'new-model';
    model.dispatchEvent(new dom.window.Event('input'));
    expect(await helpers.saveAiConnection()).toBe(true);
    expect(calls.map(x => x[0])).toEqual(['cmd_settings_save_ai', 'cmd_settings_apply', 'cmd_settings_load']);
    expect(calls[0][1].payload.model).toBe('new-model');
    expect(calls[0][1].payload.api_key).toBeNull();
    expect(helpers.getDirty().user).toBe(true);
    expect(helpers.getDirty().ai).toBe(false);
    dom.window.close();
  });

  it('invalidates a successful check after the draft changes, without saving during a check', async () => {
    const calls = [];
    const { dom, helpers } = connectionPage(async command => {
      calls.push(command);
      return { status: 'verified', elapsed_ms: 25 };
    });
    await helpers.testAiConnection();
    expect(calls).toEqual(['cmd_settings_test_ai']);
    expect(dom.window.document.getElementById('connection-status').dataset.state).toBe('verified');
    const model = dom.window.document.getElementById('ai-model');
    model.value = 'different-model';
    model.dispatchEvent(new dom.window.Event('input'));
    expect(dom.window.document.getElementById('connection-status').dataset.state).toBe('unverified');
    dom.window.close();
  });

  it('keeps a failed check from overwriting a later render', async () => {
    let resolveCheck;
    const { dom, helpers } = connectionPage(() => new Promise(resolve => { resolveCheck = resolve; }));
    const pending = helpers.testAiConnection();
    helpers.renderAi(ai());
    resolveCheck({ status: 'unauthorized' });
    await pending;
    expect(dom.window.document.getElementById('connection-status').dataset.state).toBe('unverified');
    expect(dom.window.document.getElementById('connection-fields').disabled).toBe(false);
    dom.window.close();
  });

  it('requires a valid custom endpoint and a whole-number reply limit', () => {
    const { dom, helpers } = connectionPage();
    dom.window.document.getElementById('ai-provider').value = 'custom';
    dom.window.document.getElementById('ai-baseurl').value = 'https://example.com?key=secret';
    expect(() => helpers.collectConnectionDraft()).toThrow('服务地址无效');
    dom.window.document.getElementById('ai-baseurl').value = 'https://example.com';
    dom.window.document.getElementById('ai-maxtokens').value = '1.5';
    expect(() => helpers.collectConnectionDraft()).toThrow('正整数');
    dom.window.close();
  });
});

describe('interactive cat preview', () => {
  const asset = () => ({
    image: {}, manifest: { sprite: { frameWidth: 32, frameHeight: 32, columns: 4, rows: 2, frameCount: 8 },
      states: { idle: { frames: [{ sprite: 0, duration: 100 }] },
        happy: { frames: [{ sprite: 2, duration: 100 }, { sprite: 3, duration: 200 }] },
        sleep: { frames: [{ sprite: 6, duration: 400 }] } } },
  });

  it('uses real manifest frame indices and durations, including loop boundaries', () => {
    const { dom, helpers } = loadSettings();
    const frames = helpers.petPreviewFrames(asset(), 'happy');
    expect([0, 99, 100, 299, 300].map(time => helpers.petPreviewFrameAt(frames, time))).toEqual([2, 2, 3, 3, 2]);
    expect(helpers.availablePetPreviewStates(asset()).map(x => x[0])).toEqual(['idle', 'happy', 'sleep']);
    expect(helpers.petPreviewFrames({ manifest: { states: { idle: { frames: {} } } } }, 'idle')).toEqual([]);
    dom.window.close();
  });

  it('cycles only available states without saving or marking settings dirty', () => {
    const calls = [];
    const { dom, helpers } = loadSettings('<button id="pet-preview-play"><canvas id="pet-large-preview" width="192" height="192"></canvas></button><span id="pet-preview-state"></span>', command => calls.push(command));
    const drawn = [];
    dom.window.document.querySelector('canvas').getContext = () => ({ clearRect() {}, drawImage(...args) { drawn.push(args.slice(1, 3)); } });
    helpers.bindPetPreview();
    helpers.setPetPreviewAsset(asset());
    const button = dom.window.document.getElementById('pet-preview-play');
    button.click();
    expect(dom.window.document.getElementById('pet-preview-state').textContent).toBe('开心');
    button.click();
    expect(dom.window.document.getElementById('pet-preview-state').textContent).toBe('打个盹');
    expect(drawn).toEqual([[64, 0], [64, 32]]);
    expect(calls).toEqual([]);
    expect(Object.values(helpers.getDirty()).some(Boolean)).toBe(false);
    helpers.stopPetPreview();
    expect(dom.window.document.getElementById('pet-preview-state').textContent).toBe('安静待着');
    dom.window.close();
  });
});

describe('agent connectors panel', () => {
  const statuses = [
    { source: 'claude_code', installed: true, path: '~/.claude/settings.json' },
    { source: 'codex', installed: true, path: '~/.codex/config.toml' },
    { source: 'pi', installed: false, path: '~/.pi/agent/extensions/bitcat-watch.ts' },
    { source: 'opencode', installed: true, path: '~/.config/opencode/plugins/bitcat-watch.js' },
  ];

  function loadConnectors(sessions = []) {
    const { dom, helpers } = loadSettings('<div id="aw-connectors"></div>');
    helpers.setConnectorStatuses(statuses);
    helpers.setLatestAgentSnapshot({ sessions });
    helpers.renderConnectors();
    return { dom, helpers };
  }

  it('renders one row per source and shows the repair button only for missing ones', () => {
    const { dom } = loadConnectors();
    const rows = [...dom.window.document.querySelectorAll('.connector-row')];
    expect(rows.map(row => row.dataset.source)).toEqual(['claude_code', 'codex', 'pi', 'opencode']);
    expect(rows.map(row => row.querySelector('strong').textContent))
      .toEqual(['Claude Code', 'Codex', 'pi', 'opencode']);
    const repairButtons = dom.window.document.querySelectorAll('[data-repair-source]');
    expect(repairButtons.length).toBe(1);
    expect(repairButtons[0].dataset.repairSource).toBe('pi');
    dom.window.close();
  });

  it('marks rows connected only with a recent event, otherwise idle', () => {
    const now = Date.now();
    const { dom, helpers } = loadConnectors([
      { source: 'claude_code', updated_at_ms: now - 60_000 },
      { source: 'codex', updated_at_ms: now - 3 * 24 * 60 * 60 * 1000 },
    ]);
    const stateOf = source =>
      dom.window.document.querySelector(`[data-source="${source}"] .connector-dot`).dataset.state;
    expect(stateOf('claude_code')).toBe('ready');
    expect(stateOf('codex')).toBe('idle');
    expect(stateOf('pi')).toBe('missing');
    expect(stateOf('opencode')).toBe('idle');
    expect(helpers.connectorState({ source: 'pi', installed: false }, 0, now)).toEqual({
      dot: 'missing', hint: '未安装连接脚本',
    });
    dom.window.close();
  });
});

describe('action key rows', () => {
  const catalogBtn = { name: 'A', label: '确认', position: '面键-右下', order: 2 };
  const launchDef = { type: 'launch', program: 'D:\\tools\\obs.exe', args: '', workdir: '', terminal: true };

  it('merges position into the key meta as a parenthetical', () => {
    const { helpers } = loadSettings('<div></div>');
    expect(helpers.positionLabel('面键-右下')).toBe('右下面键');
    expect(helpers.positionLabel('左上边缘')).toBe('左上边缘');
    expect(helpers.keyMetaText(catalogBtn, '')).toBe('确认（右下面键）');
    expect(helpers.keyMetaText({ name: 'X', label: '', position: '' }, 'Select + ↑'))
      .toBe('触发 Select + ↑');
  });

  it('renders unbound rows without a summary and collapsed', () => {
    const { dom, helpers } = loadSettings('<div id="holder"></div>');
    const row = helpers.renderActionItem(catalogBtn, null);
    dom.window.document.getElementById('holder').appendChild(row);
    expect(row.classList.contains('unbound')).toBe(true);
    expect(row.querySelector('.key-meta').textContent).toBe('确认（右下面键）');
    expect(row.querySelector('.action-summary').textContent).toBe('');
    // 隐藏靠 CSS 的 .unbound 规则（jsdom 不加载外部样式表），这里断言类契约。
    expect(row.classList.contains('expanded')).toBe(false);
    dom.window.close();
  });

  it('shows a one-line summary for bound rows and expands the form on click', () => {
    const { dom, helpers } = loadSettings('<div id="holder"></div>');
    const row = helpers.renderActionItem(catalogBtn, launchDef);
    dom.window.document.getElementById('holder').appendChild(row);
    expect(row.classList.contains('unbound')).toBe(false);
    // 类型已由右侧下拉框表达，摘要只说"做什么"，不重复类型名。
    expect(row.querySelector('.action-summary').textContent).toBe('打开 D:\\tools\\obs.exe');
    expect(row.classList.contains('expanded')).toBe(false);

    row.querySelector('.action-summary').click();
    expect(row.classList.contains('expanded')).toBe(true);
    expect(row.querySelector('.action-summary').getAttribute('aria-expanded')).toBe('true');
    const labels = [...row.querySelectorAll('.ai-body label')].map(l => l.textContent);
    expect(labels).toContain('键盘快捷键');
    expect(row.querySelector('.row.with-hint .row-hint').textContent).toContain('留空关闭');
    dom.window.close();
  });

  it('auto-expands when a type is picked and collapses back on unbind', () => {
    const { dom, helpers } = loadSettings('<div id="holder"></div>');
    const row = helpers.renderActionItem(catalogBtn, null);
    dom.window.document.getElementById('holder').appendChild(row);
    const select = row.querySelector('.a-type');
    select.value = 'screenshot';
    select.dispatchEvent(new dom.window.Event('change'));
    expect(row.classList.contains('expanded')).toBe(true);
    expect(row.querySelector('.action-summary').textContent).toBe('立即截图分析');

    select.value = 'unbound';
    select.dispatchEvent(new dom.window.Event('change'));
    expect(row.classList.contains('expanded')).toBe(false);
    expect(row.classList.contains('unbound')).toBe(true);
    dom.window.close();
  });
});

describe('connector headline', () => {
  const head = '<div id="aw-connectors"></div><span id="aw-connectors-status"></span><button id="aw-connectors-check"></button>';

  it('summarizes counts in the section head and de-emphasizes the check button when all connected', () => {
    const { dom, helpers } = loadSettings(head);
    const status = dom.window.document.getElementById('aw-connectors-status');
    const check = dom.window.document.getElementById('aw-connectors-check');

    helpers.setConnectorStatuses([
      { source: 'claude_code', installed: true },
      { source: 'codex', installed: true },
      { source: 'pi', installed: false },
      { source: 'opencode', installed: true },
    ]);
    helpers.renderConnectorHeadline();
    expect(status.textContent).toBe('3 已接入 · 1 未接入');
    expect(status.dataset.state).toBe('missing');
    expect(check.classList.contains('ghost')).toBe(false);

    helpers.setConnectorStatuses([
      { source: 'claude_code', installed: true },
      { source: 'codex', installed: true },
      { source: 'pi', installed: true },
      { source: 'opencode', installed: true },
    ]);
    helpers.renderConnectorHeadline();
    expect(status.textContent).toBe('4 已接入');
    expect(status.dataset.state).toBe('ready');
    expect(check.classList.contains('ghost')).toBe(true);
    dom.window.close();
  });

  it('keeps the error path when statuses cannot be read', () => {
    const { dom, helpers } = loadSettings(head);
    helpers.setConnectorStatuses(null);
    helpers.renderConnectorHeadline();
    const status = dom.window.document.getElementById('aw-connectors-status');
    expect(status.textContent).toBe('读取失败');
    expect(status.dataset.state).toBe('error');
    dom.window.close();
  });
});
