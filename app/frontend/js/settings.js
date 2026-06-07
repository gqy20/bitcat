// BitCat 设置界面逻辑
// - 启动拉取 cmd_settings_load
// - 左侧 tab 切换 + dirty 检测
// - 底部保存/取消/重置，Esc 关闭

const invoke = window.__TAURI__?.core?.invoke || mockInvoke;

const ACTION_TYPES = ["unbound", "launch", "hotkey", "script", "voice", "screenshot"];
const PET_ASSET_PRESETS = [
  { value: "", label: "默认", group: "推荐" },
  { value: "/__fixtures__/pets/hackmark", label: "Hackmark", group: "推荐" },
  { value: "/__fixtures__/pets/padlet", label: "Padlet", group: "推荐" },
  { value: "/__fixtures__/pets/piggy", label: "Piggy", group: "推荐" },
  { value: "/__fixtures__/pets/cat", label: "Cat", group: "推荐" },
  { value: "/__fixtures__/pets/status", label: "Status", group: "终端状态" },
  { value: "/__fixtures__/pets/core", label: "Core", group: "终端状态" },
  { value: "/__fixtures__/pets/stacky", label: "Stacky", group: "终端状态" },
  { value: "/__fixtures__/pets/bsod", label: "BSOD", group: "特殊状态" },
  { value: "/__fixtures__/pets/null-signal", label: "Null Signal", group: "特殊状态" },
  { value: "/__fixtures__/pets/byte-bun", label: "Byte Bun", group: "角色" },
  { value: "/__fixtures__/pets/mossbot", label: "Mossbot", group: "角色" },
  { value: "/__fixtures__/pets/moonbit", label: "Moonbit", group: "角色" },
  { value: "/__fixtures__/pets/sparkle", label: "Sparkle", group: "角色" },
  { value: "/__fixtures__/pets/dewey", label: "Dewey", group: "角色" },
  { value: "/__fixtures__/pets/fireball", label: "Fireball", group: "角色" },
  { value: "/__fixtures__/pets/rocky", label: "Rocky", group: "角色" },
  { value: "/__fixtures__/pets/seedy", label: "Seedy", group: "角色" },
];
const PET_ASSET_PIGGY = "/__fixtures__/pets/piggy";
const PET_ASSET_DEFAULT_PREVIEW = PET_ASSET_PIGGY;
const PET_ASSET_PRESET_VALUES = new Set(PET_ASSET_PRESETS.map(item => item.value).filter(Boolean));
const petAssetPreviewCache = new Map();
let selectedPetAssetPreset = "";
const ACTION_TYPE_LABELS = {
  unbound: "未绑定",
  launch: "启动程序",
  hotkey: "按键序列",
  script: "脚本命令",
  voice: "语音触发",
  screenshot: "立即截图",
};

let SNAPSHOT = null;
const dirty = { ai: false, user: false, actions: false, prompts: false, appearance: false, permissions: false, agent_watch: false };
let currentTab = "overview";
let selectedUsageModel = "__all";
let agentWatchCopyBound = false;
let agentWatchTimer = null;

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
        context: "正在打磨 BitCat 的桌面体验。",
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
        reminder_personalizer: { preamble: "" },
      },
      appearance: {
        always_on_top: false,
        default_collapsed: false,
        tts_enabled: true,
        notification_sound_enabled: true,
        notification_sound_reminder: true,
        notification_sound_agent_watch: true,
        notification_sound_skip_agent_tts: true,
        reminder_ai_personalization_enabled: false,
        reminder_ai_timeout_ms: 3000,
        global_shortcut: "CommandOrControl+Alt+Space",
        screenshot_interval_sec: 30,
        screenshot_show_bubble: true,
        camera_observation_enabled: false,
        camera_observation_interval_sec: 30,
        camera_save_frames: false,
        pet_asset_url: "",
      },
      storage: {
        settings: { data_dir: null, app_data_dir: null },
        paths: {
          data_dir: "C:\\Users\\you\\.bitcat",
          app_data_dir: "C:\\Users\\you\\AppData\\Roaming\\bitcat",
          default_data_dir: "C:\\Users\\you\\.bitcat",
          default_app_data_dir: "C:\\Users\\you\\AppData\\Roaming\\bitcat",
        },
      },
      permissions: {
        onboarding_completed: false,
        steam_demo_mode: false,
        allow_screenshot_observation: true,
        allow_camera_observation: false,
        allow_shell_tool: false,
        allow_read_file_tool: false,
        allow_clipboard_tool: false,
        allow_foreground_tool: false,
        allow_launch_program_tool: false,
        allow_hotkey_tool: false,
        allow_agent_watch_remote: false,
        diagnostics_enabled: true,
      },
      agent_watch: {
        enabled: false,
        away_nudge_enabled: true,
        first_nudge_after_sec: 30,
        repeat_nudge_after_min: 8,
        waiting_alert: true,
        done_alert: true,
        use_tts: false,
        remote_view_enabled: true,
        remote_install_enabled: true,
      },
      about: {
        version: "preview",
        app_settings_path: "~/.bitcat/app_settings.json",
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
        usage_jsonl: "~/.bitcat/logs/token_usage.jsonl",
        sessions_json: "~/.bitcat/logs/token_sessions.json",
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
  if (name === "reminders") loadReminders();
  if (name === "agent-watch") startAgentWatchRefresh();
  else stopAgentWatchRefresh();
}

function renderAi(ai) {
  $("ai-key").value = ai.overlay.api_key || "";
  $("ai-baseurl").value = ai.overlay.base_url || "";
  $("ai-model").value = ai.overlay.model || "";
  $("ai-maxtokens").value = ai.overlay.max_tokens == null ? "" : ai.overlay.max_tokens;

  const eff = ai.effective;
  $("ai-key-current").textContent = ai.has_effective_key ? "已配置" : "未配置";
  $("ai-baseurl-current").textContent = eff.base_url || "";
  $("ai-baseurl-current").title = eff.base_url || "";
  $("ai-model-current").textContent = eff.model || "";
  $("ai-model-current").title = eff.model || "";
  $("ai-maxtokens-current").textContent = eff.max_tokens == null ? "" : formatNumber(eff.max_tokens);
  renderOverviewNotices(ai);
  $("ov-ai-model").textContent = eff.model || "-";
  $("ov-ai-key").textContent = ai.has_effective_key ? "已配置" : "未配置";

  ["ai-key", "ai-baseurl", "ai-model", "ai-maxtokens"].forEach(id => {
    $(id).oninput = () => markDirty("ai");
  });
}

function renderOverviewNotices(ai) {
  const box = $("overview-notices");
  if (!box) return;
  const notices = [];
  if (!ai.has_effective_key) {
    notices.push(["API Key 未配置", "对话不可用"]);
  }
  if (!ai.effective?.model) {
    notices.push(["模型未配置", ""]);
  }
  if (!notices.length) {
    box.innerHTML = `<div class="empty compact">一切正常</div>`;
    return;
  }
  box.innerHTML = notices.map(([title, body]) => `
    <div class="notice-item">
      <strong>${escapeHtml(title)}</strong>
      ${body ? `<span>${escapeHtml(body)}</span>` : ""}
    </div>
  `).join("");
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
  $("p-reminder-personalizer").value = p.reminder_personalizer?.preamble || "";
  $("p-mem-max").value = p.memory.max_entries;
  $("p-mem-ctx").value = p.memory.max_context_chars;
  $("p-ss-interval").value = p.screen_summary.interval_min;

  ["p-agent","p-vision","p-vision-multi","p-reminder-personalizer","p-mem-max","p-mem-ctx","p-ss-interval"].forEach(id => {
    $(id).oninput = () => markDirty("prompts");
  });
}

function collectPrompts() {
  const p = structuredClone(SNAPSHOT.prompts);
  p.agent.preamble = $("p-agent").value;
  p.vision.prompt = $("p-vision").value;
  p.vision.prompt_multi = $("p-vision-multi").value;
  p.reminder_personalizer = p.reminder_personalizer || {};
  p.reminder_personalizer.preamble = $("p-reminder-personalizer").value;
  p.memory.max_entries = parseInt($("p-mem-max").value) || p.memory.max_entries;
  p.memory.max_context_chars = parseInt($("p-mem-ctx").value) || p.memory.max_context_chars;
  p.screen_summary.interval_min = parseInt($("p-ss-interval").value) || p.screen_summary.interval_min;
  return p;
}

function renderAppearance(a) {
  $("a-top").checked = a.always_on_top;
  $("a-collapsed").checked = a.default_collapsed;
  $("a-tts").checked = a.tts_enabled;
  $("a-notify-sound").checked = a.notification_sound_enabled !== false;
  $("a-notify-sound-reminder").checked = a.notification_sound_reminder !== false;
  $("a-notify-sound-agent").checked = a.notification_sound_agent_watch !== false;
  $("a-notify-sound-skip-tts").checked = a.notification_sound_skip_agent_tts !== false;
  $("a-reminder-ai").checked = !!a.reminder_ai_personalization_enabled;
  $("a-reminder-ai-timeout").value = a.reminder_ai_timeout_ms ?? 3000;
  $("a-shortcut").value = a.global_shortcut;
  $("a-ss-interval").value = a.screenshot_interval_sec ?? 30;
  $("a-ss-bubble").checked = a.screenshot_show_bubble !== false;
  $("a-camera-enabled").checked = !!a.camera_observation_enabled;
  $("a-camera-save").checked = !!a.camera_save_frames;
  renderStorage(SNAPSHOT?.storage);
  renderPetAssetPicker();
  renderPetAssetChoice(a.pet_asset_url || "");
  updateOverviewAppearance(a);

  ["a-top","a-collapsed","a-tts","a-notify-sound","a-notify-sound-reminder","a-notify-sound-agent","a-notify-sound-skip-tts","a-reminder-ai","a-ss-bubble","a-camera-enabled","a-camera-save"].forEach(id => { $(id).onchange = () => markDirty("appearance"); });
  ["a-shortcut","a-ss-interval","a-reminder-ai-timeout","a-pet-asset","a-storage-data","a-storage-app-data"].forEach(id => { $(id).oninput = () => markDirty("appearance"); });
}

function renderStorage(storage) {
  const settings = storage?.settings || {};
  const paths = storage?.paths || {};
  $("a-storage-data").value = settings.data_dir || "";
  $("a-storage-data").placeholder = paths.default_data_dir || "";
  $("a-storage-app-data").value = settings.app_data_dir || "";
  $("a-storage-app-data").placeholder = paths.default_app_data_dir || "";
}

function collectStorage() {
  return {
    data_dir: $("a-storage-data").value.trim() || null,
    app_data_dir: $("a-storage-app-data").value.trim() || null,
  };
}

function collectAppearance() {
  const rawInterval = parseInt($("a-ss-interval").value, 10);
  const interval = Number.isFinite(rawInterval) ? Math.min(3600, Math.max(5, rawInterval)) : 30;
  const rawReminderAiTimeout = parseInt($("a-reminder-ai-timeout").value, 10);
  const reminderAiTimeout = Number.isFinite(rawReminderAiTimeout) ? Math.min(10000, Math.max(500, rawReminderAiTimeout)) : 3000;
  return {
    always_on_top: $("a-top").checked,
    default_collapsed: $("a-collapsed").checked,
    tts_enabled: $("a-tts").checked,
    notification_sound_enabled: $("a-notify-sound").checked,
    notification_sound_reminder: $("a-notify-sound-reminder").checked,
    notification_sound_agent_watch: $("a-notify-sound-agent").checked,
    notification_sound_skip_agent_tts: $("a-notify-sound-skip-tts").checked,
    reminder_ai_personalization_enabled: $("a-reminder-ai").checked,
    reminder_ai_timeout_ms: reminderAiTimeout,
    global_shortcut: $("a-shortcut").value.trim() || "CommandOrControl+Alt+Space",
    screenshot_interval_sec: interval,
    screenshot_show_bubble: $("a-ss-bubble").checked,
    camera_observation_enabled: $("a-camera-enabled").checked,
    camera_observation_interval_sec: interval,
    camera_save_frames: $("a-camera-save").checked,
    pet_asset_url: collectPetAssetUrl(),
  };
}

function renderPermissions(p = {}) {
  $("perm-onboarding-completed").checked = !!p.onboarding_completed;
  $("perm-steam-demo").checked = !!p.steam_demo_mode;
  $("perm-screenshot").checked = p.allow_screenshot_observation !== false;
  $("perm-camera").checked = !!p.allow_camera_observation;
  $("perm-shell").checked = !!p.allow_shell_tool;
  $("perm-read-file").checked = !!p.allow_read_file_tool;
  $("perm-clipboard").checked = !!p.allow_clipboard_tool;
  $("perm-foreground").checked = !!p.allow_foreground_tool;
  $("perm-launch").checked = !!p.allow_launch_program_tool;
  $("perm-hotkey").checked = !!p.allow_hotkey_tool;
  $("perm-agent-remote").checked = !!p.allow_agent_watch_remote;
  $("perm-diagnostics").checked = p.diagnostics_enabled !== false;
  $("perm-onboarding").classList.toggle("hidden", !!p.onboarding_completed);

  [
    "perm-onboarding-completed",
    "perm-steam-demo",
    "perm-screenshot",
    "perm-camera",
    "perm-shell",
    "perm-read-file",
    "perm-clipboard",
    "perm-foreground",
    "perm-launch",
    "perm-hotkey",
    "perm-agent-remote",
    "perm-diagnostics",
  ].forEach(id => { $(id).onchange = () => markDirty("permissions"); });

  $("perm-complete").onclick = () => {
    $("perm-onboarding-completed").checked = true;
    markDirty("permissions");
    toast("已标记为了解，保存后生效", "ok");
  };
}

function collectPermissions() {
  return {
    onboarding_completed: $("perm-onboarding-completed").checked,
    steam_demo_mode: $("perm-steam-demo").checked,
    allow_screenshot_observation: $("perm-screenshot").checked,
    allow_camera_observation: $("perm-camera").checked,
    allow_shell_tool: $("perm-shell").checked,
    allow_read_file_tool: $("perm-read-file").checked,
    allow_clipboard_tool: $("perm-clipboard").checked,
    allow_foreground_tool: $("perm-foreground").checked,
    allow_launch_program_tool: $("perm-launch").checked,
    allow_hotkey_tool: $("perm-hotkey").checked,
    allow_agent_watch_remote: $("perm-agent-remote").checked,
    diagnostics_enabled: $("perm-diagnostics").checked,
  };
}

function renderPetAssetChoice(value) {
  const normalized = normalizePetAssetUrl(value);
  if (!normalized) {
    selectedPetAssetPreset = "";
    $("a-pet-asset").value = "";
  } else if (PET_ASSET_PRESET_VALUES.has(normalized)) {
    selectedPetAssetPreset = normalized;
    $("a-pet-asset").value = normalized;
  } else {
    selectedPetAssetPreset = "__custom";
    $("a-pet-asset").value = normalized;
  }
  updatePetAssetCustomVisibility();
  updatePetAssetPickerSelection();
}

function applyPetAssetPreset(value) {
  selectedPetAssetPreset = value;
  if (value === "__custom") {
    if (!$("a-pet-asset").value.trim()) $("a-pet-asset").value = PET_ASSET_PIGGY;
  } else {
    $("a-pet-asset").value = value;
  }
  updatePetAssetCustomVisibility();
  updatePetAssetPickerSelection();
}

function updatePetAssetCustomVisibility() {
  $("a-pet-asset").classList.toggle("hidden", selectedPetAssetPreset !== "__custom");
}

function collectPetAssetUrl() {
  const preset = selectedPetAssetPreset;
  if (!preset) return null;
  if (preset !== "__custom") return normalizePetAssetUrl(preset) || null;
  return normalizePetAssetUrl($("a-pet-asset").value) || null;
}

function normalizePetAssetUrl(value) {
  return String(value || "").trim().replace(/\/+$/, "");
}

function renderPetAssetPicker() {
  const picker = $("a-pet-asset-picker");
  if (!picker) return;
  picker.innerHTML = "";
  let currentGroup = null;
  let groupEl = null;
  let gridEl = null;

  function ensureGroup(group) {
    if (group === currentGroup && gridEl) return gridEl;
    currentGroup = group;
    groupEl = document.createElement("div");
    groupEl.className = "pet-asset-group";
    const title = document.createElement("div");
    title.className = "pet-asset-group-title";
    title.textContent = group || "其他";
    gridEl = document.createElement("div");
    gridEl.className = "pet-asset-grid";
    groupEl.append(title, gridEl);
    picker.appendChild(groupEl);
    return gridEl;
  }

  for (const preset of PET_ASSET_PRESETS) {
    const grid = ensureGroup(preset.group);
    const card = document.createElement("button");
    card.type = "button";
    card.className = "pet-asset-card";
    card.dataset.value = preset.value;
    card.setAttribute("role", "option");
    card.setAttribute("title", preset.label);
    card.onkeydown = handlePetAssetCardKeydown;

    const canvas = document.createElement("canvas");
    canvas.className = "pet-asset-thumb";
    canvas.width = 38;
    canvas.height = 38;
    canvas.setAttribute("aria-hidden", "true");

    const label = document.createElement("span");
    label.textContent = preset.label;
    card.append(canvas, label);
    card.onclick = () => {
      applyPetAssetPreset(preset.value);
      markDirty("appearance");
    };
    grid.appendChild(card);
    renderPetAssetPreview(canvas, preset.value);
  }

  const customGrid = ensureGroup("其他");
  const custom = document.createElement("button");
  custom.type = "button";
  custom.className = "pet-asset-card";
  custom.dataset.value = "__custom";
  custom.setAttribute("role", "option");
  custom.setAttribute("title", "自定义地址");
  custom.onkeydown = handlePetAssetCardKeydown;
  custom.innerHTML = `<canvas class="pet-asset-thumb" width="38" height="38" aria-hidden="true"></canvas><span>自定义地址</span>`;
  custom.onclick = () => {
    applyPetAssetPreset("__custom");
    markDirty("appearance");
  };
  customGrid.appendChild(custom);
  drawCustomPetAssetPreview(custom.querySelector("canvas"));
  updatePetAssetPickerSelection();
}

function handlePetAssetCardKeydown(event) {
  if (event.key !== "ArrowRight" && event.key !== "ArrowLeft" && event.key !== "ArrowDown" && event.key !== "ArrowUp") {
    return;
  }
  const cards = Array.from($("a-pet-asset-picker")?.querySelectorAll(".pet-asset-card") || []);
  const index = cards.indexOf(event.currentTarget);
  if (index < 0) return;
  event.preventDefault();
  const delta = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
  cards[(index + delta + cards.length) % cards.length].focus();
}

function updatePetAssetPickerSelection() {
  const picker = $("a-pet-asset-picker");
  if (!picker) return;
  const value = selectedPetAssetPreset;
  picker.querySelectorAll(".pet-asset-card").forEach(card => {
    const selected = card.dataset.value === value;
    card.classList.toggle("selected", selected);
    card.setAttribute("aria-selected", selected ? "true" : "false");
    card.tabIndex = selected ? 0 : -1;
  });
}

async function renderPetAssetPreview(canvas, value) {
  if (!canvas) return;
  const baseUrl = normalizePetAssetUrl(value || PET_ASSET_DEFAULT_PREVIEW);
  if (!baseUrl) return;
  try {
    const asset = await loadPetAssetPreview(baseUrl);
    drawPetAssetPreview(canvas, asset);
  } catch (error) {
    drawCustomPetAssetPreview(canvas);
  }
}

async function loadPetAssetPreview(baseUrl) {
  if (petAssetPreviewCache.has(baseUrl)) return petAssetPreviewCache.get(baseUrl);
  const promise = (async () => {
    const manifest = await fetch(`${baseUrl}/manifest.json`).then(res => {
      if (!res.ok) throw new Error(`manifest ${res.status}`);
      return res.json();
    });
    const image = new Image();
    image.decoding = "sync";
    image.src = `${baseUrl}/${manifest.sprite?.image || "spritesheet.webp"}`;
    await image.decode();
    return { manifest, image };
  })();
  petAssetPreviewCache.set(baseUrl, promise);
  return promise;
}

function drawPetAssetPreview(canvas, asset) {
  const ctx = canvas.getContext("2d");
  const manifest = asset.manifest || {};
  const sprite = manifest.sprite || {};
  const fw = sprite.frameWidth || 1;
  const fh = sprite.frameHeight || 1;
  const columns = sprite.columns || 1;
  const frame = Number.isInteger(manifest.mini?.frame)
    ? manifest.mini.frame
    : (manifest.states?.idle?.frames?.[0]?.sprite || 0);
  const sx = (frame % columns) * fw;
  const sy = Math.floor(frame / columns) * fh;
  const scale = Math.min(32 / fw, 32 / fh);
  const dw = Math.max(1, Math.round(fw * scale));
  const dh = Math.max(1, Math.round(fh * scale));
  const dx = Math.floor((canvas.width - dw) / 2);
  const dy = Math.floor((canvas.height - dh) / 2);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.imageSmoothingEnabled = manifest.render?.pixelated === true ? false : true;
  ctx.drawImage(asset.image, sx, sy, fw, fh, dx, dy, dw, dh);
}

function drawCustomPetAssetPreview(canvas) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "rgba(124,255,178,0.18)";
  ctx.strokeStyle = "rgba(124,255,178,0.7)";
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.roundRect(8, 8, 22, 22, 6);
  ctx.fill();
  ctx.stroke();
  ctx.fillStyle = "#dce3ee";
  ctx.font = "700 18px system-ui";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("+", 19, 19);
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
  $("aw-remote-view").checked = cfg.remote_view_enabled !== false;
  $("aw-remote-install").checked = cfg.remote_install_enabled !== false;
  ["aw-enabled","aw-away","aw-waiting","aw-done","aw-tts","aw-remote-view","aw-remote-install"].forEach(id => { $(id).onchange = () => markDirty("agent_watch"); });
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
    remote_view_enabled: $("aw-remote-view").checked,
    remote_install_enabled: $("aw-remote-install").checked,
  };
}

function renderAbout(a) {
  $("about-version").textContent = a.version;
  $("about-settings-path").textContent = a.app_settings_path;
  $("about-data-dir").textContent = SNAPSHOT?.storage?.paths?.data_dir || "-";
  $("about-app-data-dir").textContent = SNAPSHOT?.storage?.paths?.app_data_dir || "-";
  $("about-actions-hint").textContent = a.actions_yml_hint;
  $("about-prompts-hint").textContent = a.prompts_yml_hint;
}

async function loadUsageDiagnostics() {
  await Promise.all([loadTokenStats(), loadPetEventLog(), loadResourceUsage(), loadPointsState()]);
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

async function loadReminders() {
  const status = $("reminder-status");
  if (status) status.textContent = "读取中...";
  try {
    const review = await invoke("cmd_get_reminders", { includeInactive: true });
    renderReminders(review);
  } catch (e) {
    log("加载提醒失败: " + e);
    renderReminders(null);
    if (status) status.textContent = `读取失败：${String(e)}`;
  }
}

async function loadAgentSessions() {
  const status = $("aw-status");
  if (status) status.textContent = "读取中...";
  try {
    const snapshot = await invoke("cmd_get_agent_sessions");
    renderAgentSessions(snapshot);
    loadRemoteDevices();
    if (status) status.textContent = snapshot?.generated_at_ms ? "已更新" : "等待状态";
  } catch (e) {
    log("加载 Agent 会话失败: " + e);
    renderAgentSessions(null);
    if (status) status.textContent = "读取失败";
  }
}

function startAgentWatchRefresh() {
  loadRemoteInstallCommand();
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

async function loadRemoteInstallCommand() {
  const code = $("aw-remote-command");
  const watchUrl = $("aw-remote-watch-url");
  const status = $("aw-remote-status");
  if (!code) return;
  try {
    const info = await invoke("cmd_get_remote_install_cmd");
    code.textContent = info.install_command || `bash scripts/remote-install.sh --host ${info.local_ip} --port ${info.port}`;
    code.dataset.copyValue = code.textContent;
    if (watchUrl) {
      const urls = Array.isArray(info.endpoints) && info.endpoints.length
        ? info.endpoints.map(endpoint => `${endpoint.display_label || endpoint.label} /watch`).join("  ")
        : Array.isArray(info.watch_urls) && info.watch_urls.length ? info.watch_urls.join("  ") : info.watch_url;
      watchUrl.textContent = urls || `http://${info.local_ip}:${info.view_port}/watch`;
      watchUrl.dataset.copyValue = Array.isArray(info.watch_urls) && info.watch_urls.length ? info.watch_urls.join("  ") : watchUrl.textContent;
    }
    if (status) {
      const ips = Array.isArray(info.endpoints) && info.endpoints.length
        ? info.endpoints.map(endpoint => endpoint.display_label || endpoint.label).join(", ")
        : Array.isArray(info.local_ips) && info.local_ips.length ? info.local_ips.join(", ") : info.local_ip;
      status.textContent = `${ips} -> ${info.port} / ${info.view_port}`;
    }
  } catch (e) {
    code.textContent = "无法生成远程安装命令";
    code.dataset.copyValue = "";
    if (watchUrl) watchUrl.textContent = "无法生成看管地址";
    if (watchUrl) watchUrl.dataset.copyValue = "";
    if (status) status.textContent = "失败";
    log("remote install command failed: " + e);
  }
}

async function loadRemoteDevices() {
  const box = $("aw-remote-devices");
  if (!box) return;
  try {
    const devices = await invoke("cmd_list_remote_devices");
    if (!devices?.length) {
      box.innerHTML = `<div class="empty-note">暂无远程设备。</div>`;
      return;
    }
    box.innerHTML = devices.map(device => `
      <div class="remote-device ${device.stale ? "stale" : ""}">
        <strong>${escapeHtml(device.machine)}</strong>
        <span>${device.active_count || 0} active / ${device.session_count || 0} sessions</span>
        <small>${device.last_updated_at_ms ? new Date(device.last_updated_at_ms).toLocaleTimeString() : ""}</small>
      </div>
    `).join("");
  } catch (e) {
    box.innerHTML = `<div class="empty-note">远程设备状态不可用。</div>`;
  }
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
    box.innerHTML = `<div class="empty-note">暂无 Agent 会话。</div>`;
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
        ${session.machine ? `<small>${escapeHtml(session.machine)}</small>` : ""}
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
  $("ov-usage-total").textContent = compactNumber(today.total_tokens);

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
    box.innerHTML = `<div class="empty">暂无会话记录。</div>`;
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
    box.innerHTML = `<div class="empty">暂无宠物事件。</div>`;
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

function bindAgentWatchCopyActions() {
  if (agentWatchCopyBound) return;
  agentWatchCopyBound = true;
  $("aw-remote-copy").addEventListener("click", async () => {
    try {
      const command = $("aw-remote-command");
      await navigator.clipboard.writeText(command.dataset.copyValue || command.textContent || "");
      toast("远程安装命令已复制", "ok");
    } catch (e) {
      toast("复制失败：" + String(e), "err");
    }
  });
  $("aw-remote-url-copy").addEventListener("click", async () => {
    try {
      const watchUrl = $("aw-remote-watch-url");
      await navigator.clipboard.writeText(watchUrl.dataset.copyValue || watchUrl.textContent || "");
      toast("看管地址已复制", "ok");
    } catch (e) {
      toast("复制失败：" + String(e), "err");
    }
  });
}

function renderMemoryReview(review) {
  const box = $("memory-review");
  if (!box) return;
  const entries = review?.entries || [];
  updateOverviewMemory(review);
  if (!entries.length) {
    box.innerHTML = `<div class="empty">暂无长期记忆。</div>`;
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

function renderReminders(review) {
  const box = $("reminder-review");
  const status = $("reminder-status");
  if (!box) return;
  const entries = review?.entries || [];
  if (status) {
    status.textContent = review
      ? `${formatNumber(review.active_count || 0)} 个活跃 / ${formatNumber(review.total_entries || 0)} 个总计`
      : "不可用";
  }
  if (!entries.length) {
    box.innerHTML = `<div class="empty">暂无提醒。</div>`;
    return;
  }
  const eventsPath = review?.events_path
    ? `<div class="reminder-log-path">日志 <code title="${escapeAttr(review.events_path)}">${escapeHtml(review.events_path)}</code></div>`
    : "";
  const storePath = review?.store_path
    ? `<div class="reminder-log-path">数据 <code title="${escapeAttr(review.store_path)}">${escapeHtml(review.store_path)}</code></div>`
    : "";
  box.innerHTML = `
    <div class="memory-meta">
      <span>${formatNumber(review.active_count || 0)} 个活跃提醒</span>
      <span>更新于 ${escapeHtml(formatDateTime(review.generated_at))}</span>
    </div>
    ${storePath}
    ${eventsPath}
    ${entries.map(entry => {
      const description = reminderDescription(entry);
      return `
      <div class="reminder-entry ${escapeAttr(entry.status)}">
        <div class="memory-entry-head">
          <div>
            <strong>${escapeHtml(entry.title || "提醒")}</strong>
            ${description ? `<p>${escapeHtml(description)}</p>` : ""}
          </div>
          <span class="reminder-status-pill">${escapeHtml(reminderStatusLabel(entry.status))}</span>
        </div>
        <div class="memory-entry-meta">
          <span>${escapeHtml(formatReminderSchedule(entry))}</span>
          <span>下次 ${escapeHtml(formatDateTime(entry.next_fire_at))}</span>
          <span>触发 ${formatNumber(entry.fire_count || 0)} 次</span>
          ${entry.last_fired_at ? `<span>上次 ${escapeHtml(formatDateTime(entry.last_fired_at))}</span>` : ""}
        </div>
        <div class="reminder-actions">
          <button class="btn small reminder-complete" type="button" data-id="${escapeAttr(entry.id)}">完成</button>
          <button class="btn small ghost reminder-snooze" type="button" data-id="${escapeAttr(entry.id)}">10 分钟后</button>
          <button class="btn small danger reminder-cancel" type="button" data-id="${escapeAttr(entry.id)}">取消</button>
          <button class="icon-btn danger reminder-delete" type="button" data-id="${escapeAttr(entry.id)}" aria-label="删除提醒" title="删除提醒">🗑</button>
        </div>
      </div>`;
    }).join("")}
  `;
  box.querySelectorAll(".reminder-complete").forEach(btn => {
    btn.addEventListener("click", () => reminderAction("cmd_complete_reminder", btn.dataset.id));
  });
  box.querySelectorAll(".reminder-snooze").forEach(btn => {
    btn.addEventListener("click", () => reminderAction("cmd_snooze_reminder", btn.dataset.id, { minutes: 10 }));
  });
  box.querySelectorAll(".reminder-cancel").forEach(btn => {
    btn.addEventListener("click", () => {
      if (confirm("确定取消这个提醒？")) reminderAction("cmd_cancel_reminder", btn.dataset.id);
    });
  });
  box.querySelectorAll(".reminder-delete").forEach(btn => {
    btn.addEventListener("click", () => {
      if (confirm("确定彻底删除这个提醒？")) reminderAction("cmd_delete_reminder", btn.dataset.id);
    });
  });
}

async function reminderAction(command, id, extra = {}) {
  if (!id) return;
  try {
    const review = await invoke(command, { id, ...extra });
    renderReminders(review);
    toast("提醒已更新", "ok");
  } catch (e) {
    toast("提醒操作失败：" + String(e), "err");
  }
}

function reminderStatusLabel(status) {
  if (status === "active") return "活跃";
  if (status === "done") return "完成";
  if (status === "cancelled") return "已取消";
  return status || "未知";
}

function reminderDescription(entry) {
  const message = String(entry?.message || "").trim();
  return message;
}

function formatReminderSchedule(entry) {
  const raw = entry?.schedule_label || "";
  if (!raw) return "未设置计划";
  if (raw.startsWith("一次")) {
    return "一次";
  }
  return raw.replace(
    /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?/g,
    match => formatDateTime(match)
  );
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
      await invoke("cmd_settings_save_storage", { payload: collectStorage() });
      clearDirty("appearance");
    }
    if (dirty.permissions) {
      await invoke("cmd_settings_save_permissions", { payload: collectPermissions() });
      clearDirty("permissions");
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
  if (["overview", "about", "usage", "memory", "reminders"].includes(currentTab)) return;
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
  return ({
    ai: "对话方式",
    user: "记得什么",
    actions: "互动方式",
    prompts: "提示词",
    appearance: "猫猫表现",
    permissions: "发布与权限",
    "agent-watch": "陪你盯任务",
    agent_watch: "陪你盯任务",
    reminders: "不会忘的事",
    usage: "运行记录",
    about: "关于",
  })[t] || t;
}

async function loadSnapshot() {
  try {
    SNAPSHOT = await invoke("cmd_settings_load");
    renderAi(SNAPSHOT.ai);
    renderUser(SNAPSHOT.user);
    renderActions(SNAPSHOT.actions);
    renderPrompts(SNAPSHOT.prompts);
    renderAppearance(SNAPSHOT.appearance);
    renderPermissions(SNAPSHOT.permissions);
    renderAgentWatch(SNAPSHOT.agent_watch);
    renderAbout(SNAPSHOT.about);
    loadUsageDiagnostics();
    loadMemoryReview();
    loadReminders();
    if (!SNAPSHOT.permissions?.onboarding_completed) {
      switchTab("permissions");
    }
    ["ai", "user", "actions", "prompts", "appearance", "permissions", "agent_watch"].forEach(clearDirty);
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
  $("usage-model").addEventListener("change", () => {
    selectedUsageModel = $("usage-model").value || "__all";
    loadTokenStats();
  });
  $("overview-refresh").addEventListener("click", () => {
    loadUsageDiagnostics();
    loadMemoryReview();
    loadReminders();
  });
  $("memory-refresh").addEventListener("click", loadMemoryReview);
  $("reminder-refresh").addEventListener("click", loadReminders);
  $("aw-install").addEventListener("click", async () => {
    try {
      const msg = await invoke("cmd_install_claude_code_hooks");
      toast(msg || "Hook 已检查并修复", "ok");
    } catch (e) {
      toast("修复失败：" + String(e), "err");
    }
  });
  $("aw-install-codex").addEventListener("click", async () => {
    try {
      const msg = await invoke("cmd_install_codex_hooks");
      toast(msg || "Codex Hook 已检查并修复", "ok");
    } catch (e) {
      toast("Codex 修复失败：" + String(e), "err");
    }
  });
  const eventApi = window.__TAURI__?.event;
  if (eventApi?.listen) {
    eventApi.listen("agent-session-update", (event) => {
      if (currentTab === "agent-watch") renderAgentSessions(event.payload);
    });
    eventApi.listen("reminders-updated", () => {
      if (currentTab === "reminders") loadReminders();
    });
  }
  bindAgentWatchCopyActions();
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
  if (Math.abs(num) >= 1_000_000) return `${formatFixed(num / 1_000_000, 1)}M`;
  if (Math.abs(num) >= 10_000) return `${formatFixed(num / 1000, 1)}K`;
  return formatNumber(num);
}

function compactMetricNumber(value) {
  const num = Number(value || 0);
  const abs = Math.abs(num);
  if (abs >= 1_000_000) return `${formatFixed(num / 1_000_000, 1)}M`;
  if (abs >= 1_000) return `${formatFixed(num / 1000, 1)}K`;
  return formatNumber(Math.round(num));
}

function metricSizeClass(text) {
  const len = String(text || "").replace(/\s+/g, "").length;
  if (len >= 9) return " metric-value-tight";
  if (len >= 7) return " metric-value-compact";
  return "";
}

function formatMetricPart(value) {
  return typeof value === "number" ? compactMetricNumber(value) : String(value ?? "-");
}

function metricValue(value, unit = "") {
  const text = formatMetricPart(value);
  const suffix = unit ? `<small>${escapeHtml(unit)}</small>` : "";
  return `<span class="metric-main${metricSizeClass(text)}">${escapeHtml(text)}${suffix}</span>`;
}

function pairedMetric(leftLabel, leftValue, rightLabel, rightValue) {
  const leftText = formatMetricPart(leftValue);
  const rightText = formatMetricPart(rightValue);
  return `
    <span class="metric-pair">
      <span title="${escapeAttr(leftLabel)}">
        <b class="${metricSizeClass(leftText).trim()}">${escapeHtml(leftText)}</b>
      </span>
      <span title="${escapeAttr(rightLabel)}">
        <b class="${metricSizeClass(rightText).trim()}">${escapeHtml(rightText)}</b>
      </span>
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
  $("ov-memory-time").textContent = review?.generated_at ? "" : "等待";
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
  const summary = formatPetPayload(payload);
  if (summary) return summary;
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

function formatPetPayload(payload) {
  const type = payload.type || "";
  if (type === "notify") {
    const kind = formatPetKind(payload.kind);
    const parts = [`通知：${kind}`];
    if (payload.body) parts.push(shortText(repairMojibake(payload.body), 48));
    if (payload.ttl_ms) parts.push(`${formatDuration(payload.ttl_ms)} 后恢复`);
    if (payload.refresh) parts.push("刷新现有状态");
    return parts.join(" · ");
  }
  if (type === "react") {
    const parts = [`反应：${formatPetMood(payload.mood)}`];
    if (payload.speech) parts.push(shortText(repairMojibake(payload.speech), 48));
    if (payload.ttl_ms) parts.push(`${formatDuration(payload.ttl_ms)} 后恢复`);
    return parts.join(" · ");
  }
  if (type === "set_mode") return `模式：${formatPetMode(payload.mode)}`;
  if (type === "show_bubble") return `气泡：${shortText(repairMojibake(payload.text || ""), 72)}`;
  if (type === "play_dance") return `舞蹈：${payload.name || "-"}`;
  if (type === "walk_to") return `移动到 x=${Number(payload.x || 0).toFixed(0)}`;
  if (type === "clear_notification") return payload.kind ? `清理通知：${formatPetKind(payload.kind)}` : "清理全部通知";
  if (type === "exit") return "退出宠物";
  return "";
}

function formatPetKind(value) {
  const map = {
    ai_thinking: "AI 思考",
    ai_writing: "AI 回复",
    tool_preparing: "工具准备",
    tool_running: "工具运行",
    tool_blocked: "工具被阻止",
    tool_failed: "工具失败",
    listening: "正在听写",
    screenshot_observing: "截图观察",
  };
  return map[value] || value || "-";
}

function formatPetMood(value) {
  const map = {
    idle: "待机",
    happy: "开心",
    confused: "困惑",
    focused: "专注",
    caring: "关心",
    excited: "兴奋",
    sleepy: "困倦",
  };
  return map[value] || value || "-";
}

function formatPetMode(value) {
  const map = {
    idle: "待机",
    sleep: "睡眠",
    game_play: "游戏",
  };
  return map[value] || value || "-";
}

function shortText(value, limit) {
  const text = String(value || "").replace(/\s+/g, " ").trim();
  if (text.length <= limit) return text;
  return `${text.slice(0, limit)}...`;
}

function repairMojibake(value) {
  if (value === "姝ｅ湪瑙傚療灞忓箷...") return "正在观察屏幕...";
  return value;
}

// ─── 积分与成就系统渲染 ───

const POINTS_CATEGORY_LABELS = {
  Chat: "对话", Memory: "记忆", Routine: "日常",
  Fun: "娱乐", Observation: "观察", Bond: "互动", Daily: "每日",
};

async function loadPointsState() {
  try {
    const view = await invoke("cmd_get_points_state");
    renderPointsLevel(view.state);
    renderPointsBreakdown(view.state);
    renderAchievements(view.achievements, view.state);
    renderPointsEvents(view.recent_events);
  } catch (e) {
    log("加载积分状态失败: " + e);
  }
}

function renderPointsLevel(state) {
  const el = (id) => document.getElementById(id);
  const lv = el("points-level");
  const title = el("points-level-title");
  const total = el("points-total");
  const fill = el("points-exp-fill");
  const expText = el("points-exp-text");
  const streak = el("points-streak");
  const longestStreak = el("points-longest-streak");

  if (lv) lv.textContent = state.level || 1;
  if (title) title.textContent = state.level_title || "-";
  if (total) total.textContent = formatNumber(state.total_points || 0);

  const expIn = state.experience_in_current || 0;
  const expNext = state.experience_to_next || 1;
  const pct = expNext > 0 ? Math.min(100, Math.round((expIn / expNext) * 100)) : 100;
  if (fill) fill.style.width = pct + "%";
  if (expText) expText.textContent = `${formatNumber(expIn)} / ${formatNumber(expNext)}`;
  if (streak) streak.textContent = state.current_streak_days || 0;
  if (longestStreak) longestStreak.textContent = state.longest_streak_days || 0;
}

function renderPointsBreakdown(state) {
  const container = document.getElementById("points-breakdown");
  if (!container) return;

  const cats = state.categories || {};

  const items = [
    ["Chat", "chats", cats.chats || 0],
    ["Memory", "memories", cats.memories || 0],
    ["Routine", "reminders_completed", cats.reminders_completed || 0],
    ["Fun", "games_played", (cats.games_played || 0) + (cats.games_won || 0)],
    ["Observation", "screenshots", (cats.screenshots || 0) + (cats.camera_obs || 0)],
    ["Bond", "praises", cats.praises || 0],
    ["Daily", "login_days", cats.login_days || 0],
  ];

  container.innerHTML = items
    .filter(([, , v]) => v > 0)
    .map(
      ([key, , value]) =>
        `<div class="points-cat-item">
          <span class="points-cat-value">${value}</span>
          <span class="points-cat-label">${POINTS_CATEGORY_LABELS[key] || key}</span>
        </div>`
    )
    .join("") || '<div class="points-empty">暂无成长记录</div>';
}

function renderAchievements(achievements, state) {
  const grid = document.getElementById("achievements-grid");
  const countEl = document.getElementById("achievement-count");
  if (!grid) return;

  achievements = Array.isArray(achievements) ? achievements : [];
  const unlockedIds = new Set(state.achievements || []);
  const unlockedCount = achievements.filter((a) => a.unlocked).length;

  if (countEl) countEl.textContent = unlockedCount;

  if (achievements.length === 0) {
    grid.innerHTML = '<div class="points-empty">暂无成长记录</div>';
    return;
  }

  grid.innerHTML = achievements
    .map((a) => {
      const cls = a.unlocked ? "achievement-badge unlocked" : "achievement-badge locked";
      const icon = a.unlocked || !a.hidden ? a.icon : "?";
      return `<div class="${cls}" title="${escapeHtml(a.description)}">
        <span class="achievement-icon">${icon}</span>
        <span class="achievement-name">${escapeHtml(a.name)}</span>
        ${a.unlocked ? `<span class="achievement-bonus">+${a.points_reward}</span>` : ""}
      </div>`;
    })
    .join("");
}

function renderPointsEvents(events) {
  const box = document.getElementById("points-events");
  if (!box) return;

  if (!events || events.length === 0) {
    box.innerHTML =
      '<div class="points-empty">暂无成长记录</div>';
    return;
  }

  box.innerHTML = events
    .map((ev) => {
      // 后端 serde rename: kind → event_kind, points → points_awarded
      const kindLabel = eventKindLabel(ev.event_kind);
      const pts = ev.points_awarded;
      return `<div class="points-event">
        <span class="points-event-kind">${kindLabel}</span>
        <strong>+${pts}</strong>
        <span></span>
        <span class="points-event-time">${formatDateTime(ev.timestamp)}</span>
      </div>`;
    })
    .join("");
}

/// 将后端 PointsEventKind 枚举名映射为中文显示标签。
function eventKindLabel(kind) {
  const map = {
    ChatCompleted: "对话完成",
    VoiceChat: "语音对话",
    MemoryCreated: "记忆创建",
    ReminderCreated: "创建提醒",
    ReminderCompleted: "完成提醒",
    DancePerformed: "观看舞蹈",
    GamePlayed: "游戏一局",
    GameWon: "游戏胜利",
    ScreenshotObserved: "截图观察",
    CameraObserved: "摄像头观察",
    PetPraised: "夸奖宠物",
    DailyLogin: "每日登录",
  };
  return map[kind] || kind || "-";
}

document.addEventListener("DOMContentLoaded", () => {
  bindGlobal();
  loadSnapshot();
});

if (typeof window !== "undefined") {
  window.__settingsTest = {
    formatReminderSchedule,
    reminderDescription,
    renderReminders,
  };
}
