// 8Bit Cat 设置界面逻辑
// - 启动拉取 cmd_settings_load
// - 左侧 tab 切换 + dirty 检测
// - 底部保存/取消/重置，Esc 关闭

const { invoke } = window.__TAURI__.core;

// 按键绑定类型：unbound 代表未绑定（保存时会从 actions.yml 中移除该按键）
const ACTION_TYPES = ["unbound", "launch", "hotkey", "script", "voice", "screenshot"];
const ACTION_TYPE_LABELS = {
  unbound: "未绑定",
  launch: "启动程序",
  hotkey: "按键序列",
  script: "脚本命令",
  voice: "语音触发",
  screenshot: "立即截图",
};

// 全量快照（来自后端）
let SNAPSHOT = null;
// 各分类 dirty 标记
const dirty = { ai: false, actions: false, prompts: false, appearance: false };
// 当前激活 tab
let currentTab = "ai";
let musicDiagnosticsBound = false;
let musicDiagnosticsRenderTimer = null;
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

// ---- 工具 ----

function log(msg) {
  try { invoke("cmd_settings_log", { msg: String(msg) }); } catch {}
}

function toast(text, kind = "ok") {
  const el = document.getElementById("toast");
  el.textContent = text;
  el.classList.remove("hidden", "ok", "err");
  el.classList.add(kind);
  clearTimeout(toast._t);
  toast._t = setTimeout(() => el.classList.add("hidden"), 2200);
}

function $(id) { return document.getElementById(id); }

function markDirty(tab) {
  dirty[tab] = true;
  const nav = document.querySelector(`.nav-item[data-tab="${tab}"]`);
  if (nav) nav.classList.add("dirty");
}
function clearDirty(tab) {
  dirty[tab] = false;
  const nav = document.querySelector(`.nav-item[data-tab="${tab}"]`);
  if (nav) nav.classList.remove("dirty");
}
function anyDirty() { return Object.values(dirty).some(Boolean); }

// ---- Tab 切换 ----

function switchTab(name) {
  currentTab = name;
  document.querySelectorAll(".nav-item").forEach(b => {
    b.classList.toggle("active", b.dataset.tab === name);
  });
  document.querySelectorAll(".tab").forEach(s => {
    s.classList.toggle("hidden", s.dataset.pane !== name);
  });
  if (name === "usage") loadUsageDiagnostics();
}

// ---- 渲染各分类 ----

function renderAi(ai) {
  $("ai-key").value = ai.overlay.api_key || "";
  $("ai-baseurl").value = ai.overlay.base_url || "";
  $("ai-model").value = ai.overlay.model || "";
  $("ai-maxtokens").value = ai.overlay.max_tokens == null ? "" : ai.overlay.max_tokens;

  const eff = ai.effective;
  $("ai-effective").innerHTML =
    `<div class="effective-title">当前生效</div>` +
    `<div class="effective-item"><span>API Key</span><b>${ai.has_effective_key ? "已配置" : "未配置"}</b></div>` +
    `<div class="effective-item"><span>Base URL</span><b title="${escapeAttr(eff.base_url)}">${escapeHtml(eff.base_url)}</b></div>` +
    `<div class="effective-item"><span>模型</span><b title="${escapeAttr(eff.model)}">${escapeHtml(eff.model)}</b></div>` +
    `<div class="effective-item"><span>Max Tokens</span><b>${formatNumber(eff.max_tokens)}</b></div>`;

  ["ai-key", "ai-baseurl", "ai-model", "ai-maxtokens"].forEach(id => {
    $(id).oninput = () => markDirty("ai");
  });
}

function renderActions(actionsView) {
  $("actions-term").value = actionsView.defaults.terminal || "powershell";
  $("actions-win").value = actionsView.defaults.window || "maximized";
  $("actions-term").onchange = () => markDirty("actions");
  $("actions-win").onchange = () => markDirty("actions");

  const list = $("actions-list");
  list.innerHTML = "";

  // 优先按 button_catalog 渲染（覆盖 buttons.yml 里的全部按键）
  const catalog = Array.isArray(SNAPSHOT.button_catalog) ? SNAPSHOT.button_catalog : [];
  if (catalog.length > 0) {
    for (const item of catalog) {
      const def = actionsView.actions[item.name] || null;
      list.appendChild(renderActionItem(item, def));
    }
    // 补充 catalog 之外、但 actions.yml 里已有的自定义按键（若有）
    const catalogNames = new Set(catalog.map(i => i.name));
    Object.keys(actionsView.actions).sort().forEach(key => {
      if (catalogNames.has(key)) return;
      list.appendChild(renderActionItem(
        { name: key, label: "(自定义)", position: "", order: 9999 },
        actionsView.actions[key]
      ));
    });
  } else {
    // 退化：没有 catalog 时按已配置按键渲染
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

function actionSummary(type, def) {
  if (!def || type === "unbound") return "未写入";
  if (type === "launch") return def.program ? `打开 ${def.program}` : "启动程序";
  if (type === "hotkey") return def.command || "按键序列";
  if (type === "script") return def.command || "脚本命令";
  if (type === "voice") return "语音触发";
  if (type === "screenshot") return "立即截图分析";
  return ACTION_TYPE_LABELS[type] || type;
}

function renderActionBody(body, def, onChange = () => {}) {
  body.innerHTML = "";
  const t = def.action_type;
  if (t === "unbound") {
    body.innerHTML = "";
    return;
  }
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
    return row;
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
    if (type === "unbound") return; // 未绑定：不写入 actions.yml
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
    // 键盘热键（可选，所有动作类型都可绑定）
    const kbd = (getVal("kbd") || "").trim();
    if (kbd) def.keyboard_shortcut = kbd;
    actions[key] = def;
  });
  return { defaults, actions };
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

function renderAbout(a) {
  $("about-version").textContent = a.version;
  $("about-settings-path").textContent = a.app_settings_path;
  $("about-actions-hint").textContent = a.actions_yml_hint;
  $("about-prompts-hint").textContent = a.prompts_yml_hint;
}

async function loadUsageDiagnostics() {
  await Promise.all([loadTokenStats(), loadPetEventLog(), loadMemoryReview()]);
  renderMusicDiagnostics();
}

async function loadTokenStats() {
  const status = $("usage-status");
  if (!status) return;
  status.textContent = "读取中...";
  try {
    const stats = await invoke("cmd_get_token_stats");
    renderTokenStats(stats);
    status.textContent = `更新于 ${formatDateTime(stats.generated_at)}`;
  } catch (e) {
    log("加载 token 统计失败: " + e);
    status.textContent = "读取失败：" + String(e);
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
    log("load memory review failed: " + e);
    renderMemoryReview(null);
  }
}

async function deleteMemoryEntry(id) {
  try {
    const review = await invoke("cmd_delete_memory_entry", { id, limit: 20 });
    renderMemoryReview(review);
    toast("Memory deleted", "ok");
  } catch (e) {
    toast("Delete failed: " + String(e), "err");
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
  $("usage-total").textContent = formatNumber(today.total_tokens);
  $("usage-io").textContent = `${formatNumber(today.input_tokens)} / ${formatNumber(today.output_tokens)}`;
  $("usage-cache").textContent = `${formatNumber(today.cache_read_tokens)} / ${formatNumber(today.cache_write_tokens)}`;
  $("usage-records").textContent = formatNumber(today.record_count);
  $("usage-paths").textContent = stats
    ? `${stats.paths.usage_jsonl}\n${stats.paths.sessions_json}`
    : "-";

  renderUsageBreakdown(today);
  renderUsageSessions(stats?.recent_sessions || []);
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
    box.innerHTML = `<div class="empty">暂无会话记录</div>`;
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
          <span>${escapeHtml((session.models || []).join(", ") || "unknown model")}</span>
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
    box.innerHTML = `<div class="empty">暂无宠物事件</div>`;
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
    toast(source === "wasapi" ? "WASAPI started" : "Fake music started", "ok");
  } catch (e) {
    Object.assign(MUSIC_STATE, {
      status: "error",
      error: String(e),
      updatedAt: new Date(),
    });
    renderMusicDiagnostics();
    toast("Music start failed: " + String(e), "err");
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
    toast("Music stopped", "ok");
  } catch (e) {
    MUSIC_STATE.error = String(e);
    MUSIC_STATE.status = "error";
    MUSIC_STATE.updatedAt = new Date();
    renderMusicDiagnostics();
    toast("Music stop failed: " + String(e), "err");
  }
}

function renderMusicDiagnostics() {
  const box = $("music-diagnostics");
  if (!box) return;
  const energyPct = Math.round(clamp01(MUSIC_STATE.energy) * 100);
  const bassPct = Math.round(clamp01(MUSIC_STATE.bass) * 100);
  const session = MUSIC_STATE.sessionId || "-";
  const updated = MUSIC_STATE.updatedAt ? MUSIC_STATE.updatedAt.toLocaleTimeString("zh-CN") : "-";
  const error = MUSIC_STATE.error ? `<div class="music-error">${escapeHtml(MUSIC_STATE.error)}</div>` : "";
  box.innerHTML = `
    <div class="music-status-grid">
      <div class="music-kv"><span>Status</span><strong>${escapeHtml(MUSIC_STATE.status)}</strong></div>
      <div class="music-kv"><span>Source</span><strong>${escapeHtml(MUSIC_STATE.source)}</strong></div>
      <div class="music-kv"><span>Session</span><strong>${escapeHtml(session)}</strong></div>
      <div class="music-kv"><span>Updated</span><strong>${escapeHtml(updated)}</strong></div>
    </div>
    <div class="music-meter-row">
      <div class="music-meter-label"><span>Energy</span><strong>${energyPct}%</strong></div>
      <div class="music-meter"><span style="width:${energyPct}%"></span></div>
    </div>
    <div class="music-meter-row">
      <div class="music-meter-label"><span>Bass</span><strong>${bassPct}%</strong></div>
      <div class="music-meter bass"><span style="width:${bassPct}%"></span></div>
    </div>
    <div class="music-flags">
      <span class="${MUSIC_STATE.onset ? "on" : ""}">onset</span>
      <span class="${MUSIC_STATE.silence ? "on" : ""}">silence</span>
    </div>
    ${error}
  `;
}

function renderMemoryReview(review) {
  const box = $("memory-review");
  if (!box) return;
  const entries = review?.entries || [];
  if (!entries.length) {
    box.innerHTML = `<div class="empty">No long-term memory entries</div>`;
    return;
  }

  box.innerHTML = `
    <div class="memory-meta">
      <span>${formatNumber(review.total_entries)} total</span>
      <span>${escapeHtml(formatDateTime(review.generated_at))}</span>
    </div>
    ${entries.map(entry => {
      const tags = (entry.tags || []).map(tag => `<span>#${escapeHtml(tag)}</span>`).join("");
      const source = entry.source || "unknown";
      const importance = entry.importance == null ? "?" : entry.importance;
      return `
        <div class="memory-entry">
          <div class="memory-entry-head">
            <strong>${escapeHtml(entry.title)}</strong>
            <button class="btn small danger memory-delete" type="button" data-id="${escapeAttr(entry.id)}">Delete</button>
          </div>
          <div class="memory-entry-meta">
            <span>${escapeHtml(entry.id)}</span>
            <span>${escapeHtml(entry.timestamp)}</span>
            <span>${escapeHtml(source)}</span>
            <span>importance ${escapeHtml(importance)}</span>
            ${entry.aggregated ? "<span>aggregated</span>" : ""}
          </div>
          <div class="memory-tags">${tags || "<span>untagged</span>"}</div>
          <div class="memory-body">
            <p>${escapeHtml(entry.user_msg)}</p>
            <p>${escapeHtml(entry.ai_reply)}</p>
          </div>
        </div>
      `;
    }).join("")}
  `;

  box.querySelectorAll(".memory-delete").forEach(btn => {
    btn.addEventListener("click", () => {
      const id = btn.dataset.id;
      if (id && confirm("Delete this memory entry?")) {
        deleteMemoryEntry(id);
      }
    });
  });
}

// ---- 保存 / 重置 ----

async function saveAll() {
  try {
    if (dirty.ai) {
      const keyRaw = $("ai-key").value;
      // 非空校验：如果用户明确输入了（非空白），允许保存；空串视为"清除覆盖"
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
    await invoke("cmd_settings_apply");
    toast("已保存 ✓", "ok");
    // 重新拉快照，刷新 effective
    await loadSnapshot();
  } catch (e) {
    log("保存失败: " + e);
    toast("保存失败：" + String(e), "err");
  }
}

async function resetCurrent() {
  if (currentTab === "about" || currentTab === "usage") return;
  if (!confirm(`确定将「${tabLabel(currentTab)}」重置为默认？`)) return;
  try {
    await invoke("cmd_settings_reset", { category: currentTab });
    toast("已重置 ✓", "ok");
    await loadSnapshot();
    clearDirty(currentTab);
  } catch (e) {
    toast("重置失败：" + String(e), "err");
  }
}

function tabLabel(t) {
  return ({ ai: "AI 模型", actions: "按键绑定", prompts: "Prompt", appearance: "外观行为" })[t] || t;
}

// ---- 主流程 ----

async function loadSnapshot() {
  try {
    SNAPSHOT = await invoke("cmd_settings_load");
    renderAi(SNAPSHOT.ai);
    renderActions(SNAPSHOT.actions);
    renderPrompts(SNAPSHOT.prompts);
    renderAppearance(SNAPSHOT.appearance);
    renderAbout(SNAPSHOT.about);
    loadUsageDiagnostics();
    ["ai", "actions", "prompts", "appearance"].forEach(clearDirty);
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
  $("btn-close").addEventListener("click", tryClose);
  $("btn-cancel").addEventListener("click", async () => {
    await loadSnapshot();
    toast("已取消修改", "ok");
  });
  $("btn-save").addEventListener("click", saveAll);
  $("btn-reset").addEventListener("click", resetCurrent);
  $("usage-refresh").addEventListener("click", loadUsageDiagnostics);
  const memoryRefresh = $("memory-refresh");
  if (memoryRefresh) memoryRefresh.addEventListener("click", loadMemoryReview);
  const musicFake = $("music-fake");
  if (musicFake) musicFake.addEventListener("click", () => startMusicDance("fake"));
  const musicWasapi = $("music-wasapi");
  if (musicWasapi) musicWasapi.addEventListener("click", () => startMusicDance("wasapi"));
  const musicStop = $("music-stop");
  if (musicStop) musicStop.addEventListener("click", stopMusicDance);
  bindMusicDiagnostics();
  $("ai-key-toggle").addEventListener("click", () => {
    const el = $("ai-key");
    el.type = el.type === "password" ? "text" : "password";
  });
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") tryClose();
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      saveAll();
    }
  });
}

// ---- utils ----
function escapeHtml(s) {
  return String(s ?? "").replace(/[&<>"']/g, c => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
  })[c]);
}
function escapeAttr(s) { return escapeHtml(s).replace(/"/g, "&quot;"); }

function formatNumber(value) {
  return Number(value || 0).toLocaleString("zh-CN");
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

// 启动
document.addEventListener("DOMContentLoaded", () => {
  bindGlobal();
  loadSnapshot();
});
