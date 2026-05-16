// 8Bit Cat 设置界面逻辑
// - 启动拉取 cmd_settings_load
// - 左侧 tab 切换 + dirty 检测
// - 底部保存/取消/重置，Esc 关闭

const invoke = window.__TAURI__?.core?.invoke || mockInvoke;

const ACTION_TYPES = ["unbound", "launch", "hotkey", "script", "voice", "screenshot"];
const ACTION_TYPE_LABELS = {
  unbound: "未绑定",
  launch: "启动程序",
  hotkey: "按键序列",
  script: "脚本命令",
  voice: "语音触发",
  screenshot: "立即截图",
};

let SNAPSHOT = null;
const dirty = { ai: false, user: false, actions: false, prompts: false, appearance: false, agent_watch: false };
let currentTab = "overview";
let selectedUsageModel = "__all";
let musicDiagnosticsBound = false;
let musicDiagnosticsRenderTimer = null;
let agentWatchTimer = null;
const MUSIC_STATE = {
  status: "idle",
  source: "-",
  sessionId: null,
  energy: 0,
  bass: 0,
  onset: false,
  silence: true,
  updatedAt: null,
  error: null,
};
const MUSIC_DIAGNOSTICS_RENDER_MS = 500;

async function mockInvoke(command) {
  if (command === "cmd_settings_load") {
    return {
      ai: {
        overlay: {},
        effective: {
          base_url: "https://api.anthropic.com",
          model: "claude-sonnet-4-20250514",
          max_tokens: 256000,
        },
        has_effective_key: true,
      },
      user: {
        name: "小顾",
        role: "独立开发者",
        preferences: ["回答先给结论", "代码改动保持克制"],
        context: "正在打磨 8Bit Cat 的桌面体验。",
        language: "zh-CN",
      },
      actions: {
        defaults: { terminal: "powershell", window: "maximized" },
        actions: {},
      },
      prompts: {
        agent: { preamble: "" },
        vision: { prompt: "", prompt_multi: "" },
        memory: { max_entries: 20, max_context_chars: 6000 },
        screen_summary: { interval_min: 5 },
      },
      appearance: {
        always_on_top: false,
        default_collapsed: false,
        tts_enabled: true,
        global_shortcut: "CommandOrControl+Alt+Space",
        screenshot_interval_sec: 30,
      },
      agent_watch: {
        enabled: false,
        away_nudge_enabled: true,
        first_nudge_after_sec: 30,
        repeat_nudge_after_min: 8,
        waiting_alert: true,
        done_alert: true,
        use_tts: false,
      },
      about: {
        version: "preview",
        app_settings_path: "~/.ai-pad/app_settings.json",
        actions_yml_hint: "config/actions.yml",
        prompts_yml_hint: "config/prompts.yml",
      },
      button_catalog: [
        { name: "Start", label: "开始", position: "中间偏右", order: 1 },
        { name: "A", label: "确认", position: "右侧下", order: 2 },
        { name: "B", label: "返回", position: "右侧右", order: 3 },
      ],
    };
  }
  if (command === "cmd_get_token_stats") {
    return {
      generated_at: new Date().toISOString(),
      today: {
        record_count: 12,
        input_tokens: 14520,
        output_tokens: 8230,
        total_tokens: 22750,
        cache_read_tokens: 3600,
        cache_write_tokens: 910,
        chat_total_tokens: 15800,
        vision_total_tokens: 3600,
        screen_summary_total_tokens: 2400,
        memory_aggregation_total_tokens: 950,
      },
      selected_model: null,
      models: [
        { model: "claude-sonnet-4-20250514", record_count: 8, total_tokens: 18200 },
        { model: "claude-opus-4-20250514", record_count: 4, total_tokens: 4550 },
      ],
      recent_sessions: [],
      paths: {
        usage_jsonl: "~/.ai-pad/logs/token_usage.jsonl",
        sessions_json: "~/.ai-pad/logs/token_sessions.json",
      },
    };
  }
  if (command === "cmd_get_memory_review") {
    return {
      generated_at: new Date().toISOString(),
      total_entries: 2,
      entries: [],
      markdown: "",
    };
  }
  if (command === "cmd_get_resource_usage") {
    return {
      generated_at: new Date().toISOString(),
      process_cpu_percent: 4.8,
      process_memory_mb: 156.4,
      system_memory_used_mb: 18342,
      system_memory_total_mb: 32674,
      system_memory_percent: 56.1,
    };
  }
  if (command === "cmd_get_pet_event_log") {
    return { entries: [] };
  }
  return null;
}

function log(msg) {
  try { invoke("cmd_settings_log", { msg: String(msg) }); } catch {}
}

function toast(text, kind = "ok") {
  const el = $("toast");
  el.textContent = text;
  el.classList.remove("hidden", "ok", "err");
  el.classList.add(kind);
  clearTimeout(toast._t);
  toast._t = setTimeout(() => el.classList.add("hidden"), 2200);
}

function $(id) { return document.getElementById(id); }

function markDirty(tab) {
  dirty[tab] = true;
  const nav = document.querySelector(`.nav-item[data-tab="${tab === "user" ? "memory" : tab === "agent_watch" ? "agent-watch" : tab}"]`);
  if (nav) nav.classList.add("dirty");
}

function clearDirty(tab) {
  dirty[tab] = false;
  const nav = document.querySelector(`.nav-item[data-tab="${tab === "user" ? "memory" : tab === "agent_watch" ? "agent-watch" : tab}"]`);
  if (nav) nav.classList.remove("dirty");
}

function anyDirty() { return Object.values(dirty).some(Boolean); }

function switchTab(name) {
  currentTab = name;
  document.querySelectorAll(".nav-item").forEach(b => {
    b.classList.toggle("active", b.dataset.tab === name);
  });
  document.querySelectorAll(".tab").forEach(s => {
    s.classList.toggle("hidden", s.dataset.pane !== name);
  });
  if (name === "usage") loadUsageDiagnostics();
  if (name === "memory") loadMemoryReview();
  if (name === "agent-watch") startAgentWatchRefresh();
  else stopAgentWatchRefresh();
}

function renderAi(ai) {
  $("ai-key").value = ai.overlay.api_key || "";
  $("ai-baseurl").value = ai.overlay.base_url || "";
  $("ai-model").value = ai.overlay.model || "";
  $("ai-maxtokens").value = ai.overlay.max_tokens == null ? "" : ai.overlay.max_tokens;

  const eff = ai.effective;
  const effectiveHtml =
    `<div class="effective-title">当前生效</div>` +
    `<div class="effective-item"><span>API Key</span><b>${ai.has_effective_key ? "已配置" : "未配置"}</b></div>` +
    `<div class="effective-item"><span>Base URL</span><b title="${escapeAttr(eff.base_url)}">${escapeHtml(eff.base_url)}</b></div>` +
    `<div class="effective-item"><span>模型</span><b title="${escapeAttr(eff.model)}">${escapeHtml(eff.model)}</b></div>` +
    `<div class="effective-item"><span>最大 token</span><b>${formatNumber(eff.max_tokens)}</b></div>`;
  $("ai-effective").innerHTML = effectiveHtml;
  $("overview-effective").innerHTML = effectiveHtml;
  $("ov-ai-model").textContent = eff.model || "-";
  $("ov-ai-key").textContent = ai.has_effective_key ? "API Key 已配置" : "API Key 未配置";

  ["ai-key", "ai-baseurl", "ai-model", "ai-maxtokens"].forEach(id => {
    $(id).oninput = () => markDirty("ai");
  });
}

function renderUser(user) {
  $("u-name").value = user?.name || "";
  $("u-role").value = user?.role || "";
  $("u-language").value = user?.language || "";
  $("u-context").value = user?.context || "";
  $("u-preferences").value = Array.isArray(user?.preferences) ? user.preferences.join("\n") : "";
  ["u-name", "u-role", "u-language", "u-context", "u-preferences"].forEach(id => {
    $(id).oninput = () => markDirty("user");
  });
}

function collectUser() {
  return {
    name: $("u-name").value.trim(),
    role: $("u-role").value.trim(),
    preferences: $("u-preferences").value
      .split(/\r?\n/)
      .map(value => value.trim())
      .filter(Boolean),
    context: $("u-context").value.trim(),
    language: $("u-language").value.trim(),
  };
}

function renderActions(actionsView) {
  $("actions-term").value = actionsView.defaults.terminal || "powershell";
  $("actions-win").value = actionsView.defaults.window || "maximized";
  $("actions-term").onchange = () => markDirty("actions");
  $("actions-win").onchange = () => markDirty("actions");

  const list = $("actions-list");
  list.innerHTML = "";

  const catalog = Array.isArray(SNAPSHOT.button_catalog) ? SNAPSHOT.button_catalog : [];
  if (catalog.length > 0) {
    for (const item of catalog) {
      const def = actionsView.actions[item.name] || null;
      list.appendChild(renderActionItem(item, def));
    }
    const catalogNames = new Set(catalog.map(i => i.name));
    Object.keys(actionsView.actions).sort().forEach(key => {
      if (catalogNames.has(key)) return;
      list.appendChild(renderActionItem(
        { name: key, label: "(自定义)", position: "", order: 9999 },
        actionsView.actions[key]
      ));
    });
  } else {
    Object.keys(actionsView.actions).sort().forEach(key => {
      list.appendChild(renderActionItem(
        { name: key, label: "", position: "", order: 0 },
        actionsView.actions[key]
      ));
    });
  }
}

function renderActionItem(btn, def) {
  const el = document.createElement("div");
  el.className = "action-item";
  el.dataset.key = btn.name;

  const isUnbound = !def;
  if (isUnbound) el.classList.add("unbound");

  const curType = def ? def.action_type : "unbound";
  const trigHintText = def && Array.isArray(def.trigger) && def.trigger.length > 0
    ? def.trigger.join(" + ")
    : "";
  const meta = [btn.label, btn.position, trigHintText && `触发 ${trigHintText}`].filter(Boolean).join(" · ");
  const workingDef = def ? { ...def } : { action_type: "unbound" };

  el.innerHTML = `
    <div class="ai-head">
      <div class="key-block">
        <span class="key">${escapeHtml(btn.name)}</span>
        <span class="key-meta">${escapeHtml(meta || "自定义按键")}</span>
      </div>
      <span class="action-summary">${escapeHtml(actionSummary(workingActionType(def), def))}</span>
      <select class="a-type" title="动作类型">
        ${ACTION_TYPES.map(t => `<option value="${t}" ${t === curType ? "selected" : ""}>${escapeHtml(ACTION_TYPE_LABELS[t] || t)}</option>`).join("")}
      </select>
    </div>
    <div class="ai-body"></div>
  `;

  const body = el.querySelector(".ai-body");
  const summary = el.querySelector(".action-summary");
  const refreshSummary = () => {
    if (summary) summary.textContent = actionSummary(workingDef.action_type, workingDef);
  };
  renderActionBody(body, workingDef, refreshSummary);

  const sel = el.querySelector(".a-type");
  sel.addEventListener("change", () => {
    workingDef.action_type = sel.value;
    el.classList.toggle("unbound", sel.value === "unbound");
    refreshSummary();
    renderActionBody(body, workingDef, refreshSummary);
    markDirty("actions");
  });
  return el;
}

function workingActionType(def) {
  return def ? def.action_type : "unbound";
}

function renderActionBody(body, def, onChange = () => {}) {
  body.innerHTML = "";
  const t = def.action_type;
  if (t === "unbound") return;

  const mk = (label, id, val, type = "text") => {
    const row = document.createElement("div");
    row.className = "row";
    row.innerHTML = `<label>${label}</label><input data-field="${id}" type="${type}" value="${escapeAttr(val ?? "")}" />`;
    body.appendChild(row);
    row.querySelector("input").oninput = (event) => {
      setWorkingActionField(def, id, event.target.value);
      onChange();
      markDirty("actions");
    };
  };
  const mkToggle = (label, id, val) => {
    const row = document.createElement("div");
    row.className = "row toggle";
    row.innerHTML = `<label>${label}</label><input data-field="${id}" type="checkbox" ${val ? "checked" : ""} />`;
    body.appendChild(row);
    row.querySelector("input").onchange = (event) => {
      setWorkingActionField(def, id, event.target.checked);
      onChange();
      markDirty("actions");
    };
  };

  if (t === "launch") {
    mk("程序", "program", def.program || "");
    mk("参数", "args", def.args || "");
    mk("工作目录", "workdir", def.workdir || "");
    mkToggle("终端启动", "terminal", !!def.terminal);
  } else if (t === "hotkey" || t === "script") {
    mk("命令", "command", def.command || "");
  } else if (t === "voice") {
    const trig = def.voice?.trigger?.join(",") ?? "";
    const delay = def.voice?.delay ?? 1.0;
    mk("触发键", "voice-trigger", trig);
    mk("延迟（秒）", "voice-delay", delay, "number");
  }
  mk("键盘热键", "kbd", def.keyboard_shortcut || "");
}

function setWorkingActionField(def, id, value) {
  if (id === "program") def.program = value;
  else if (id === "args") def.args = value;
  else if (id === "workdir") def.workdir = value;
  else if (id === "terminal") def.terminal = value;
  else if (id === "command") def.command = value;
  else if (id === "kbd") def.keyboard_shortcut = value;
  else if (id === "voice-trigger") {
    def.voice = def.voice || {};
    def.voice.trigger = String(value || "").split(",").map(s => s.trim()).filter(Boolean);
  } else if (id === "voice-delay") {
    def.voice = def.voice || {};
    def.voice.delay = parseFloat(value) || 1.0;
  }
}

function collectActions() {
  const defaults = {
    terminal: $("actions-term").value.trim() || "powershell",
    window: $("actions-win").value.trim() || "maximized",
  };
  const actions = {};
  document.querySelectorAll(".action-item").forEach(el => {
    const key = el.dataset.key;
    const type = el.querySelector(".a-type").value;
    if (type === "unbound") return;
    const def = { type };
    const getVal = (f) => {
      const node = el.querySelector(`input[data-field="${f}"]`);
      return node ? (node.type === "checkbox" ? node.checked : node.value) : null;
    };
    const existing = SNAPSHOT.actions.actions[key] || {};
    if (existing.trigger) def.trigger = existing.trigger;
    if (type === "launch") {
      const program = getVal("program");
      const args = getVal("args");
      const workdir = getVal("workdir");
      const terminal = getVal("terminal");
      if (program) def.program = program;
      if (args) def.args = args;
      if (workdir) def.workdir = workdir;
      if (terminal) def.terminal = true;
    } else if (type === "hotkey" || type === "script") {
      const cmd = getVal("command");
      if (cmd) def.command = cmd;
    } else if (type === "voice") {
      const trig = (getVal("voice-trigger") || "").split(",").map(s => s.trim()).filter(Boolean);
      const delay = parseFloat(getVal("voice-delay")) || 1.0;
      def.voice = { trigger: trig, delay };
    }
    const kbd = (getVal("kbd") || "").trim();
    if (kbd) def.keyboard_shortcut = kbd;
    actions[key] = def;
  });
  return { defaults, actions };
}

function actionSummary(type, def) {
  if (!def || type === "unbound") return "未写入";
  if (type === "launch") return def.program ? `打开 ${def.program}` : "启动程序";
  if (type === "hotkey") return def.command || "按键序列";
  if (type === "script") return def.command || "脚本命令";
  if (type === "voice") return "语音触发";
  if (type === "screenshot") return "立即截图分析";
  return ACTION_TYPE_LABELS[type] || type;
}

function renderPrompts(p) {
  $("p-agent").value = p.agent.preamble;
  $("p-vision").value = p.vision.prompt;
  $("p-vision-multi").value = p.vision.prompt_multi;
  $("p-mem-max").value = p.memory.max_entries;
  $("p-mem-ctx").value = p.memory.max_context_chars;
  $("p-ss-interval").value = p.screen_summary.interval_min;

  ["p-agent","p-vision","p-vision-multi","p-mem-max","p-mem-ctx","p-ss-interval"].forEach(id => {
    $(id).oninput = () => markDirty("prompts");
  });
}

function collectPrompts() {
  const p = structuredClone(SNAPSHOT.prompts);
  p.agent.preamble = $("p-agent").value;
  p.vision.prompt = $("p-vision").value;
  p.vision.prompt_multi = $("p-vision-multi").value;
  p.memory.max_entries = parseInt($("p-mem-max").value) || p.memory.max_entries;
  p.memory.max_context_chars = parseInt($("p-mem-ctx").value) || p.memory.max_context_chars;
  p.screen_summary.interval_min = parseInt($("p-ss-interval").value) || p.screen_summary.interval_min;
  return p;
}

function renderAppearance(a) {
  $("a-top").checked = a.always_on_top;
  $("a-collapsed").checked = a.default_collapsed;
  $("a-tts").checked = a.tts_enabled;
  $("a-shortcut").value = a.global_shortcut;
  $("a-ss-interval").value = a.screenshot_interval_sec ?? 30;
  updateOverviewAppearance(a);

  ["a-top","a-collapsed","a-tts"].forEach(id => { $(id).onchange = () => markDirty("appearance"); });
  ["a-shortcut","a-ss-interval"].forEach(id => { $(id).oninput = () => markDirty("appearance"); });
}

function collectAppearance() {
  const rawInterval = parseInt($("a-ss-interval").value, 10);
  const interval = Number.isFinite(rawInterval) ? Math.min(3600, Math.max(5, rawInterval)) : 30;
  return {
    always_on_top: $("a-top").checked,
    default_collapsed: $("a-collapsed").checked,
    tts_enabled: $("a-tts").checked,
    global_shortcut: $("a-shortcut").value.trim() || "CommandOrControl+Alt+Space",
    screenshot_interval_sec: interval,
  };
}

function renderAgentWatch(a) {
  const cfg = a || {};
  $("aw-enabled").checked = !!cfg.enabled;
  $("aw-away").checked = cfg.away_nudge_enabled !== false;
  $("aw-first").value = cfg.first_nudge_after_sec ?? 30;
  $("aw-repeat").value = cfg.repeat_nudge_after_min ?? 8;
  $("aw-waiting").checked = cfg.waiting_alert !== false;
  $("aw-done").checked = cfg.done_alert !== false;
  $("aw-tts").checked = !!cfg.use_tts;
  ["aw-enabled","aw-away","aw-waiting","aw-done","aw-tts"].forEach(id => { $(id).onchange = () => markDirty("agent_watch"); });
  ["aw-first","aw-repeat"].forEach(id => { $(id).oninput = () => markDirty("agent_watch"); });
}

function collectAgentWatch() {
  const first = parseInt($("aw-first").value, 10);
  const repeat = parseInt($("aw-repeat").value, 10);
  return {
    enabled: $("aw-enabled").checked,
    away_nudge_enabled: $("aw-away").checked,
    first_nudge_after_sec: Number.isFinite(first) ? Math.min(3600, Math.max(10, first)) : 30,
    repeat_nudge_after_min: Number.isFinite(repeat) ? Math.min(240, Math.max(1, repeat)) : 8,
    waiting_alert: $("aw-waiting").checked,
    done_alert: $("aw-done").checked,
    use_tts: $("aw-tts").checked,
  };
}

function renderAbout(a) {
  $("about-version").textContent = a.version;
  $("about-settings-path").textContent = a.app_settings_path;
  $("about-actions-hint").textContent = a.actions_yml_hint;
  $("about-prompts-hint").textContent = a.prompts_yml_hint;
}

async function loadUsageDiagnostics() {
  await Promise.all([loadTokenStats(), loadPetEventLog(), loadResourceUsage()]);
  renderMusicDiagnostics();
}

async function loadResourceUsage() {
  try {
    const usage = await invoke("cmd_get_resource_usage");
    renderResourceUsage(usage);
  } catch (e) {
    log("加载资源占用失败: " + e);
    renderResourceUsage(null);
  }
}

async function loadTokenStats() {
  const status = $("usage-status");
  if (status) status.textContent = "读取中...";
  try {
    const model = selectedUsageModel === "__all" ? null : selectedUsageModel;
    const stats = await invoke("cmd_get_token_stats", { model });
    renderTokenStats(stats);
    if (status) status.textContent = `更新于 ${formatDateTime(stats.generated_at)}`;
  } catch (e) {
    log("加载 token 统计失败: " + e);
    if (status) status.textContent = "读取失败：" + String(e);
    renderTokenStats(null);
  }
}

async function loadPetEventLog() {
  try {
    const logView = await invoke("cmd_get_pet_event_log");
    renderPetEventLog(logView);
  } catch (e) {
    log("加载宠物事件失败: " + e);
    renderPetEventLog(null);
  }
}

async function loadMemoryReview() {
  try {
    const review = await invoke("cmd_get_memory_review", { limit: 20 });
    renderMemoryReview(review);
  } catch (e) {
    log("加载长期记忆失败: " + e);
    renderMemoryReview(null);
  }
}

async function loadAgentSessions() {
  const status = $("aw-status");
  if (status) status.textContent = "读取中...";
  try {
    const snapshot = await invoke("cmd_get_agent_sessions");
    renderAgentSessions(snapshot);
    if (status) status.textContent = snapshot?.generated_at_ms ? "已更新" : "等待状态";
  } catch (e) {
    log("加载 Agent 会话失败: " + e);
    renderAgentSessions(null);
    if (status) status.textContent = "读取失败";
  }
}

function startAgentWatchRefresh() {
  loadAgentSessions();
  if (agentWatchTimer) return;
  agentWatchTimer = setInterval(() => {
    if (currentTab === "agent-watch") loadAgentSessions();
  }, 2000);
}

function stopAgentWatchRefresh() {
  if (!agentWatchTimer) return;
  clearInterval(agentWatchTimer);
  agentWatchTimer = null;
}

function renderAgentSessions(snapshot) {
  const box = $("aw-sessions");
  if (!box) return;
  const diag = $("aw-diag");
  if (diag) {
    const parts = [];
    if (snapshot?.monitor_port) parts.push(`端口 ${snapshot.monitor_port}`);
    if (typeof snapshot?.event_count === "number") parts.push(`事件 ${snapshot.event_count}`);
    if (snapshot?.last_event_at_ms) parts.push(`最近 ${new Date(snapshot.last_event_at_ms).toLocaleTimeString()}`);
    if (snapshot?.log_dir) parts.push(`日志 ${snapshot.log_dir}`);
    diag.innerHTML = parts.map(part => `<span>${escapeHtml(part)}</span>`).join("");
  }
  const sessions = snapshot?.sessions || [];
  if (!sessions.length) {
    box.innerHTML = `<div class="empty-note">还没有 Agent 会话。安装 Claude Code 或 Codex hook 并启用看管后，状态会出现在这里。</div>`;
    return;
  }
  box.innerHTML = sessions.map(session => `
    <div class="agent-session ${escapeAttr(session.status)}">
      <div class="agent-session-main">
        <strong>${escapeHtml(session.workspace_name || "未知项目")}</strong>
        <span>${escapeHtml(session.status_label || session.status)}</span>
      </div>
      <div class="agent-session-sub">
        <span>${escapeHtml(agentSourceLabel(session.source))}</span>
        <code>${escapeHtml(session.workspace || session.session_id)}</code>
        ${session.tool_name ? `<small>${escapeHtml(session.tool_name)}</small>` : ""}
      </div>
      ${session.user_prompt_preview ? `<p>${escapeHtml(session.user_prompt_preview)}</p>` : ""}
    </div>
  `).join("");
}

function agentSourceLabel(source) {
  if (source === "codex") return "Codex";
  if (source === "claude_code") return "Claude Code";
  return source || "Agent";
}

async function deleteMemoryEntry(id) {
  try {
    const review = await invoke("cmd_delete_memory_entry", { id, limit: 20 });
    renderMemoryReview(review);
    toast("记忆已删除", "ok");
  } catch (e) {
    toast("删除失败：" + String(e), "err");
  }
}

function renderTokenStats(stats) {
  const empty = {
    record_count: 0,
    input_tokens: 0,
    output_tokens: 0,
    total_tokens: 0,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    chat_total_tokens: 0,
    vision_total_tokens: 0,
    screen_summary_total_tokens: 0,
    memory_aggregation_total_tokens: 0,
  };
  const today = stats?.today || empty;
  renderUsageModelSelect(stats);
  $("usage-total").innerHTML = metricValue(today.total_tokens);
  $("usage-io").innerHTML = pairedMetric("输入", today.input_tokens, "输出", today.output_tokens);
  $("usage-cache").innerHTML = pairedMetric("读", today.cache_read_tokens, "写", today.cache_write_tokens);
  $("usage-records").innerHTML = metricValue(today.record_count, "条");
  $("ov-usage-total").textContent = formatNumber(today.total_tokens);
  $("usage-paths").textContent = stats
    ? `${stats.paths.usage_jsonl}\n${stats.paths.sessions_json}`
    : "-";

  renderUsageBreakdown(today);
  renderUsageSessions(stats?.recent_sessions || []);
}

function renderUsageModelSelect(stats) {
  const select = $("usage-model");
  if (!select || !stats) return;
  const models = stats.models || [];
  const current = stats.selected_model || "__all";
  selectedUsageModel = current;
  const options = [
    `<option value="__all">全部模型</option>`,
    ...models.map(item => {
      const detail = `${compactNumber(item.total_tokens)} · ${formatNumber(item.record_count)} 条`;
      return `<option value="${escapeAttr(item.model)}">${escapeHtml(item.model)} (${escapeHtml(detail)})</option>`;
    }),
  ].join("");
  if (select.innerHTML !== options) select.innerHTML = options;
  select.value = current;
}

function renderResourceUsage(usage) {
  $("resource-cpu").innerHTML = usage ? metricValue(formatFixed(usage.process_cpu_percent, 1), "%") : "-";
  $("resource-process-memory").innerHTML = usage ? metricValue(formatFixed(usage.process_memory_mb, 1), "MB") : "-";
  $("resource-system-memory").innerHTML = usage
    ? pairedMetric("已用", `${formatFixed(usage.system_memory_used_mb / 1024, 1)} GB`, "总量", `${formatFixed(usage.system_memory_total_mb / 1024, 1)} GB`)
    : "-";
  $("resource-system-memory-bar").style.width = usage ? `${Math.round(usage.system_memory_percent)}%` : "0%";
  $("resource-updated").textContent = usage?.generated_at ? `更新于 ${formatDateTime(usage.generated_at)}` : "读取失败";
}

function renderUsageBreakdown(today) {
  const rows = [
    ["聊天", today.chat_total_tokens],
    ["截图理解", today.vision_total_tokens],
    ["屏幕摘要", today.screen_summary_total_tokens],
    ["记忆聚合", today.memory_aggregation_total_tokens],
  ];
  const total = Math.max(1, today.total_tokens || rows.reduce((sum, [, value]) => sum + value, 0));
  const box = $("usage-breakdown");
  box.innerHTML = rows.map(([label, value]) => {
    const pct = Math.round((value / total) * 100);
    return `
      <div class="usage-bar-row">
        <div class="usage-bar-meta">
          <span>${escapeHtml(label)}</span>
          <span>${formatNumber(value)} · ${pct}%</span>
        </div>
        <div class="usage-bar"><span style="width:${pct}%"></span></div>
      </div>
    `;
  }).join("");
}

function renderUsageSessions(sessions) {
  const box = $("usage-sessions");
  if (!sessions.length) {
    box.innerHTML = `<div class="empty">暂无会话记录。开始一次对话后，这里会显示最近的 token 明细。</div>`;
    return;
  }

  box.innerHTML = sessions.map(session => {
    const parts = [
      ["聊", session.chat_total_tokens],
      ["图", session.vision_total_tokens],
      ["摘", session.screen_summary_total_tokens],
      ["忆", session.memory_aggregation_total_tokens],
    ].filter(([, value]) => value > 0)
      .map(([label, value]) => `<span>${label} ${formatNumber(value)}</span>`)
      .join("");
    return `
      <div class="usage-session">
        <div class="usage-session-main">
          <strong>${formatNumber(session.total_tokens)}</strong>
          <span>${escapeHtml(formatDateTime(session.ended_at))}</span>
        </div>
        <div class="usage-session-sub">
          <span>${escapeHtml((session.models || []).join(", ") || "未知模型")}</span>
          <span>${formatNumber(session.record_count)} 条 · ${formatDuration(session.elapsed_ms_total)}</span>
        </div>
        <div class="usage-session-parts">${parts || "<span>无分类明细</span>"}</div>
      </div>
    `;
  }).join("");
}

function renderPetEventLog(logView) {
  const box = $("pet-events");
  if (!box) return;
  const entries = logView?.entries || [];
  if (!entries.length) {
    box.innerHTML = `<div class="empty">暂无宠物事件。事件发送、去重和节流记录会显示在这里。</div>`;
    return;
  }

  box.innerHTML = entries.map(entry => {
    const payload = compactPayload(entry.payload);
    const reason = entry.reason ? `<span>${escapeHtml(entry.reason)}</span>` : "";
    return `
      <div class="pet-event ${escapeAttr(entry.decision)}">
        <div class="pet-event-main">
          <strong>#${formatNumber(entry.seq)} ${escapeHtml(entry.event_type)}</strong>
          <span>${escapeHtml(entry.timestamp)}</span>
        </div>
        <div class="pet-event-sub">
          <span class="pet-event-decision">${escapeHtml(formatPetDecision(entry.decision))}</span>
          ${reason}
        </div>
        <code>${escapeHtml(payload)}</code>
      </div>
    `;
  }).join("");
}

function bindMusicDiagnostics() {
  if (musicDiagnosticsBound) return;
  musicDiagnosticsBound = true;
  const eventApi = window.__TAURI__?.event;
  if (!eventApi?.listen) return;

  eventApi.listen("performance-start", (event) => {
    const payload = event.payload || {};
    if (payload.kind !== "music-reactive") return;
    Object.assign(MUSIC_STATE, {
      status: "active",
      source: payload.source || MUSIC_STATE.source || "-",
      sessionId: payload.session_id || null,
      energy: 0,
      bass: 0,
      onset: false,
      silence: false,
      updatedAt: new Date(),
      error: null,
    });
    renderMusicDiagnostics();
  });

  eventApi.listen("performance-frame", (event) => {
    const payload = event.payload || {};
    if (typeof payload.energy !== "number" && typeof payload.bass !== "number") return;
    Object.assign(MUSIC_STATE, {
      status: "active",
      sessionId: payload.session_id || MUSIC_STATE.sessionId,
      energy: clamp01(payload.energy),
      bass: clamp01(payload.bass),
      onset: Boolean(payload.onset),
      silence: Boolean(payload.silence),
      updatedAt: new Date(),
      error: null,
    });
    scheduleMusicDiagnosticsRender();
  });

  eventApi.listen("performance-stop", (event) => {
    const payload = event.payload || {};
    if (payload.session_id && MUSIC_STATE.sessionId && payload.session_id !== MUSIC_STATE.sessionId) return;
    MUSIC_STATE.status = "stopped";
    MUSIC_STATE.updatedAt = new Date();
    renderMusicDiagnostics();
  });

  eventApi.listen("performance-error", (event) => {
    const payload = event.payload || {};
    Object.assign(MUSIC_STATE, {
      status: "error",
      sessionId: payload.session_id || MUSIC_STATE.sessionId,
      error: payload.message || String(payload.error || "unknown error"),
      updatedAt: new Date(),
    });
    renderMusicDiagnostics();
  });
}

function scheduleMusicDiagnosticsRender() {
  if (musicDiagnosticsRenderTimer) return;
  musicDiagnosticsRenderTimer = setTimeout(() => {
    musicDiagnosticsRenderTimer = null;
    renderMusicDiagnostics();
  }, MUSIC_DIAGNOSTICS_RENDER_MS);
}

async function startMusicDance(source) {
  const command = source === "wasapi" ? "cmd_start_wasapi_music_dance" : "cmd_start_fake_music_dance";
  Object.assign(MUSIC_STATE, {
    status: "starting",
    source,
    error: null,
    updatedAt: new Date(),
  });
  renderMusicDiagnostics();
  try {
    const sessionId = await invoke(command);
    Object.assign(MUSIC_STATE, {
      status: "active",
      source,
      sessionId,
      updatedAt: new Date(),
      error: null,
    });
    renderMusicDiagnostics();
    toast(source === "wasapi" ? "WASAPI 已启动" : "模拟音乐已启动", "ok");
  } catch (e) {
    Object.assign(MUSIC_STATE, {
      status: "error",
      error: String(e),
      updatedAt: new Date(),
    });
    renderMusicDiagnostics();
    toast("音乐启动失败：" + String(e), "err");
  }
}

async function stopMusicDance() {
  try {
    await invoke("cmd_stop_music_dance");
    Object.assign(MUSIC_STATE, {
      status: "stopped",
      updatedAt: new Date(),
    });
    renderMusicDiagnostics();
    toast("音乐响应已停止", "ok");
  } catch (e) {
    MUSIC_STATE.error = String(e);
    MUSIC_STATE.status = "error";
    MUSIC_STATE.updatedAt = new Date();
    renderMusicDiagnostics();
    toast("停止失败：" + String(e), "err");
  }
}

function renderMusicDiagnostics() {
  const boxes = ["music-diagnostics", "music-diagnostics-usage"]
    .map(id => document.getElementById(id))
    .filter(Boolean);
  if (!boxes.length) return;
  const energyPct = Math.round(clamp01(MUSIC_STATE.energy) * 100);
  const bassPct = Math.round(clamp01(MUSIC_STATE.bass) * 100);
  const session = MUSIC_STATE.sessionId || "-";
  const updated = MUSIC_STATE.updatedAt ? MUSIC_STATE.updatedAt.toLocaleTimeString("zh-CN") : "-";
  const error = MUSIC_STATE.error ? `<div class="music-error">${escapeHtml(MUSIC_STATE.error)}</div>` : "";
  const html = `
    <div class="music-status-grid">
      <div class="music-kv"><span>状态</span><strong>${escapeHtml(MUSIC_STATE.status)}</strong></div>
      <div class="music-kv"><span>来源</span><strong>${escapeHtml(MUSIC_STATE.source)}</strong></div>
      <div class="music-kv"><span>会话</span><strong>${escapeHtml(session)}</strong></div>
      <div class="music-kv"><span>更新</span><strong>${escapeHtml(updated)}</strong></div>
    </div>
    <div class="music-meter-row">
      <div class="music-meter-label"><span>能量</span><strong>${energyPct}%</strong></div>
      <div class="music-meter"><span style="width:${energyPct}%"></span></div>
    </div>
    <div class="music-meter-row">
      <div class="music-meter-label"><span>低频</span><strong>${bassPct}%</strong></div>
      <div class="music-meter bass"><span style="width:${bassPct}%"></span></div>
    </div>
    <div class="music-flags">
      <span class="${MUSIC_STATE.onset ? "on" : ""}">起拍</span>
      <span class="${MUSIC_STATE.silence ? "on" : ""}">静音</span>
    </div>
    ${error}
  `;
  boxes.forEach(box => { box.innerHTML = html; });
}

function renderMemoryReview(review) {
  const box = $("memory-review");
  if (!box) return;
  const entries = review?.entries || [];
  updateOverviewMemory(review);
  if (!entries.length) {
    box.innerHTML = `<div class="empty">还没有长期记忆。对话结束后，猫猫会把值得保留的内容放在这里供你审查。</div>`;
    return;
  }

  box.innerHTML = `
    <div class="memory-meta">
      <span>${formatNumber(review.total_entries)} 条记忆</span>
      <span>最近更新 ${escapeHtml(formatDateTime(review.generated_at))}</span>
    </div>
    ${entries.map(entry => {
      const tags = (entry.tags || []).map(tag => `<span>#${escapeHtml(tag)}</span>`).join("");
      const source = entry.source || "unknown";
      const importance = entry.importance == null ? "?" : entry.importance;
      const summary = entry.ai_reply || entry.user_msg || "";
      return `
        <div class="memory-entry">
          <div class="memory-entry-head">
            <div>
              <strong>${escapeHtml(entry.title)}</strong>
              <p>${escapeHtml(summary)}</p>
            </div>
            <button class="icon-btn danger memory-delete" type="button" data-id="${escapeAttr(entry.id)}" aria-label="删除记忆" title="删除记忆">×</button>
          </div>
          <div class="memory-entry-meta">
            <span>${escapeHtml(entry.timestamp)}</span>
            <span>${escapeHtml(source)}</span>
            <span>重要度 ${escapeHtml(importance)}</span>
            ${entry.aggregated ? "<span>已聚合</span>" : ""}
          </div>
          <div class="memory-tags">${tags || "<span>未标记</span>"}</div>
          <details class="memory-detail">
            <summary>查看原文</summary>
            <div class="memory-body">
              <p><b>用户</b>${escapeHtml(entry.user_msg)}</p>
              <p><b>回复</b>${escapeHtml(entry.ai_reply)}</p>
            </div>
          </details>
        </div>
      `;
    }).join("")}
  `;

  box.querySelectorAll(".memory-delete").forEach(btn => {
    btn.addEventListener("click", () => {
      const id = btn.dataset.id;
      if (id && confirm("确定删除这条长期记忆？")) {
        deleteMemoryEntry(id);
      }
    });
  });
}

async function saveAll() {
  try {
    if (dirty.ai) {
      const keyRaw = $("ai-key").value;
      const payload = {
        api_key: keyRaw === "" ? null : keyRaw,
        base_url: $("ai-baseurl").value === "" ? null : $("ai-baseurl").value,
        model: $("ai-model").value === "" ? null : $("ai-model").value,
        max_tokens: $("ai-maxtokens").value === "" ? null : parseInt($("ai-maxtokens").value),
      };
      if (payload.api_key !== null && payload.api_key.trim() === "") {
        toast("API Key 不能只含空白字符", "err");
        return;
      }
      await invoke("cmd_settings_save_ai", { payload });
      clearDirty("ai");
    }
    if (dirty.user) {
      await invoke("cmd_settings_save_user", { payload: collectUser() });
      clearDirty("user");
    }
    if (dirty.actions) {
      await invoke("cmd_settings_save_actions", { payload: collectActions() });
      clearDirty("actions");
    }
    if (dirty.prompts) {
      await invoke("cmd_settings_save_prompts", { payload: collectPrompts() });
      clearDirty("prompts");
    }
    if (dirty.appearance) {
      await invoke("cmd_settings_save_appearance", { payload: collectAppearance() });
      clearDirty("appearance");
    }
    if (dirty.agent_watch) {
      await invoke("cmd_settings_save_agent_watch", { payload: collectAgentWatch() });
      clearDirty("agent_watch");
    }
    await invoke("cmd_settings_apply");
    toast("已保存", "ok");
    await loadSnapshot();
  } catch (e) {
    log("保存失败: " + e);
    toast("保存失败：" + String(e), "err");
  }
}

async function resetCurrent() {
  if (["overview", "about", "usage", "memory"].includes(currentTab)) return;
  if (!confirm(`确定将「${tabLabel(currentTab)}」重置为默认？`)) return;
  try {
    await invoke("cmd_settings_reset", { category: currentTab });
    toast("已重置", "ok");
    await loadSnapshot();
    clearDirty(currentTab);
  } catch (e) {
    toast("重置失败：" + String(e), "err");
  }
}

function tabLabel(t) {
  return ({ ai: "AI 与对话", user: "记忆与画像", actions: "按键与操作", prompts: "提示词", appearance: "外观与行为", "agent-watch": "Agent 看管", agent_watch: "Agent 看管" })[t] || t;
}

async function loadSnapshot() {
  try {
    SNAPSHOT = await invoke("cmd_settings_load");
    renderAi(SNAPSHOT.ai);
    renderUser(SNAPSHOT.user);
    renderActions(SNAPSHOT.actions);
    renderPrompts(SNAPSHOT.prompts);
    renderAppearance(SNAPSHOT.appearance);
    renderAgentWatch(SNAPSHOT.agent_watch);
    renderAbout(SNAPSHOT.about);
    loadUsageDiagnostics();
    loadMemoryReview();
    ["ai", "user", "actions", "prompts", "appearance", "agent_watch"].forEach(clearDirty);
  } catch (e) {
    log("加载失败: " + e);
    toast("加载配置失败：" + String(e), "err");
  }
}

async function tryClose() {
  if (anyDirty()) {
    if (!confirm("有未保存的修改，确定放弃？")) return;
  }
  try { await invoke("cmd_settings_close"); } catch {}
}

function bindGlobal() {
  document.querySelectorAll(".nav-item").forEach(btn => {
    btn.addEventListener("click", () => switchTab(btn.dataset.tab));
  });
  document.querySelectorAll(".quick-link[data-go]").forEach(btn => {
    btn.addEventListener("click", () => switchTab(btn.dataset.go));
  });
  $("btn-close").addEventListener("click", tryClose);
  $("btn-cancel").addEventListener("click", async () => {
    await loadSnapshot();
    toast("已取消修改", "ok");
  });
  $("btn-save").addEventListener("click", saveAll);
  $("btn-reset").addEventListener("click", resetCurrent);
  $("usage-refresh").addEventListener("click", loadUsageDiagnostics);
  $("usage-model").addEventListener("change", () => {
    selectedUsageModel = $("usage-model").value || "__all";
    loadTokenStats();
  });
  $("overview-refresh").addEventListener("click", () => {
    loadUsageDiagnostics();
    loadMemoryReview();
  });
  $("memory-refresh").addEventListener("click", loadMemoryReview);
  $("aw-install").addEventListener("click", async () => {
    try {
      const msg = await invoke("cmd_install_claude_code_hooks");
      toast(msg || "Hook 已安装", "ok");
    } catch (e) {
      toast("安装失败：" + String(e), "err");
    }
  });
  $("aw-install-codex").addEventListener("click", async () => {
    try {
      const msg = await invoke("cmd_install_codex_hooks");
      toast(msg || "Codex Hook 已安装", "ok");
    } catch (e) {
      toast("Codex 安装失败：" + String(e), "err");
    }
  });
  const eventApi = window.__TAURI__?.event;
  if (eventApi?.listen) {
    eventApi.listen("agent-session-update", (event) => {
      if (currentTab === "agent-watch") renderAgentSessions(event.payload);
    });
  }
  $("music-fake").addEventListener("click", () => startMusicDance("fake"));
  $("music-wasapi").addEventListener("click", () => startMusicDance("wasapi"));
  $("music-stop").addEventListener("click", stopMusicDance);
  bindMusicDiagnostics();
  $("ai-key-toggle").addEventListener("click", () => {
    const el = $("ai-key");
    const show = el.type === "password";
    el.type = show ? "text" : "password";
    $("ai-key-toggle").setAttribute("aria-label", show ? "隐藏 API Key" : "显示 API Key");
  });
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") tryClose();
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      saveAll();
    }
  });
}

function escapeHtml(s) {
  return String(s ?? "").replace(/[&<>"']/g, c => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
  })[c]);
}

function escapeAttr(s) { return escapeHtml(s).replace(/"/g, "&quot;"); }

function formatNumber(value) {
  return Number(value || 0).toLocaleString("zh-CN");
}

function compactNumber(value) {
  const num = Number(value || 0);
  if (Math.abs(num) >= 1_000_000) return `${formatFixed(num / 1_000_000, 1)}m`;
  if (Math.abs(num) >= 10_000) return `${formatFixed(num / 1000, 1)}k`;
  return formatNumber(num);
}

function formatMetricPart(value) {
  return typeof value === "number" ? compactNumber(value) : String(value ?? "-");
}

function metricValue(value, unit = "") {
  const suffix = unit ? `<small>${escapeHtml(unit)}</small>` : "";
  return `<span class="metric-main">${escapeHtml(formatMetricPart(value))}${suffix}</span>`;
}

function pairedMetric(leftLabel, leftValue, rightLabel, rightValue) {
  return `
    <span class="metric-pair">
      <span><b>${escapeHtml(formatMetricPart(leftValue))}</b><small>${escapeHtml(leftLabel)}</small></span>
      <span><b>${escapeHtml(formatMetricPart(rightValue))}</b><small>${escapeHtml(rightLabel)}</small></span>
    </span>
  `;
}

function formatFixed(value, digits) {
  return Number(value || 0).toLocaleString("zh-CN", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
}

function formatDuration(ms) {
  const value = Number(ms || 0);
  if (value < 1000) return `${value}ms`;
  if (value < 60_000) return `${(value / 1000).toFixed(1)}s`;
  return `${Math.round(value / 60_000)}m`;
}

function clamp01(value) {
  return Math.max(0, Math.min(1, Number(value) || 0));
}

function formatDateTime(value) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function updateOverviewAppearance(appearance) {
  $("ov-screenshot").textContent = `${formatNumber(appearance?.screenshot_interval_sec ?? 30)} 秒`;
}

function updateOverviewMemory(review) {
  $("ov-memory-total").textContent = formatNumber(review?.total_entries || 0);
  $("ov-memory-time").textContent = review?.generated_at ? `更新于 ${formatDateTime(review.generated_at)}` : "等待记录";
}

function formatPetDecision(value) {
  if (value === "sent") return "已发送";
  if (value === "deduplicated") return "已去重";
  if (value === "throttled") return "已节流";
  if (value === "emit_failed") return "发送失败";
  return value || "-";
}

function compactPayload(payload) {
  if (!payload || payload === null) return "";
  const copy = { ...payload };
  if (typeof copy.body === "string" && copy.body.length > 80) {
    copy.body = copy.body.slice(0, 80) + "...";
  }
  if (typeof copy.text === "string" && copy.text.length > 80) {
    copy.text = copy.text.slice(0, 80) + "...";
  }
  if (typeof copy.speech === "string" && copy.speech.length > 80) {
    copy.speech = copy.speech.slice(0, 80) + "...";
  }
  return JSON.stringify(copy);
}

document.addEventListener("DOMContentLoaded", () => {
  bindGlobal();
  loadSnapshot();
});
