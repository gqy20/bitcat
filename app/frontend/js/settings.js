// BitCat 设置界面逻辑
// - 启动拉取 cmd_settings_load
// - 左侧 tab 切换 + dirty 检测
// - 底部保存/取消/重置，Esc 关闭

const CANVAS_UI_FONT = typeof getComputedStyle === "function" ? getComputedStyle(document.documentElement).getPropertyValue("--font-ui").trim() || "monospace" : "monospace";

const invoke = window.__TAURI__?.core?.invoke || mockInvoke;

const ACTION_TYPES = ["unbound", "launch", "hotkey", "script", "voice", "screenshot"];
const PET_ASSET_PRESETS = [
  { value: "", label: "默认", group: "推荐" },
  { value: "/__fixtures__/pets/cat-tabby", label: "狸花猫", group: "推荐" },
  { value: "/__fixtures__/pets/cat-calico", label: "三花猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-siamese", label: "暹罗猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-tuxedo", label: "燕尾服猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-black", label: "黑猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-white", label: "白猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-ginger", label: "橘猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-gray", label: "灰猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-cream", label: "奶油猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-blue-gray", label: "蓝灰猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-cow", label: "奶牛猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-tortie", label: "玳瑁猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-ragdoll", label: "布偶猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-snowshoe", label: "雪鞋猫", group: "猫咪" },
  { value: "/__fixtures__/pets/cat-lilac", label: "丁香猫", group: "猫咪" },
];
const PET_ASSET_DEFAULT = "/__fixtures__/pets/cat-tabby";
const PET_ASSET_DEFAULT_PREVIEW = PET_ASSET_DEFAULT;
const PET_ASSET_PRESET_VALUES = new Set(PET_ASSET_PRESETS.map(item => item.value).filter(Boolean));
const petAssetPreviewCache = new Map();
let selectedPetAssetPreset = "";
const PET_PREVIEW_STATES = [
  ["idle", "安静待着"], ["happy", "开心"], ["walk", "走两步"],
  ["sleep", "打个盹"], ["talk", "说话中"], ["focused", "认真思考"],
  ["confused", "有点困惑"], ["gamewin", "赢啦"],
];
const petPreview = { asset: null, raf: 0, animation: null, index: 0, lastFrame: -1, gesture: null, suppressClick: false };
let previewReducedMotion = null;
const ACTION_TYPE_LABELS = {
  unbound: "未绑定",
  launch: "启动程序",
  hotkey: "发送按键序列",
  script: "脚本命令",
  voice: "语音触发",
  screenshot: "立即截图",
};

let SNAPSHOT = null;
let connectionRevision = 0;
let connectionBusy = false;
let clearSavedKey = false;
let mockConnectionAi = null;
// 浏览器预览用的连接器安装状态；pi 初始未接入，便于预览「修复」按钮。
const mockConnectorInstalled = { claude_code: true, codex: true, pi: false, opencode: true };
let connectorStatuses = null;
let connectorBusy = false;
let latestAgentSnapshot = null;
const dirty = { ai: false, user: false, actions: false, prompts: false, appearance: false, permissions: false, agent_watch: false };
let currentTab = "home";
let currentExpertPage = "prompts";
let selectedUsageModel = "__all";
let agentWatchCopyBound = false;
let agentWatchTimer = null;
let pointsSnakeRaf = null;
let pointsSnakeLastFrame = 0;
let pointsSnakePath = [];
let pointsSnakeOffset = 0;
let pointsSnakeLength = 0;

async function mockInvoke(command, args = {}) {
  if (command === "cmd_settings_test_ai") {
    return { status: "preview_only", elapsed_ms: 0 };
  }
  if (command === "cmd_settings_save_ai") {
    const draft = args.payload;
    const previous = mockConnectionAi || { has_effective_key: true, has_saved_key: false };
    mockConnectionAi = {
      overlay: { base_url: draft.base_url, model: draft.model, max_tokens: draft.max_tokens },
      effective: { base_url: draft.base_url, model: draft.model, max_tokens: draft.max_tokens || 256000 },
      has_effective_key: draft.api_key ? true : draft.clear_saved_key ? !previous.has_saved_key : previous.has_effective_key,
      has_saved_key: draft.clear_saved_key ? false : !!draft.api_key || previous.has_saved_key,
    };
    return null;
  }
  if (command === "cmd_settings_load") {
    return {
      ai: mockConnectionAi || {
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
        // 预览样例：A 键预置一个绑定，便于核对「摘要一行 + 点击展开」样式。
        actions: {
          A: {
            type: "launch",
            program: "D:\\tools\\obs.exe",
            args: "",
            workdir: "",
            terminal: true,
            keyboard_shortcut: "",
          },
        },
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
        earnings: {
          monthly_salary_cents: 1500000,
          work_start_minutes: 540,
          work_end_minutes: 1080,
          workdays_per_month: 21.75,
        },
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
        onboarding_completed: true,
        steam_demo_mode: false,
        // 预览默认已开启屏幕观察，便于核对首页摘要与开关同步。
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
      // 预览样例：贴近真实形状的近期活动，便于核对单行聚合样式。
      recent_sessions: [
        { ended_at: new Date(Date.now() - 60_000).toISOString(), models: ["MiniMax-M3"], record_count: 2, elapsed_ms_total: 8400, total_tokens: 5667, vision_total_tokens: 5667 },
        { ended_at: new Date(Date.now() - 120_000).toISOString(), models: ["MiniMax-M3"], record_count: 1, elapsed_ms_total: 6600, total_tokens: 2781, vision_total_tokens: 2781 },
        { ended_at: new Date(Date.now() - 180_000).toISOString(), models: ["MiniMax-M3"], record_count: 1, elapsed_ms_total: 6400, total_tokens: 2673, vision_total_tokens: 2673 },
        { ended_at: new Date(Date.now() - 240_000).toISOString(), models: ["MiniMax-M3"], record_count: 1, elapsed_ms_total: 3800, total_tokens: 2966, vision_total_tokens: 2966 },
      ],
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
  if (command === "cmd_get_points_state") {
    return {
      state: {
        total_points: 42,
        level: 2,
        level_title: "熟悉",
        experience_in_current: 42,
        experience_to_next: 80,
        current_streak_days: 3,
        longest_streak_days: 5,
        categories: {
          chats: 18,
          memories: 4,
          reminders_completed: 3,
          games_played: 2,
          screenshots: 11,
          praises: 4,
          login_days: 3,
        },
      },
      achievements: [
        { name: "初次对话", icon: "💬", description: "完成第一次对话", unlocked: true, hidden: false, points_reward: 10 },
        { name: "观察员", icon: "🖥️", description: "屏幕观察累计 10 次", unlocked: true, hidden: false, points_reward: 15 },
        { name: "好伙伴", icon: "🐾", description: "连续陪伴 3 天", unlocked: true, hidden: false, points_reward: 20 },
        { name: "话痨", icon: "📣", description: "单日对话 20 轮", unlocked: false, hidden: false, points_reward: 30 },
        { name: "记忆守护", icon: "💾", description: "长期记忆 50 条", unlocked: false, hidden: false, points_reward: 40 },
        { name: "提醒达人", icon: "⏰", description: "完成 10 个提醒", unlocked: false, hidden: true, points_reward: 50 },
      ],
      recent_events: [
        { event_kind: "ChatCompleted", points_awarded: 5, timestamp: new Date().toISOString() },
        { event_kind: "ScreenshotObserved", points_awarded: 3, timestamp: new Date().toISOString() },
        { event_kind: "MemoryCreated", points_awarded: 4, timestamp: new Date().toISOString() },
      ],
    };
  }
  if (command === "cmd_get_agent_sessions") {
    const now = Date.now();
    return {
      sessions: [
        {
          session_id: "ses_preview_claude",
          source: "claude_code",
          status: "working",
          status_label: "正在处理",
          workspace_name: "bitcat",
          workspace: "~/workspace/project/2609/bitcat",
          machine: "preview",
          tool_name: "",
          user_prompt_preview: "预览：整理编程助手看管的连接列表",
          updated_at_ms: now - 90_000,
          age_sec: 90,
          tokens_in: 1200,
          tokens_out: 340,
        },
        {
          session_id: "ses_preview_pi",
          source: "pi",
          status: "done",
          status_label: "已完成",
          workspace_name: "docs",
          workspace: "~/workspace/project/2609/docs",
          machine: "preview",
          tool_name: "",
          user_prompt_preview: "预览：审阅 road map 产品视角一节",
          updated_at_ms: now - 3 * 60 * 60 * 1000,
          age_sec: 3 * 60 * 60,
          tokens_in: 800,
          tokens_out: 150,
        },
      ],
      primary: null,
      generated_at_ms: now,
      monitor_port: 8787,
      view_port: 8788,
      event_count: 128,
      last_event_at_ms: now - 90_000,
      log_dir: "~/.bitcat/logs/agent_watch",
    };
  }
  if (command === "cmd_agent_connectors_status") {
    return [
      { source: "claude_code", installed: mockConnectorInstalled.claude_code },
      { source: "codex", installed: mockConnectorInstalled.codex },
      { source: "pi", installed: mockConnectorInstalled.pi },
      { source: "opencode", installed: mockConnectorInstalled.opencode },
    ];
  }
  if (command === "cmd_screen_time_summary") {
    return { enabled: true, today_minutes: 18, week_minutes: 96 };
  }
  if (command === "cmd_earnings_summary") {
    return { enabled: true, today_cents: 4250, coins: 45, coins_emitted: 42 };
  }
  if (command === "cmd_repair_connector") {
    const source = String(args?.source || "");
    if (source in mockConnectorInstalled) mockConnectorInstalled[source] = true;
    return `预览：${window.AgentSources.label(source)} 连接脚本已写入`;
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
  // 错误是三段式文案，2.2 秒读不完；成功提示快消失。点击可提前关掉。
  toast._t = setTimeout(() => el.classList.add("hidden"), kind === "err" ? 5500 : 2200);
  el.onclick = () => {
    clearTimeout(toast._t);
    el.classList.add("hidden");
  };
}

function confirmDialog(options = {}) {
  const layer = $("confirm-layer");
  const title = $("confirm-title");
  const message = $("confirm-message");
  const ok = $("confirm-ok");
  const cancel = $("confirm-cancel");
  if (!layer || !title || !message || !ok || !cancel) {
    return Promise.resolve(window.confirm(options.message || options.title || "确认操作？"));
  }

  title.textContent = options.title || "确认操作";
  message.textContent = options.message || "";
  ok.textContent = options.okText || "确认";
  cancel.textContent = options.cancelText || "取消";
  ok.classList.toggle("danger", options.tone !== "primary");

  layer.classList.remove("hidden");
  layer.setAttribute("aria-hidden", "false");
  window.setTimeout(() => ok.focus(), 0);

  return new Promise((resolve) => {
    const finish = (value) => {
      layer.classList.add("hidden");
      layer.setAttribute("aria-hidden", "true");
      ok.removeEventListener("click", onOk);
      cancel.removeEventListener("click", onCancel);
      layer.removeEventListener("click", onLayer);
      window.removeEventListener("keydown", onKey);
      resolve(value);
    };
    const onOk = () => finish(true);
    const onCancel = () => finish(false);
    const onLayer = (event) => {
      if (event.target === layer) finish(false);
    };
    const onKey = (event) => {
      if (event.key === "Escape") finish(false);
      if (event.key === "Enter") finish(true);
    };
    ok.addEventListener("click", onOk);
    cancel.addEventListener("click", onCancel);
    layer.addEventListener("click", onLayer);
    window.addEventListener("keydown", onKey);
  });
}

function confirmDialogOpen() {
  return !$("confirm-layer")?.classList.contains("hidden");
}

function $(id) { return document.getElementById(id); }

// ─── 4+1 分区导航 ───
// 用户分区按四个疑问组织；专家模式内部再分五个子页。
// dirty / reset 的 category 仍是后端的保存类别（ai/user/actions/...），与分区导航解耦。

// category → 用户分区导航项
const DIRTY_NAV_TAB = {
  ai: "cost",
  user: "memory",
  appearance: "companion",
  permissions: "home",
  actions: "expert",
  prompts: "expert",
  agent_watch: "expert",
};

// category → 专家模式子页
const DIRTY_EXPERT_PAGE = {
  prompts: "prompts",
  actions: "actions",
  agent_watch: "agent-watch",
};

function updateSaveIndicator() {
  $("btn-save")?.classList.toggle("dirty", anyDirty());
}

function markDirty(tab) {
  dirty[tab] = true;
  updateSaveIndicator();
  const navTab = DIRTY_NAV_TAB[tab] || tab;
  const nav = document.querySelector(`.nav-item[data-tab="${navTab}"]`);
  if (nav) nav.classList.add("dirty");
  const expertPage = DIRTY_EXPERT_PAGE[tab];
  if (expertPage) {
    const sub = document.querySelector(`.expert-subnav-item[data-expert="${expertPage}"]`);
    if (sub) sub.classList.add("dirty");
  }
}

function clearDirty(tab) {
  dirty[tab] = false;
  updateSaveIndicator();
  const navTab = DIRTY_NAV_TAB[tab] || tab;
  const nav = document.querySelector(`.nav-item[data-tab="${navTab}"]`);
  if (nav) nav.classList.remove("dirty");
  const expertPage = DIRTY_EXPERT_PAGE[tab];
  if (expertPage) {
    const sub = document.querySelector(`.expert-subnav-item[data-expert="${expertPage}"]`);
    if (sub) sub.classList.remove("dirty");
  }
}

function anyDirty() { return Object.values(dirty).some(Boolean); }

function switchTab(name) {
  if (name !== "companion") stopPetPreview();
  currentTab = name;
  document.querySelectorAll(".nav > .nav-item").forEach(b => {
    b.classList.toggle("active", b.dataset.tab === name);
    if (b.dataset.tab === name) b.setAttribute("aria-current", "page");
    else b.removeAttribute("aria-current");
  });
  document.querySelectorAll(".pane > .tab").forEach(s => {
    s.classList.toggle("hidden", s.dataset.pane !== name);
  });
  // 分区切换回到顶部：避免停留在上个分区的滚动深度
  const pane = document.querySelector(".pane");
  if (pane) pane.scrollTop = 0;
  if (name === "home") {
    loadUsageDiagnostics();
    loadMemoryReview();
    loadReminders();
  } else if (name === "memory") {
    loadMemoryReview();
  } else if (name === "cost") {
    loadTokenStats();
  } else if (name === "companion") {
    loadReminders();
    loadPointsState();
  } else if (name === "expert") {
    switchExpertPage(currentExpertPage);
    return;
  }
  stopAgentWatchRefresh();
}

function switchExpertPage(name) {
  currentExpertPage = name;
  document.querySelectorAll(".expert-subnav-item").forEach(b => {
    b.classList.toggle("active", b.dataset.expert === name);
  });
  document.querySelectorAll(".expert-page").forEach(s => {
    s.classList.toggle("hidden", s.dataset.expertPane !== name);
  });
  // 子页切换回到顶部，避免停留在上一子页的滚动深度
  const pane = document.querySelector(".pane");
  if (pane) pane.scrollTop = 0;
  if (name === "agent-watch") {
    startAgentWatchRefresh();
  } else {
    stopAgentWatchRefresh();
  }
  if (name === "diagnostics") {
    loadPetEventLog();
    loadResourceUsage();
  }
}

function setConnectionEditor(open) {
  $("connection-editor").classList.toggle("hidden", !open);
  $("ai-edit").setAttribute("aria-expanded", String(open));
  $("ai-edit").textContent = open ? "正在编辑" : "修改连接";
}

function setConnectionBusy(busy) {
  connectionBusy = busy;
  $("connection-fields").disabled = busy;
  ["ai-test", "ai-save", "ai-edit", "ai-cancel", "btn-save", "btn-cancel", "btn-reset"].forEach(id => {
    if ($(id)) $(id).disabled = busy;
  });
}

function setConnectionStatus(state, text, detail = "") {
  $("connection-status").dataset.state = state;
  $("connection-status").textContent = text;
  // 检测通过时右上角已是结论，正文不再重复一句"已收到回复"；
  // 失败原因和浏览器预览提示仍保留明细。
  const shown = state === "verified" ? "" : detail;
  $("ai-test-result").textContent = shown;
  $("ai-test-result").classList.toggle("hidden", !shown);
  $("ai-test-result").dataset.state = state;
}

function updateConnectionKeyState() {
  const saved = !!SNAPSHOT?.ai?.has_saved_key;
  $("ai-clear-key").hidden = !saved;
  $("ai-clear-key").textContent = clearSavedKey ? "保留本机密钥" : "移除本机密钥";
  $("ai-clear-note").classList.toggle("hidden", !clearSavedKey);
  $("ai-key-current").textContent = clearSavedKey ? "待移除" : saved ? "已保存在本机" : SNAPSHOT?.ai?.has_effective_key ? "使用外部配置" : "尚未配置";
}

function invalidateConnection() {
  connectionRevision += 1;
  setConnectionStatus("unverified", "有修改 · 尚未检测");
  $("ov-ai-key").dataset.state = "missing";
  $("ov-ai-key").title = "连接配置有修改，尚未检测";
  $("ov-ai-key").setAttribute("aria-label", "连接配置有修改，尚未检测");
  markDirty("ai");
}

function renderAi(ai) {
  connectionRevision += 1;
  clearSavedKey = false;
  const eff = ai.effective;
  $("ai-key").value = "";
  $("ai-key").type = "password";
  $("ai-key-toggle").setAttribute("aria-label", "显示新输入的密钥");
  $("ai-baseurl").value = ai.overlay.base_url || eff.base_url || "https://api.anthropic.com";
  $("ai-model").value = ai.overlay.model || eff.model || "";
  $("ai-maxtokens").value = ai.overlay.max_tokens ?? "";
  $("ai-maxtokens-current").textContent = `当前上限 ${formatNumber(eff.max_tokens)}`;
  const official = /^https:\/\/api\.anthropic\.com(?:\/v1(?:\/messages)?)?\/?$/.test($("ai-baseurl").value);
  $("ai-provider").value = official ? "official" : "custom";
  $("ai-custom-endpoint").classList.toggle("hidden", official);
  $("ai-service-name").textContent = official ? "Anthropic" : "自定义兼容服务";
  $("ai-current-model").textContent = eff.model || "尚未选择模型";
  $("ai-current-model").title = eff.model || "";
  updateConnectionKeyState();
  setConnectionStatus("unverified", ai.has_effective_key ? "已配置 · 尚未检测" : "尚未配置");
  setConnectionEditor(!ai.has_effective_key);
  renderOverviewNotices(ai);
  $("ov-ai-model").textContent = eff.model ? eff.model.replace(/-\d{8}$/, "") : "-";
  $("ov-ai-model").title = eff.model || "";
  const connection = $("ov-ai-key");
  const connectionLabel = ai.has_effective_key ? "已配置，尚未检测" : "尚未配置密钥";
  connection.dataset.state = "missing";
  connection.setAttribute("aria-label", connectionLabel);
  connection.title = connectionLabel;

  ["ai-key", "ai-baseurl", "ai-model", "ai-maxtokens"].forEach(id => {
    $(id).oninput = () => {
      if (id === "ai-key" && $(id).value) { clearSavedKey = false; updateConnectionKeyState(); }
      invalidateConnection();
    };
  });
}

function collectConnectionDraft() {
  const base = $("ai-provider").value === "official" ? "https://api.anthropic.com" : $("ai-baseurl").value.trim();
  let url;
  try { url = new URL(base); } catch { throw new Error("服务地址不完整，请填写完整的 http 或 https 地址。"); }
  if (!["https:", "http:"].includes(url.protocol) || url.username || url.password || url.search || url.hash) {
    throw new Error("服务地址无效，请去掉密钥、查询参数或其他多余内容。");
  }
  const model = $("ai-model").value.trim();
  if (!model) throw new Error("还没有选择模型，请填写服务商提供的模型 ID。");
  const key = $("ai-key").value;
  if (key && !key.trim()) throw new Error("密钥不能只含空格，请重新粘贴。");
  const rawLimit = $("ai-maxtokens").value;
  const limit = rawLimit === "" ? null : Number(rawLimit);
  if (limit !== null && (!Number.isSafeInteger(limit) || limit < 1)) throw new Error("回复长度上限必须是正整数，或留空使用默认值。");
  return { api_key: key.trim() || null, clear_saved_key: clearSavedKey,
    base_url: base, model, max_tokens: limit };
}

const CONNECTION_RESULTS = {
  verified: ["verified", "检测通过", "已收到模型回复，当前文字对话连接可用。"],
  missing_key: ["error", "缺少密钥", "没有可用密钥，请填写密钥后重试。"],
  invalid_config: ["error", "配置不完整", "请检查服务地址和模型名称后重试。"],
  unauthorized: ["error", "密钥无效", "服务没有接受这个密钥，请检查是否复制完整或已被撤销。"],
  forbidden: ["error", "没有访问权限", "当前账号没有访问权限，请检查服务商的账号与模型授权。"],
  model_unavailable: ["error", "请求未被接受", "请检查模型名称、服务地址，以及是否兼容 Anthropic Messages 接口。"],
  rate_limited: ["error", "请求过于频繁", "服务暂时限制了请求，请稍后重试。"],
  network_error: ["error", "连接失败", "暂时连不上服务，请检查地址和网络后重试。"],
  timed_out: ["error", "连接超时", "服务没有及时回应，请检查网络或稍后重试。"],
  invalid_response: ["error", "回复格式不兼容", "服务未返回有效的文字回复，请检查是否兼容 Anthropic Messages 接口。"],
  service_error: ["error", "服务暂不可用", "服务未能完成请求，请检查服务状态或稍后重试。"],
  preview_only: ["unverified", "浏览器预览", "这里只预览界面，请在 BitCat 桌面应用中检测真实连接。"],
};

async function testAiConnection() {
  if (connectionBusy) return;
  let payload;
  try { payload = collectConnectionDraft(); } catch (error) { setConnectionStatus("error", "请检查配置", error.message); setConnectionEditor(true); return; }
  const revision = connectionRevision;
  setConnectionBusy(true);
  setConnectionStatus("checking", "正在检测…");
  $("ai-test").textContent = "检测中…";
  try {
    const result = await invoke("cmd_settings_test_ai", { payload });
    if (revision !== connectionRevision) return;
    const [state, label, message] = CONNECTION_RESULTS[result.status] || CONNECTION_RESULTS.service_error;
    setConnectionStatus(state, label, message + (dirty.ai && state === "verified" ? " 当前修改尚未保存。" : ""));
    if (state === "verified" && !dirty.ai) {
      $("ov-ai-key").dataset.state = "ready";
      $("ov-ai-key").title = "连接检测通过";
      $("ov-ai-key").setAttribute("aria-label", "连接检测通过");
    }
  } catch {
    if (revision === connectionRevision) setConnectionStatus("error", "检测失败", "无法完成连接检测，请稍后重试。");
  } finally {
    setConnectionBusy(false);
    $("ai-test").textContent = "检测连接";
  }
}

async function saveAiConnection(apply = true) {
  if (connectionBusy) return false;
  let payload;
  try { payload = collectConnectionDraft(); } catch (error) { setConnectionStatus("error", "请检查配置", error.message); setConnectionEditor(true); return false; }
  const verified = $("connection-status").dataset.state === "verified";
  setConnectionBusy(true);
  $("ai-save").textContent = "保存中…";
  try {
    await invoke("cmd_settings_save_ai", { payload });
    if (apply) await invoke("cmd_settings_apply");
    const fresh = await invoke("cmd_settings_load");
    SNAPSHOT.ai = fresh.ai;
    renderAi(fresh.ai);
    clearDirty("ai");
    setConnectionEditor(false);
    if (verified) {
      setConnectionStatus("verified", "检测通过", "连接已保存并通过文字对话检测。");
      $("ov-ai-key").dataset.state = "ready";
      $("ov-ai-key").title = "连接检测通过";
      $("ov-ai-key").setAttribute("aria-label", "连接检测通过");
    }
    if (apply) toast("连接已保存", "ok");
    return true;
  } catch {
    setConnectionStatus("error", "保存未完成", "连接未能完成保存或应用，请稍后重试。其他分区的修改仍然保留。");
    return false;
  } finally {
    setConnectionBusy(false);
    $("ai-save").textContent = "保存连接";
  }
}

function bindConnection() {
  $("ai-edit").onclick = () => { setConnectionEditor(true); $("ai-provider").focus(); };
  $("ai-provider").onchange = () => {
    const official = $("ai-provider").value === "official";
    $("ai-custom-endpoint").classList.toggle("hidden", official);
    if (!official && $("ai-baseurl").value === "https://api.anthropic.com") $("ai-baseurl").value = "";
    invalidateConnection();
  };
  $("ai-clear-key").onclick = () => {
    clearSavedKey = !clearSavedKey;
    $("ai-key").value = "";
    updateConnectionKeyState();
    invalidateConnection();
  };
  $("ai-cancel").onclick = () => { renderAi(SNAPSHOT.ai); clearDirty("ai"); setConnectionEditor(false); };
  $("ai-test").onclick = testAiConnection;
  $("ai-save").onclick = () => saveAiConnection();
}

function renderOverviewNotices(ai) {
  const box = $("overview-notices");
  if (!box) return;
  const notices = [];
  if (!ai.has_effective_key) {
    notices.push(["密钥未配置", "对话不可用"]);
  }
  if (!ai.effective?.model) {
    notices.push(["模型未配置", ""]);
  }
  if (!notices.length) {
    box.innerHTML = `<div class="empty compact">一切正常，没有需要你处理的。</div>`;
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

// 「面键-右下」→「右下面键」：位置以括号内的次要信息出现，不再占独立视觉位。
function positionLabel(position) {
  const value = String(position || "").trim();
  const facePrefix = "面键-";
  if (value.startsWith(facePrefix)) return `${value.slice(facePrefix.length)}面键`;
  return value;
}

// 按键行的说明文字：确认（右下面键）· 触发 Select + ↑。
function keyMetaText(btn, triggerHint) {
  const label = btn.label && btn.label !== "(自定义)" ? btn.label : "";
  const position = positionLabel(btn.position);
  let meta = label;
  if (position) meta = meta ? `${meta}（${position}）` : position;
  if (triggerHint) meta = meta ? `${meta} · 触发 ${triggerHint}` : `触发 ${triggerHint}`;
  return meta || "自定义按键";
}

function renderActionItem(btn, def) {
  const el = document.createElement("div");
  el.className = "action-item";
  el.dataset.key = btn.name;

  const isUnbound = !def;
  if (isUnbound) el.classList.add("unbound");

  // 后端序列化用 "type"（serde rename），读取时归一到 action_type，历史遗留字段也兼容。
  const rawType = String(def?.action_type ?? def?.type ?? "unbound");
  const trigHintText = def && Array.isArray(def.trigger) && def.trigger.length > 0
    ? def.trigger.join(" + ")
    : "";
  const workingDef = def ? { ...def, action_type: rawType } : { action_type: "unbound" };
  const curType = rawType;

  el.innerHTML = `
    <div class="ai-head">
      <div class="key-block">
        <span class="key">${escapeHtml(btn.name)}</span>
        <span class="key-meta">${escapeHtml(keyMetaText(btn, trigHintText))}</span>
      </div>
      <button type="button" class="action-summary" aria-expanded="false" title="展开编辑详情"></button>
      <select class="a-type" title="动作类型">
        ${ACTION_TYPES.map(t => `<option value="${t}" ${t === curType ? "selected" : ""}>${escapeHtml(ACTION_TYPE_LABELS[t] || t)}</option>`).join("")}
      </select>
    </div>
    <div class="ai-body"></div>
  `;

  const body = el.querySelector(".ai-body");
  const summary = el.querySelector(".action-summary");
  const setExpanded = (expanded) => {
    el.classList.toggle("expanded", expanded);
    summary.setAttribute("aria-expanded", String(expanded));
  };
  const refreshSummary = () => {
    // 未绑定行不显示摘要，下拉框就是全部状态；绑定后摘要是一句人话，点击展开表单。
    summary.textContent = workingDef.action_type === "unbound"
      ? ""
      : actionSummary(workingDef.action_type, workingDef);
  };
  refreshSummary();
  renderActionBody(body, workingDef, refreshSummary);

  summary.addEventListener("click", () => {
    setExpanded(!el.classList.contains("expanded"));
  });

  const sel = el.querySelector(".a-type");
  sel.addEventListener("change", () => {
    workingDef.action_type = sel.value;
    el.classList.toggle("unbound", sel.value === "unbound");
    refreshSummary();
    renderActionBody(body, workingDef, refreshSummary);
    // 主动配置动作时自动展开表单；改回未绑定则收起。
    setExpanded(sel.value !== "unbound");
    markDirty("actions");
  });
  return el;
}

function renderActionBody(body, def, onChange = () => {}) {
  body.innerHTML = "";
  const t = def.action_type;
  if (t === "unbound") return;

  const mk = (label, id, val, type = "text", hint = "") => {
    const row = document.createElement("div");
    row.className = hint ? "row with-hint" : "row";
    row.innerHTML = `<label>${label}</label><input data-field="${id}" type="${type}" value="${escapeAttr(val ?? "")}" />${hint ? `<span class="row-hint">${hint}</span>` : ""}`;
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
  mk("键盘快捷键", "kbd", def.keyboard_shortcut || "", "text", "在键盘上按它直接触发，留空关闭");
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

// 提示词徽标：留空 = 使用内置默认值（overlay 语义），非空 = 已自定义。
const PROMPT_BADGE_PAIRS = [
  ["p-agent", "p-agent-custom"],
  ["p-vision", "p-vision-custom"],
  ["p-vision-multi", "p-vision-multi-custom"],
  ["p-reminder-personalizer", "p-reminder-personalizer-custom"],
];

function updatePromptBadges() {
  for (const [inputId, badgeId] of PROMPT_BADGE_PAIRS) {
    const badge = $(badgeId);
    if (!badge) continue;
    const custom = !!$(inputId)?.value.trim();
    badge.textContent = custom ? "已自定义" : "默认";
    badge.classList.toggle("custom", custom);
  }
}

function renderPrompts(p) {
  $("p-agent").value = p.agent.preamble;
  $("p-vision").value = p.vision.prompt;
  $("p-vision-multi").value = p.vision.prompt_multi;
  $("p-reminder-personalizer").value = p.reminder_personalizer?.preamble || "";
  $("p-mem-max").value = p.memory.max_entries;
  $("p-mem-ctx").value = p.memory.max_context_chars;
  $("p-ss-interval").value = p.screen_summary.interval_min;
  updatePromptBadges();

  ["p-agent","p-vision","p-vision-multi","p-reminder-personalizer","p-mem-max","p-mem-ctx","p-ss-interval"].forEach(id => {
    $(id).oninput = () => {
      markDirty("prompts");
      updatePromptBadges();
    };
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
  renderShortcutChips();
  bindShortcutChips();
  $("a-ss-interval").value = a.screenshot_interval_sec ?? 30;
  $("a-ss-bubble").checked = a.screenshot_show_bubble !== false;
  $("a-camera-enabled").checked = !!a.camera_observation_enabled;
  $("a-camera-save").checked = !!a.camera_save_frames;
  $("screen-time-master").checked = a.screen_time_enabled !== false;
  const earnings = a.earnings || {};
  $("a-earnings-salary").value = earnings.monthly_salary_cents
    ? String(earnings.monthly_salary_cents / 100)
    : "";
  $("a-earnings-start").value = minutesToTime(earnings.work_start_minutes ?? 540);
  $("a-earnings-end").value = minutesToTime(earnings.work_end_minutes ?? 1080);
  $("a-earnings-workdays").value = String(earnings.workdays_per_month ?? 21.75);
  renderStorage(SNAPSHOT?.storage);
  renderPetAssetPicker();
  renderPetAssetChoice(a.pet_asset_url || "");
  updateOverviewAppearance(a);

  ["a-top","a-collapsed","a-tts","a-notify-sound","a-notify-sound-reminder","a-notify-sound-agent","a-notify-sound-skip-tts","a-reminder-ai","a-ss-bubble","a-camera-enabled","a-camera-save","screen-time-master"].forEach(id => { $(id).onchange = () => markDirty("appearance"); });
  ["a-earnings-salary","a-earnings-start","a-earnings-end","a-earnings-workdays"].forEach(id => { $(id).oninput = () => markDirty("appearance"); });
  ["a-shortcut","a-ss-interval","a-reminder-ai-timeout","a-pet-asset","a-storage-data","a-storage-app-data"].forEach(id => { $(id).oninput = () => markDirty("appearance"); });
  $("a-pet-asset").onchange = updatePetAssetPickerSelection;
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
    screen_time_enabled: $("screen-time-master").checked,
    earnings: collectEarnings(),
    pet_asset_url: collectPetAssetUrl(),
  };
}

// A4 上班金币：月薪单位分，时间 HH:MM ↔ 分钟
function collectEarnings() {
  const salaryYuan = parseFloat($("a-earnings-salary").value);
  return {
    monthly_salary_cents:
      Number.isFinite(salaryYuan) && salaryYuan > 0 ? Math.round(salaryYuan * 100) : 0,
    work_start_minutes: timeToMinutes($("a-earnings-start").value, 540),
    work_end_minutes: timeToMinutes($("a-earnings-end").value, 1080),
    workdays_per_month: Math.min(31, Math.max(1, parseFloat($("a-earnings-workdays").value) || 21.75)),
  };
}

function minutesToTime(minutes) {
  const m = Number(minutes) || 0;
  const h = Math.min(23, Math.floor(m / 60));
  const rest = m % 60;
  return String(h).padStart(2, "0") + ":" + String(rest).padStart(2, "0");
}

function timeToMinutes(value, fallback) {
  const match = /^(\d{1,2}):(\d{2})$/.exec(String(value || "").trim());
  if (!match) return fallback;
  const h = Number(match[1]);
  const m = Number(match[2]);
  if (h > 23 || m > 59) return fallback;
  return h * 60 + m;
}

// 上班金币只读卡片（A4）
async function loadEarningsSummary() {
  const el = $("earnings-today");
  if (!el) return;
  try {
    const s = await invoke("cmd_earnings_summary");
    const rateEl = $("earnings-rate");
    if (!s.enabled) {
      el.textContent = "未开启 · 填月薪后生效";
      if (rateEl) rateEl.textContent = "";
      return;
    }
    const yuan = (s.today_cents / 100).toFixed(2);
    // "已落"用实际掉落数（息屏跳过不计），账本和视觉保持一致。
    el.textContent = `¥${yuan} · 已落 ${s.coins_emitted ?? s.coins ?? 0} 枚金币`;
    // 机制自解释：面额 + 按月薪/工时算出的掉币速率。
    if (rateEl) {
      const e = SNAPSHOT?.appearance?.earnings || {};
      const workMin = (e.work_end_minutes ?? 1080) - (e.work_start_minutes ?? 540);
      const centsPerMin = e.monthly_salary_cents > 0 && workMin > 0 && e.workdays_per_month > 0
        ? e.monthly_salary_cents / e.workdays_per_month / workMin
        : 0;
      const coinsPerMin = centsPerMin / 10;
      rateEl.textContent = coinsPerMin >= 1
        ? `1 枚 = 1 角 · 每分钟约 ${Math.round(coinsPerMin)} 枚`
        : coinsPerMin > 0
          ? `1 枚 = 1 角 · 约每 ${Math.max(1, Math.round(1 / coinsPerMin))} 分钟一枚`
          : "1 枚 = 1 角";
    }
  } catch (e) {
    log("上班金币加载失败: " + e);
    el.textContent = "—";
  }
}

// 陪伴时长只读卡片：今天 + 最近 7 天（A4.0）
async function loadScreenTimeSummary() {
  const today = $("screen-time-today");
  const week = $("screen-time-week");
  const weekWrap = $("screen-time-week-wrap");
  if (!today || !week) return;
  try {
    const s = await invoke("cmd_screen_time_summary");
    if (!s.enabled) {
      today.textContent = "已关闭，可在「它能做什么」开启";
      if (weekWrap) weekWrap.hidden = true;
      return;
    }
    if (weekWrap) weekWrap.hidden = false;
    today.textContent = formatDurationMin(s.today_minutes);
    week.textContent = formatDurationMin(s.week_minutes);
  } catch (e) {
    log("陪伴时长加载失败: " + e);
    today.textContent = "—";
    week.textContent = "—";
  }
}

// ─── 全局快捷键键帽展示 ───
// 加速器词表（CommandOrControl+Alt+Space）是开发者语汇；展示时拆成键帽，
// CommandOrControl 按平台翻译。编辑时才露出原始文本框。
const IS_MAC = typeof navigator !== "undefined"
  && /Mac|iP(hone|ad|od)/.test(navigator.platform || navigator.userAgent || "");

const SHORTCUT_TOKEN_LABELS = {
  commandorcontrol: () => (IS_MAC ? "⌘" : "Ctrl"),
  cmdorctrl: () => (IS_MAC ? "⌘" : "Ctrl"),
  command: () => "⌘",
  cmd: () => "⌘",
  control: () => "Ctrl",
  ctrl: () => "Ctrl",
  alt: () => "Alt",
  option: () => "Alt",
  shift: () => "Shift",
  space: () => "空格",
  plus: () => "+",
  up: () => "↑",
  down: () => "↓",
  left: () => "←",
  right: () => "→",
};

function shortcutChips(value) {
  return String(value || "")
    .split("+")
    .map(token => token.trim())
    .filter(Boolean)
    .map(token => {
      const label = SHORTCUT_TOKEN_LABELS[token.toLowerCase()];
      return label ? label() : token;
    });
}

function renderShortcutChips() {
  const chips = $("a-shortcut-chips");
  if (!chips) return;
  const labels = shortcutChips($("a-shortcut")?.value);
  chips.innerHTML = labels.length
    ? labels.map(label => `<kbd>${escapeHtml(label)}</kbd>`).join("")
    : `<span class="key-chips-empty">未设置，点击输入</span>`;
}

function bindShortcutChips() {
  const field = $("shortcut-field");
  const chips = $("a-shortcut-chips");
  const input = $("a-shortcut");
  if (!field || !chips || !input) return;
  const edit = () => {
    field.classList.add("editing");
    input.focus();
  };
  const done = () => {
    field.classList.remove("editing");
    renderShortcutChips();
  };
  chips.onclick = edit;
  chips.onkeydown = (event) => {
    if (event.key === "Enter" || event.key === " ") { event.preventDefault(); edit(); }
  };
  input.onblur = done;
  input.onkeydown = (event) => {
    if (event.key === "Enter") { event.preventDefault(); done(); }
    if (event.key === "Escape") { input.value = SNAPSHOT?.appearance?.global_shortcut || input.value; done(); }
  };
}

function formatDurationMin(minutes) {
  const m = Number(minutes) || 0;
  if (m < 60) return m + " 分钟";
  const h = Math.floor(m / 60);
  const rest = m % 60;
  return rest ? h + " 小时 " + rest + " 分钟" : h + " 小时";
}

function renderPermissions(p = {}) {
  $("perm-onboarding-completed").checked = !!p.onboarding_completed;
  $("perm-steam-demo").checked = !!p.steam_demo_mode;
  // 默认值已是 false（F3），缺失字段必须显示为关闭，不能沿用旧 !== false。
  $("perm-screenshot").checked = !!p.allow_screenshot_observation;
  $("perm-camera").checked = !!p.allow_camera_observation;
  $("perm-shell").checked = !!p.allow_shell_tool;
  $("perm-read-file").checked = !!p.allow_read_file_tool;
  $("perm-clipboard").checked = !!p.allow_clipboard_tool;
  $("perm-foreground").checked = !!p.allow_foreground_tool;
  $("perm-launch").checked = !!p.allow_launch_program_tool;
  $("perm-hotkey").checked = !!p.allow_hotkey_tool;
  $("perm-agent-remote").checked = !!p.allow_agent_watch_remote;
  $("perm-diagnostics").checked = p.diagnostics_enabled !== false;
  $("perm-onboarding").classList.remove("hidden");
  updatePermissionGateSummary();
  // renderAppearance 先于本函数执行，首页摘要此刻才拿到真实开关值，必须重算一次。
  updateHomePermissionCards();

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
  ].forEach(id => {
    $(id).onchange = () => {
      updatePermissionGateSummary();
      updateHomePermissionCards();
      markDirty("permissions");
    };
  });

  $("perm-complete").onclick = () => {
    showWizard();
  };
}

// 首页“它能做什么”分区的状态行：把分散的权限开关压缩成人话结论。
function updateHomePermissionCards() {
  const screenshotOn = $("perm-screenshot")?.checked;
  const qs = $("qs-screenshot");
  if (qs) {
    qs.textContent = screenshotOn ? "开启" : "关闭";
    qs.dataset.state = screenshotOn ? "ready" : "off";
  }
  const interval = Number(SNAPSHOT?.appearance?.screenshot_interval_sec ?? 30);
  const hint = $("ss-interval-hint");
  if (hint) hint.textContent = screenshotOn ? `每 ${formatNumber(interval)} 秒一次` : "看不到你的屏幕";
  if (qs) qs.title = hint?.textContent || "";

  const toolIds = ["perm-shell", "perm-read-file", "perm-clipboard", "perm-foreground", "perm-launch", "perm-hotkey"];
  const enabledCount = toolIds.filter(id => $(id)?.checked).length;
  const summary = $("perm-tools-summary");
  if (summary) {
    if (!enabledCount) summary.textContent = "动手能力全部关闭";
    else if (enabledCount === toolIds.length) summary.textContent = "6 类操作已全部允许，危险命令仍会拦截";
    else summary.textContent = `已允许 ${enabledCount} 类操作，危险命令仍会拦截`;
  }

  const masterOn = !!($("perm-camera")?.checked) && !!($("a-camera-enabled")?.checked);
  ["camera-master", "camera-master-expert"].forEach(id => {
    const el = $(id);
    if (el) el.checked = masterOn;
  });
}

// 摄像头观察主开关有两处（首页 + 专家），任一处都同步权限层与功能层两个底层设置。
function bindCameraMaster() {
  ["camera-master", "camera-master-expert"].forEach(id => {
    const master = $(id);
    if (!master) return;
    master.onchange = () => {
      const on = master.checked;
      if ($("perm-camera")) $("perm-camera").checked = on;
      if ($("a-camera-enabled")) $("a-camera-enabled").checked = on;
      ["camera-master", "camera-master-expert"].forEach(other => {
        const el = $(other);
        if (el) el.checked = on;
      });
      markDirty("permissions");
      markDirty("appearance");
      updatePermissionGateSummary();
      updateHomePermissionCards();
    };
  });
}

async function revokeAllPermissions() {
  if (!await confirmDialog({
    title: "全部收权",
    message: "会关掉屏幕观察、摄像头观察和所有动手能力，只保留聊天和陪伴。保存后生效。",
    okText: "全部收权",
  })) return;
  ["perm-screenshot", "perm-camera", "a-camera-enabled",
   "perm-shell", "perm-read-file", "perm-clipboard",
   "perm-foreground", "perm-launch", "perm-hotkey",
   "perm-agent-remote"].forEach(id => {
    if ($(id)) $(id).checked = false;
  });
  markDirty("permissions");
  markDirty("appearance");
  updatePermissionGateSummary();
  updateHomePermissionCards();
  toast("已全部收权，点击保存后生效", "ok");
}

// 状态词表全 app 统一：开启 / 关闭 / 拦截（高风险工具按六开关实际状态细分）。
// "默认"前缀不再出现在状态行——默认值属于开关提示，不属于当前状态。
function setGateStatus(key, dotState, label) {
  const dot = $(`perm-dot-${key}`);
  const value = $(`perm-status-${key}`);
  if (dot) dot.dataset.state = dotState;
  if (value) value.textContent = label;
}

function updatePermissionGateSummary() {
  const completed = $("perm-onboarding-completed")?.checked;
  const screenshot = $("perm-screenshot")?.checked;
  const camera = $("perm-camera")?.checked;
  const remote = $("perm-agent-remote")?.checked;
  const toolIds = ["perm-shell", "perm-read-file", "perm-clipboard", "perm-foreground", "perm-launch", "perm-hotkey"];
  const enabledCount = toolIds.filter(id => $(id)?.checked).length;

  $("perm-gate-title").textContent = completed ? "首次说明已确认" : "等待首次确认";
  $("perm-gate-summary").textContent = completed
    ? "改动保存后生效，只存本机。"
    : "完成前会自动打开本页，便于审查 AI、观察和系统工具边界。";
  setGateStatus("screenshot", screenshot ? "ready" : "idle", screenshot ? "开启" : "关闭");
  setGateStatus("camera", camera ? "ready" : "idle", camera ? "开启" : "关闭");
  if (enabledCount === 0) setGateStatus("tools", "missing", "拦截");
  else if (enabledCount === toolIds.length) setGateStatus("tools", "ready", "全部允许");
  else setGateStatus("tools", "missing", `部分允许 ${enabledCount}/${toolIds.length}`);
  setGateStatus("remote", remote ? "ready" : "idle", remote ? "开启" : "关闭");
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

// ---- D1 三步信任向导：它是谁 → 会什么/不会什么 → 随时可收回 ----

let wizardStep = 1;

function showWizard(step = 1) {
  const wizard = $("onboarding-wizard");
  if (!wizard) return;
  wizard.classList.remove("hidden");
  setWizardStep(step);
}

function hideWizard() {
  $("onboarding-wizard").classList.add("hidden");
}

function setWizardStep(step) {
  wizardStep = Math.max(1, Math.min(3, step));
  document.querySelectorAll("#onboarding-wizard .wizard-step").forEach((el) => {
    el.classList.toggle("hidden", Number(el.dataset.wizardStep) !== wizardStep);
  });
  document.querySelectorAll("#onboarding-wizard [data-dot]").forEach((el) => {
    const dot = Number(el.dataset.dot);
    el.classList.toggle("active", dot === wizardStep);
    el.classList.toggle("done", dot < wizardStep);
  });
}

async function finishWizard() {
  const base = SNAPSHOT?.permissions || {};
  const toolsOn = $("wiz-tools").checked;
  const payload = {
    ...base,
    onboarding_completed: true,
    allow_screenshot_observation: $("wiz-screenshot").checked,
    allow_camera_observation: $("wiz-camera").checked,
    // "替你动手"是粗粒度组开关：六项一起开；细粒度调整在设置页。
    allow_shell_tool: toolsOn,
    allow_read_file_tool: toolsOn,
    allow_clipboard_tool: toolsOn,
    allow_foreground_tool: toolsOn,
    allow_launch_program_tool: toolsOn,
    allow_hotkey_tool: toolsOn,
  };
  try {
    await invoke("cmd_settings_save_permissions", { payload });
    if (SNAPSHOT?.permissions) Object.assign(SNAPSHOT.permissions, payload);
    renderPermissions(SNAPSHOT.permissions);
    updateHomePermissionCards();
    hideWizard();
    toast("设置好了，去陪它玩吧", "ok");
  } catch (e) {
    toast("保存失败：" + String(e), "err");
  }
}

function setupWizard() {
  const wizard = $("onboarding-wizard");
  if (!wizard) return;
  // 显式跳过出口：关掉窗口、下次启动再回来；比隐藏的 Esc 更可控。
  const later = $("wizard-later");
  if (later) later.onclick = () => tryClose();
  wizard.querySelectorAll("[data-wizard-next]").forEach((btn) => {
    btn.addEventListener("click", () => setWizardStep(wizardStep + 1));
  });
  wizard.querySelectorAll("[data-wizard-back]").forEach((btn) => {
    btn.addEventListener("click", () => setWizardStep(wizardStep - 1));
  });
  wizard.querySelector("[data-wizard-skip]")?.addEventListener("click", () => {
    // 稍后再说：不标记完成，下次启动还会回来。
    hideWizard();
  });
  $("wiz-finish").addEventListener("click", finishWizard);
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
    if (!$("a-pet-asset").value.trim()) $("a-pet-asset").value = PET_ASSET_DEFAULT;
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
  const gridEl = document.createElement("div");
  gridEl.className = "pet-asset-grid";
  picker.appendChild(gridEl);

  for (const preset of PET_ASSET_PRESETS) {
    const grid = gridEl;
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

  const customGrid = gridEl;
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
  const columns = Math.max(1, getComputedStyle(event.currentTarget.parentElement).gridTemplateColumns.split(/\s+/).filter(Boolean).length);
  const next = event.key === "ArrowDown" ? Math.min(cards.length - 1, index + columns)
    : event.key === "ArrowUp" ? Math.max(0, index - columns)
    : (index + (event.key === "ArrowLeft" ? -1 : 1) + cards.length) % cards.length;
  cards[next].focus();
}

function updatePetAssetPickerSelection() {
  const picker = $("a-pet-asset-picker");
  if (!picker) return;
  const value = selectedPetAssetPreset;
  const preview = $("pet-large-preview");
  const caption = $("pet-preview-name");
  if (preview && caption) {
    stopPetPreview();
    petPreview.asset = null;
    petPreview.index = 0;
    if ($("pet-preview-play")) $("pet-preview-play").disabled = true;
    if ($("pet-preview-state")) $("pet-preview-state").textContent = "正在加载";
    caption.textContent = PET_ASSET_PRESETS.find(preset => preset.value === value)?.label || "自定义猫咪";
    renderPetAssetPreview(preview, value === "__custom" ? $("a-pet-asset").value : value);
  }
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
  // Each canvas keeps the latest request so a slow previous selection cannot overwrite it.
  canvas.dataset.previewUrl = baseUrl;
  try {
    const asset = await loadPetAssetPreview(baseUrl);
    if (canvas.dataset.previewUrl === baseUrl) {
      drawPetAssetPreview(canvas, asset);
      if (canvas.id === "pet-large-preview") {
        petPreview.asset = asset;
        if ($("pet-preview-play")) $("pet-preview-play").disabled = !availablePetPreviewStates(asset).length;
        if ($("pet-preview-state")) $("pet-preview-state").textContent = "安静待着";
      }
    }
  } catch (error) {
    if (canvas.dataset.previewUrl === baseUrl) {
      drawCustomPetAssetPreview(canvas);
      if (canvas.id === "pet-large-preview") {
        petPreview.asset = null;
        if ($("pet-preview-play")) $("pet-preview-play").disabled = true;
        if ($("pet-preview-state")) $("pet-preview-state").textContent = "暂时无法预览";
      }
    }
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

function drawPetAssetPreview(canvas, asset, selectedFrame) {
  const ctx = canvas.getContext("2d");
  const manifest = asset.manifest || {};
  const sprite = manifest.sprite || {};
  const fw = sprite.frameWidth || 1;
  const fh = sprite.frameHeight || 1;
  const columns = sprite.columns || 1;
  const frame = Number.isInteger(selectedFrame) ? selectedFrame : Number.isInteger(manifest.mini?.frame)
    ? manifest.mini.frame
    : (manifest.states?.idle?.frames?.[0]?.sprite || 0);
  const sx = (frame % columns) * fw;
  const sy = Math.floor(frame / columns) * fh;
  const scale = Math.min((canvas.width - 6) / fw, (canvas.height - 6) / fh);
  const dw = Math.max(1, Math.round(fw * scale));
  const dh = Math.max(1, Math.round(fh * scale));
  const dx = Math.floor((canvas.width - dw) / 2);
  const dy = Math.floor((canvas.height - dh) / 2);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.imageSmoothingEnabled = manifest.render?.pixelated === true ? false : true;
  ctx.drawImage(asset.image, sx, sy, fw, fh, dx, dy, dw, dh);
}

// Preview timelines use the selected pack's actual frame indices and durations.
function petPreviewFrames(asset, name) {
  const manifest = asset?.manifest;
  const config = manifest?.actions?.[name] || manifest?.states?.[name];
  const limit = manifest?.sprite?.frameCount || (manifest?.sprite?.columns || 1) * (manifest?.sprite?.rows || 1);
  return (Array.isArray(config?.frames) ? config.frames : []).filter(frame => frame && Number.isInteger(frame.sprite) && frame.sprite >= 0 && frame.sprite < limit && Number.isFinite(frame.duration) && frame.duration > 0);
}

function availablePetPreviewStates(asset) {
  return PET_PREVIEW_STATES.filter(([name]) => petPreviewFrames(asset, name).length);
}

function petPreviewFrameAt(frames, elapsed) {
  const duration = frames.reduce((sum, frame) => sum + frame.duration, 0);
  if (!duration) return 0;
  let remaining = Math.max(0, elapsed) % duration;
  for (const frame of frames) {
    if (remaining < frame.duration) return frame.sprite;
    remaining -= frame.duration;
  }
  return frames[0].sprite;
}

function stopPetPreview(reset = true) {
  if (petPreview.raf) window.cancelAnimationFrame(petPreview.raf);
  petPreview.raf = 0;
  petPreview.animation = null;
  petPreview.lastFrame = -1;
  petPreview.gesture = null;
  const button = $("pet-preview-play");
  button?.classList.remove("is-dragging");
  button?.style.removeProperty("--preview-x");
  button?.style.removeProperty("--preview-y");
  if (reset && petPreview.asset && $("pet-large-preview")) {
    drawPetAssetPreview($("pet-large-preview"), petPreview.asset);
    if ($("pet-preview-state")) $("pet-preview-state").textContent = "安静待着";
  }
}

function playPetPreview(name, label) {
  const frames = petPreviewFrames(petPreview.asset, name);
  const canvas = $("pet-large-preview");
  if (!canvas || !frames.length) return false;
  if (petPreview.raf) window.cancelAnimationFrame(petPreview.raf);
  petPreview.raf = 0;
  petPreview.animation = null;
  $("pet-preview-state").textContent = label;
  drawPetAssetPreview(canvas, petPreview.asset, frames[0].sprite);
  if (previewReducedMotion?.matches || document.hidden || currentTab !== "companion") return true;
  petPreview.lastFrame = frames[0].sprite;
  const animation = { start: performance.now(), frames };
  petPreview.animation = animation;
  const tick = now => {
    petPreview.raf = 0;
    if (petPreview.animation !== animation) return;
    if (document.hidden || currentTab !== "companion" || now - animation.start >= 5000) { stopPetPreview(); return; }
    const frame = petPreviewFrameAt(frames, now - animation.start);
    if (frame !== petPreview.lastFrame) { drawPetAssetPreview(canvas, petPreview.asset, frame); petPreview.lastFrame = frame; }
    petPreview.raf = window.requestAnimationFrame(tick);
  };
  petPreview.raf = window.requestAnimationFrame(tick);
  return true;
}

function cyclePetPreview() {
  const states = availablePetPreviewStates(petPreview.asset);
  if (!states.length) return;
  petPreview.index = (petPreview.index + 1) % states.length;
  playPetPreview(...states[petPreview.index]);
}

function bindPetPreview() {
  const button = $("pet-preview-play");
  if (!button) return;
  previewReducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)");
  previewReducedMotion?.addEventListener("change", () => stopPetPreview());
  button.onclick = event => {
    if (petPreview.suppressClick && event.detail !== 0) { petPreview.suppressClick = false; return; }
    cyclePetPreview();
  };
  button.onpointerdown = event => {
    if (event.button !== 0 || button.disabled) return;
    petPreview.suppressClick = false;
    petPreview.gesture = { id: event.pointerId, x: event.clientX, y: event.clientY, moved: false };
    button.setPointerCapture?.(event.pointerId);
  };
  button.onpointermove = event => {
    const gesture = petPreview.gesture;
    if (!gesture || gesture.id !== event.pointerId) return;
    const dx = event.clientX - gesture.x, dy = event.clientY - gesture.y;
    if (!gesture.moved && Math.hypot(dx, dy) < 6) return;
    if (!gesture.moved) {
      gesture.moved = true;
      button.classList.add("is-dragging");
      if (!playPetPreview("dragging", "轻轻拎起来")) playPetPreview("walk", "走两步");
    }
    button.style.setProperty("--preview-x", `${Math.max(-18, Math.min(18, dx))}px`);
    button.style.setProperty("--preview-y", `${Math.max(-12, Math.min(12, dy))}px`);
  };
  button.onpointerup = event => {
    const gesture = petPreview.gesture;
    if (!gesture || gesture.id !== event.pointerId) return;
    petPreview.gesture = null;
    button.classList.remove("is-dragging");
    button.style.removeProperty("--preview-x");
    button.style.removeProperty("--preview-y");
    if (gesture.moved) { petPreview.suppressClick = true; if (!playPetPreview("happy", "开心")) stopPetPreview(); }
    if (button.hasPointerCapture?.(event.pointerId)) button.releasePointerCapture(event.pointerId);
  };
  button.onpointercancel = () => stopPetPreview();
  button.onlostpointercapture = () => { if (petPreview.gesture) stopPetPreview(); };
  document.addEventListener("visibilitychange", () => { if (document.hidden) stopPetPreview(); });
  window.addEventListener("pagehide", () => stopPetPreview());
}

function drawCustomPetAssetPreview(canvas) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const size = Math.min(canvas.width, canvas.height);
  const inset = Math.round(size * .2);
  ctx.fillStyle = "#e0eee5";
  ctx.strokeStyle = "#789b85";
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.roundRect(inset, inset, size - inset * 2, size - inset * 2, 4);
  ctx.fill();
  ctx.stroke();
  ctx.fillStyle = "#28764f";
  ctx.font = `600 ${Math.round(size * .45)}px ${CANVAS_UI_FONT}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("?", canvas.width / 2, canvas.height / 2);
}

function renderAgentWatch(a) {
  const cfg = a || {};
  const quiet = cfg.quiet_hours || {};
  $("aw-enabled").checked = !!cfg.enabled;
  $("aw-away").checked = cfg.away_nudge_enabled !== false;
  $("aw-first").value = cfg.first_nudge_after_sec ?? 30;
  $("aw-repeat").value = cfg.repeat_nudge_after_min ?? 8;
  $("aw-waiting").checked = cfg.waiting_alert !== false;
  $("aw-done").checked = cfg.done_alert !== false;
  $("aw-tts").checked = !!cfg.use_tts;
  $("aw-quiet").checked = !!quiet.enabled;
  $("aw-quiet-start").value = quiet.start || "23:00";
  $("aw-quiet-end").value = quiet.end || "07:00";
  $("aw-remote-view").checked = cfg.remote_view_enabled !== false;
  $("aw-remote-install").checked = cfg.remote_install_enabled !== false;
  ["aw-enabled","aw-away","aw-waiting","aw-done","aw-tts","aw-quiet","aw-remote-view","aw-remote-install"].forEach(id => { $(id).onchange = () => markDirty("agent_watch"); });
  ["aw-first","aw-repeat","aw-quiet-start","aw-quiet-end"].forEach(id => { $(id).oninput = () => markDirty("agent_watch"); });
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
    quiet_hours: {
      enabled: $("aw-quiet").checked,
      start: $("aw-quiet-start").value || "23:00",
      end: $("aw-quiet-end").value || "07:00",
    },
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
    refreshConnectorActivity();
    loadRemoteDevices();
    if (status) status.textContent = snapshot?.generated_at_ms ? `更新于 ${formatDateTime(snapshot.generated_at_ms)}` : "等待状态";
  } catch (e) {
    log("加载 Agent 会话失败: " + e);
    renderAgentSessions(null);
    refreshConnectorActivity();
    if (status) status.textContent = "读取失败";
  }
}

function startAgentWatchRefresh() {
  loadRemoteInstallCommand();
  loadConnectorStatuses();
  loadAgentSessions();
  if (agentWatchTimer) return;
  agentWatchTimer = setInterval(() => {
    if (currentTab === "expert" && currentExpertPage === "agent-watch") loadAgentSessions();
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
  latestAgentSnapshot = snapshot || null;
  const diag = $("aw-diag");
  if (diag) {
    const parts = [];
    if (snapshot?.monitor_port) parts.push(escapeHtml(`端口 ${snapshot.monitor_port}`));
    if (typeof snapshot?.event_count === "number") parts.push(escapeHtml(`事件 ${snapshot.event_count}`));
    if (snapshot?.last_event_at_ms) parts.push(escapeHtml(`最近 ${new Date(snapshot.last_event_at_ms).toLocaleTimeString()}`));
    if (snapshot?.log_dir) parts.push(`日志 <code>${escapeHtml(snapshot.log_dir)}</code>`);
    // 诊断辅助信息降成一行小字：不值得四颗药丸的存在感。
    diag.innerHTML = parts.length ? `<span>${parts.join(" · ")}</span>` : "";
  }
  const sessions = snapshot?.sessions || [];
  if (!sessions.length) {
    box.innerHTML = `<div class="empty-note">暂无 Agent 会话。</div>`;
    return;
  }
  // 每会话两行：身份行（项目·状态·来源·机器）+ 任务行（路径·提示词）。
  box.innerHTML = sessions.map(session => {
    const path = session.workspace || session.session_id;
    return `
    <div class="agent-session ${escapeAttr(session.status)}">
      <div class="agent-session-main">
        <strong>${escapeHtml(session.workspace_name || "未知项目")}</strong>
        <span class="agent-session-status">${escapeHtml(session.status_label || session.status)}</span>
        <span>${escapeHtml(window.AgentSources.label(session.source))}</span>
        ${session.machine ? `<span>${escapeHtml(session.machine)}</span>` : ""}
        ${session.tool_name ? `<span>${escapeHtml(session.tool_name)}</span>` : ""}
      </div>
      <div class="agent-session-sub">
        <code title="${escapeAttr(path)}">${escapeHtml(path)}</code>
        ${session.user_prompt_preview ? `<span class="agent-session-prompt" title="${escapeAttr(session.user_prompt_preview)}">${escapeHtml(session.user_prompt_preview)}</span>` : ""}
      </div>
    </div>
  `;
  }).join("");
}

// ── 编程助手连接器（专家模式 · 编程助手看管）──
// 每个来源一行：安装状态来自 cmd_agent_connectors_status，最近事件时间从
// 会话快照按来源聚合。「修复」按钮只在该来源未接入时出现。

// 24 小时内有事件才算"已连接"；更久视为已接入但暂时安静。
const CONNECTOR_FRESH_MS = 24 * 60 * 60 * 1000;

function connectorActivityBySource(snapshot) {
  const bySource = new Map();
  (snapshot?.sessions || []).forEach(session => {
    if (!session?.source) return;
    const at = Number(session.updated_at_ms || 0);
    if (at > (bySource.get(session.source) || 0)) bySource.set(session.source, at);
  });
  return bySource;
}

function connectorAgoLabel(deltaMs) {
  const sec = Math.max(0, Math.floor(deltaMs / 1000));
  if (sec < 60) return "刚刚";
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} 分钟前`;
  const hour = Math.floor(min / 60);
  if (hour < 24) return `${hour} 小时前`;
  return `${Math.floor(hour / 24)} 天前`;
}

function connectorState(status, lastEventAtMs, nowMs) {
  if (!status.installed) return { dot: "missing", hint: "未安装连接脚本" };
  if (!lastEventAtMs) return { dot: "idle", hint: "已安装 · 还没有事件" };
  const fresh = nowMs - lastEventAtMs < CONNECTOR_FRESH_MS;
  return {
    dot: fresh ? "ready" : "idle",
    hint: `已${fresh ? "连接" : "接入"} · 最近事件 ${connectorAgoLabel(nowMs - lastEventAtMs)}`,
  };
}

function renderConnectors() {
  const box = $("aw-connectors");
  if (!box) return;
  if (!Array.isArray(connectorStatuses)) {
    box.innerHTML = `<div class="empty-note">连接状态不可用，点「检查全部连接」重试。</div>`;
    return;
  }
  const nowMs = Date.now();
  const activity = connectorActivityBySource(latestAgentSnapshot);
  box.innerHTML = connectorStatuses.map(status => {
    const state = connectorState(status, activity.get(status.source) || 0, nowMs);
    return `
      <div class="connector-row" data-source="${escapeAttr(status.source)}">
        <span class="connector-dot" data-state="${state.dot}"></span>
        <div class="connector-copy">
          <strong>${escapeHtml(window.AgentSources.label(status.source))}</strong>
          <span class="row-hint">${escapeHtml(state.hint)}</span>
        </div>
        ${status.installed ? "" : `<button class="btn small ghost" type="button" data-repair-source="${escapeAttr(status.source)}">修复</button>`}
      </div>
    `;
  }).join("");
}

// 会话每 2 秒轮询一次，这里只轻量更新活动文案，避免整行重绘打断点击。
function refreshConnectorActivity() {
  const box = $("aw-connectors");
  if (!box || !Array.isArray(connectorStatuses)) return;
  const nowMs = Date.now();
  const activity = connectorActivityBySource(latestAgentSnapshot);
  connectorStatuses.forEach(status => {
    // source 是我们自己的 snake_case 枚举值，不含选择器特殊字符，无需 CSS.escape。
    const row = box.querySelector(`[data-source="${status.source}"]`);
    if (!row) return;
    const state = connectorState(status, activity.get(status.source) || 0, nowMs);
    const dot = row.querySelector(".connector-dot");
    if (dot) dot.dataset.state = state.dot;
    const hint = row.querySelector(".row-hint");
    if (hint) hint.textContent = state.hint;
  });
}

// 标题行右侧是带信息量的计数：一眼看出要不要动手。
// 未接入 > 0 用琥珀色，全接入用绿色；读取失败保留红字错误路径。
function renderConnectorHeadline() {
  const status = $("aw-connectors-status");
  const check = $("aw-connectors-check");
  const missing = Array.isArray(connectorStatuses)
    ? connectorStatuses.filter(item => !item.installed).length
    : -1;
  if (status) {
    if (missing < 0) {
      status.textContent = "读取失败";
      status.dataset.state = "error";
    } else if (missing > 0) {
      status.textContent = `${connectorStatuses.length - missing} 已接入 · ${missing} 未接入`;
      status.dataset.state = "missing";
    } else {
      status.textContent = `${connectorStatuses.length} 已接入`;
      status.dataset.state = "ready";
    }
  }
  // 有未接入时按钮才醒目；全接入时降为 ghost。
  if (check) check.classList.toggle("ghost", missing === 0);
}

async function loadConnectorStatuses() {
  try {
    connectorStatuses = await invoke("cmd_agent_connectors_status");
  } catch (e) {
    connectorStatuses = null;
    log("读取连接状态失败: " + e);
  }
  renderConnectorHeadline();
  renderConnectors();
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
  const amount = value => Number.isFinite(Number(value)) ? Math.max(0, Number(value)) : 0;
  const rows = [
    { label: "聊天", value: amount(today.chat_total_tokens), color: "#32835c" },
    { label: "截图理解", value: amount(today.vision_total_tokens), color: "#6389a5" },
    { label: "屏幕摘要", value: amount(today.screen_summary_total_tokens), color: "#b08c43" },
    { label: "记忆整理", value: amount(today.memory_aggregation_total_tokens), color: "#927aa6" },
  ];
  const classified = rows.reduce((sum, row) => sum + row.value, 0);
  const total = Math.max(amount(today.total_tokens), classified);
  if (total > classified) rows.push({ label: "其他", value: total - classified, color: "#8c9891" });
  let offset = 0;
  const segments = rows.map(row => {
    const percent = total ? row.value / total * 100 : 0;
    const segment = percent ? `<circle cx="80" cy="80" r="62" pathLength="100" fill="none" stroke="${row.color}" stroke-width="18" stroke-dasharray="${percent} ${100 - percent}" stroke-dashoffset="${-offset}" transform="rotate(-90 80 80)" />` : "";
    offset += percent;
    return segment;
  }).join("");
  $("usage-breakdown").innerHTML = `
    <div class="usage-donut" role="img" aria-label="${total ? "今日用量分布，分类数值见右侧明细" : "今日暂无用量"}">
      <svg viewBox="0 0 160 160" aria-hidden="true"><circle cx="80" cy="80" r="62" fill="none" stroke="#e5ebe7" stroke-width="18" />${segments}</svg>
      <div class="usage-donut-center"><strong>${compactNumber(total)}</strong><span>${total ? "今日用量" : "暂无用量"}</span></div>
    </div>
    <ul class="usage-legend" aria-label="用量分类明细">${rows.map(row => `
      <li><span class="usage-swatch" style="background:${row.color}" aria-hidden="true"></span><span class="usage-category">${row.label}</span><strong>${formatNumber(row.value)}</strong><span class="usage-percent">${total ? formatFixed(row.value / total * 100, 1) : "0"}%</span></li>
    `).join("")}</ul>`;
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
    // 单行聚合：总量 · 模型 · 条数与耗时 · 分类明细 · 时间。
    return `
      <div class="usage-session">
        <strong>${formatNumber(session.total_tokens)}</strong>
        <span class="usage-session-model">${escapeHtml((session.models || []).join(", ") || "未知模型")}</span>
        <span class="usage-session-meta">${formatNumber(session.record_count)} 条 · ${formatDuration(session.elapsed_ms_total)}</span>
        ${parts ? `<span class="usage-session-parts">${parts}</span>` : ""}
        <time class="usage-session-time">${escapeHtml(formatDateTime(session.ended_at))}</time>
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

// Five equal segments express importance without making a score the visual headline.
// Missing or invalid values remain unscored rather than implying the lowest importance.
function renderMemoryImportance(value) {
  const rated = Number.isInteger(value) && value >= 1 && value <= 5;
  const label = rated ? `重要程度：${value} / 5` : "重要程度：尚未评估";
  const semantics = rated
    ? `role="meter" aria-label="重要程度" aria-valuemin="1" aria-valuemax="5" aria-valuenow="${value}" aria-valuetext="${value} / 5"`
    : 'role="img" aria-label="重要程度：尚未评估"';
  return `<span class="memory-importance${rated ? "" : " unrated"}" ${semantics} title="${label}">${Array.from({ length: 5 }, (_, index) => `<i class="importance-segment${rated && index < value ? " filled" : ""}" aria-hidden="true"></i>`).join("")}</span>`;
}

function memoryRelativeTime(value, now = Date.now()) {
  if (!value) return "时间未记录";
  const time = new Date(value).getTime();
  if (!Number.isFinite(time)) return "时间未记录";
  const minutes = Math.max(0, Math.floor((now - time) / 60000));
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)} 小时前`;
  return `${Math.floor(minutes / 1440)} 天前`;
}

function renderMemoryReview(review) {
  const box = $("memory-review");
  if (!box) return;
  const entries = review?.entries || [];
  updateOverviewMemory(review);
  const heading = $("memory-review-summary");
  if (heading) heading.innerHTML = `<span>${formatNumber(review?.total_entries || 0)} 条记忆</span>${review?.generated_at ? `<span title="${escapeAttr(review.generated_at)}">更新于 ${escapeHtml(memoryRelativeTime(review.generated_at))}</span>` : ""}`;
  if (!entries.length) {
    box.innerHTML = `<div class="empty">暂无长期记忆。</div>`;
    return;
  }

  box.innerHTML = `
    ${entries.map(entry => {
      const tags = (entry.tags || []).map(tag => `<span class="memory-tag">${escapeHtml(tag)}</span>`).join("");
      const source = { conversation: "聊天中记住的", agent_reaction: "聊天中记住的", user: "你告诉它的", manual: "你告诉它的" }[entry.source] || "其他来源";
      const summary = entry.ai_reply || entry.user_msg || "";
      return `
        <div class="memory-entry">
          <div class="memory-entry-head">
            <div>
              <strong>${escapeHtml(entry.title)}</strong>
              <p>${escapeHtml(summary)}</p>
            </div>
            <button class="icon-btn danger memory-delete" type="button" data-id="${escapeAttr(entry.id)}" aria-label="删除记忆" title="删除记忆">删除</button>
          </div>
          <div class="memory-entry-meta memory-meta-inline">
            <time title="${escapeAttr(entry.timestamp)}">${escapeHtml(memoryRelativeTime(entry.timestamp))}</time>
            <span class="memory-source">${escapeHtml(source)}</span>
            ${renderMemoryImportance(entry.importance)}
            ${tags}
            ${entry.aggregated ? '<span class="memory-source">已整理</span>' : ""}
          </div>
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
    btn.addEventListener("click", async () => {
      const id = btn.dataset.id;
      if (id && await confirmDialog({
        title: "删除长期记忆",
        message: "这条记忆会从审查列表中移除。",
        okText: "删除",
      })) {
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
    btn.addEventListener("click", async () => {
      if (await confirmDialog({
        title: "取消提醒",
        message: "提醒会保留记录，但不再触发。",
        okText: "取消提醒",
      })) reminderAction("cmd_cancel_reminder", btn.dataset.id);
    });
  });
  box.querySelectorAll(".reminder-delete").forEach(btn => {
    btn.addEventListener("click", async () => {
      if (await confirmDialog({
        title: "删除提醒",
        message: "这个提醒会被彻底删除。",
        okText: "删除",
      })) reminderAction("cmd_delete_reminder", btn.dataset.id);
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

let savingAll = false;

async function saveAll() {
  if (connectionBusy || savingAll) return;
  savingAll = true;
  const saveBtn = $("btn-save");
  if (saveBtn) saveBtn.disabled = true;
  try {
    if (dirty.ai && !await saveAiConnection(false)) return;
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
      loadScreenTimeSummary();
      loadEarningsSummary();
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
  } finally {
    savingAll = false;
    if (saveBtn) saveBtn.disabled = false;
  }
}

// 每个分区恰好对应一个可重置的保存类别；专家模式按当前子页决定。
const SECTION_RESET_CATEGORY = {
  home: "permissions",
  memory: "user",
  cost: "ai",
  companion: "appearance",
};
const EXPERT_RESET_CATEGORY = {
  prompts: "prompts",
  "agent-watch": "agent_watch",
  actions: "actions",
  permissions: "permissions",
  diagnostics: "appearance",
};

async function resetCurrent() {
  let category = null;
  if (currentTab === "expert") {
    category = EXPERT_RESET_CATEGORY[currentExpertPage];
  } else {
    category = SECTION_RESET_CATEGORY[currentTab];
  }
  if (!category) return;
  const label = currentTab === "expert" ? `专家模式 · ${tabLabel(currentExpertPage)}` : tabLabel(currentTab);
  if (!await confirmDialog({
    title: "恢复默认",
    message: `将「${label}」恢复到默认配置。`,
    okText: "恢复",
  })) return;
  try {
    await invoke("cmd_settings_reset", { category });
    toast("已重置", "ok");
    await loadSnapshot();
    clearDirty(category);
  } catch (e) {
    toast("重置失败：" + String(e), "err");
  }
}

function tabLabel(t) {
  return ({
    home: "它能做什么",
    memory: "它记住了我什么",
    cost: "它花了多少钱",
    companion: "它怎么陪着我",
    expert: "专家模式",
    prompts: "提示词",
    actions: "按键与动作",
    permissions: "权限明细",
    diagnostics: "诊断与日志",
    "agent-watch": "编程助手看管",
    agent_watch: "编程助手看管",
    ai: "连接",
    user: "你告诉它的",
    appearance: "它的表现",
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
    loadScreenTimeSummary();
    loadEarningsSummary();
    renderPermissions(SNAPSHOT.permissions);
    renderAgentWatch(SNAPSHOT.agent_watch);
    renderAbout(SNAPSHOT.about);
    loadUsageDiagnostics();
    loadMemoryReview();
    loadReminders();
    if (!SNAPSHOT.permissions?.onboarding_completed) {
      switchTab("home");
      showWizard();
    }
    ["ai", "user", "actions", "prompts", "appearance", "permissions", "agent_watch"].forEach(clearDirty);
  } catch (e) {
    log("加载失败: " + e);
    toast("加载配置失败：" + String(e), "err");
  }
}

async function tryClose() {
  if (anyDirty()) {
    if (!await confirmDialog({
      title: "放弃未保存修改",
      message: "当前设置有改动，关闭后不会保存。",
      okText: "放弃",
    })) return;
  }
  try { await invoke("cmd_settings_close"); } catch {}
}

function currentSettingsWindow() {
  try {
    return window.__TAURI__?.window?.getCurrentWindow?.() || null;
  } catch (e) {
    log("settings getCurrentWindow failed: " + e);
    return null;
  }
}

function isInteractiveDragTarget(target) {
  return !!target?.closest?.([
    "button",
    "input",
    "select",
    "textarea",
    "a",
    "summary",
    "[role='button']",
    "[contenteditable='true']",
    ".pet-asset-card",
  ].join(","));
}

function bindWindowDrag() {
  const win = currentSettingsWindow();
  if (!win?.startDragging) return;
  document.querySelectorAll(".nav-brand, .pane-head").forEach((handle) => {
    handle.addEventListener("pointerdown", async (event) => {
      if (event.button !== 0 || isInteractiveDragTarget(event.target)) return;
      event.preventDefault();
      try {
        await win.startDragging();
      } catch (e) {
        log("settings drag failed: " + e);
      }
    });
  });
}

// 刷新按钮 loading 态：点击后短暂禁用并提示，避免重复点击
function bindRefreshButton(id, fn) {
  const btn = $(id);
  if (!btn) return;
  btn.addEventListener("click", async () => {
    if (btn.disabled) return;
    const label = btn.textContent;
    btn.disabled = true;
    btn.textContent = "刷新中";
    try {
      await fn();
    } finally {
      btn.disabled = false;
      btn.textContent = label;
    }
  });
}

function bindGlobal() {
  bindWindowDrag();
  document.querySelectorAll(".nav-item").forEach(btn => {
    btn.addEventListener("click", () => switchTab(btn.dataset.tab));
  });
  document.querySelectorAll(".expert-subnav-item").forEach(btn => {
    btn.addEventListener("click", () => switchExpertPage(btn.dataset.expert));
  });
  document.querySelectorAll("[data-goto]").forEach(el => {
    const go = () => switchTab(el.dataset.goto);
    el.addEventListener("click", go);
    el.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        go();
      }
    });
  });
  // 首页速答条：本分区内的条目点击滚到对应开关，跨分区的走 data-goto。
  document.querySelectorAll("[data-scrollto]").forEach(el => {
    const go = () => {
      const target = document.getElementById(el.dataset.scrollto);
      target?.closest(".section-panel")?.scrollIntoView({ behavior: "smooth", block: "center" });
    };
    el.addEventListener("click", go);
    el.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        go();
      }
    });
  });
  $("perm-tools-jump").addEventListener("click", () => {
    switchTab("expert");
    switchExpertPage("permissions");
  });
  $("revoke-all").addEventListener("click", revokeAllPermissions);
  bindCameraMaster();
  $("btn-close").addEventListener("click", tryClose);
  $("btn-cancel").addEventListener("click", async () => {
    await loadSnapshot();
    toast("已取消修改", "ok");
  });
  $("btn-save").addEventListener("click", saveAll);
  $("btn-reset").addEventListener("click", resetCurrent);
  $("usage-model").addEventListener("change", () => {
    selectedUsageModel = $("usage-model").value || "__all";
    loadTokenStats();
  });
  bindRefreshButton("overview-refresh", async () => {
    await loadUsageDiagnostics();
    await loadMemoryReview();
    await loadReminders();
  });
  bindRefreshButton("memory-refresh", loadMemoryReview);
  bindRefreshButton("reminder-refresh", loadReminders);
  bindRefreshButton("usage-refresh", loadUsageDiagnostics);
  // 连接器：行内"修复"只修该来源；"检查全部连接"先探测，再幂等修复未接入的。
  $("aw-connectors").addEventListener("click", async (event) => {
    const btn = event.target.closest("[data-repair-source]");
    if (!btn || connectorBusy) return;
    const source = btn.dataset.repairSource;
    connectorBusy = true;
    btn.disabled = true;
    try {
      const msg = await invoke("cmd_repair_connector", { source });
      toast(msg || `${window.AgentSources.label(source)} 连接已修复`, "ok");
    } catch (e) {
      toast(`${window.AgentSources.label(source)} 修复失败：` + String(e), "err");
    } finally {
      connectorBusy = false;
      await loadConnectorStatuses();
    }
  });
  $("aw-connectors-check").addEventListener("click", async () => {
    if (connectorBusy) return;
    const btn = $("aw-connectors-check");
    connectorBusy = true;
    btn.disabled = true;
    try {
      connectorStatuses = await invoke("cmd_agent_connectors_status");
      const repaired = [];
      for (const item of connectorStatuses.filter(entry => !entry.installed)) {
        try {
          await invoke("cmd_repair_connector", { source: item.source });
          repaired.push(window.AgentSources.label(item.source));
        } catch (e) {
          toast(`${window.AgentSources.label(item.source)} 修复失败：` + String(e), "err");
        }
      }
      connectorStatuses = await invoke("cmd_agent_connectors_status");
      renderConnectorHeadline();
      renderConnectors();
      toast(repaired.length ? `已修复连接：${repaired.join("、")}` : "全部连接就绪", "ok");
    } catch (e) {
      toast("连接检查失败：" + String(e), "err");
    } finally {
      connectorBusy = false;
      btn.disabled = false;
    }
  });
  const eventApi = window.__TAURI__?.event;
  if (eventApi?.listen) {
    eventApi.listen("agent-session-update", (event) => {
      if (currentTab === "expert" && currentExpertPage === "agent-watch") renderAgentSessions(event.payload);
    });
    eventApi.listen("reminders-updated", () => {
      if (currentTab === "companion" || currentTab === "home") loadReminders();
    });
  }
  bindAgentWatchCopyActions();
  $("ai-key-toggle").addEventListener("click", () => {
    const el = $("ai-key");
    const show = el.type === "password";
    el.type = show ? "text" : "password";
    $("ai-key-toggle").setAttribute("aria-label", show ? "隐藏新输入的密钥" : "显示新输入的密钥");
  });
  window.addEventListener("keydown", (e) => {
    if (confirmDialogOpen()) return;
    if (e.key === "Escape") {
      // 向导模态打开时忽略 Esc：跳过要走显式的「稍后再说」，避免模态态误关窗。
      if (!$("onboarding-wizard")?.classList.contains("hidden")) return;
      tryClose();
    }
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
  const hint = $("ss-interval-hint");
  const screenshotOn = $("perm-screenshot")?.checked;
  const interval = Number(appearance?.screenshot_interval_sec ?? 30);
  if (hint) hint.textContent = screenshotOn ? `每 ${formatNumber(interval)} 秒一次` : "看不到你的屏幕";
  updateHomePermissionCards();
}

function updateOverviewMemory(review) {
  const count = $("ov-memory-total");
  count.textContent = review ? `${formatNumber(review.total_entries || 0)} 件事` : "等待";
  count.title = review?.generated_at ? `最近更新 ${formatDateTime(review.generated_at)}` : "";
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
    if (payload.body) parts.push(shortText(payload.body, 48));
    if (payload.ttl_ms) parts.push(`${formatDuration(payload.ttl_ms)} 后恢复`);
    if (payload.refresh) parts.push("刷新现有状态");
    return parts.join(" · ");
  }
  if (type === "react") {
    const parts = [`反应：${formatPetMood(payload.mood)}`];
    if (payload.speech) parts.push(shortText(payload.speech, 48));
    if (payload.ttl_ms) parts.push(`${formatDuration(payload.ttl_ms)} 后恢复`);
    return parts.join(" · ");
  }
  if (type === "set_mode") return `模式：${formatPetMode(payload.mode)}`;
  if (type === "show_bubble") return `气泡：${shortText(payload.text || "", 72)}`;
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

// ─── 积分与成就系统渲染 ───

const POINTS_CATEGORY_LABELS = {
  Chat: "对话", Memory: "记忆", Routine: "日常",
  Fun: "娱乐", Observation: "观察", Bond: "互动", Daily: "陪伴",
};

let achievementsExpanded = false;
let lastAchievementsView = null;

async function loadPointsState() {
  try {
    const view = await invoke("cmd_get_points_state");
    renderPointsLevel(view.state);
    renderPointsBreakdown(view.state);
    renderAchievements(view.achievements, view.state);
    renderPointsEvents(view.recent_events);
    // 彩蛋与徽章展开都是显式动作，用 onclick 赋值保证重复加载不叠监听。
    const badge = document.getElementById("points-level-badge");
    if (badge) badge.onclick = () => togglePointsSnake();
    const more = document.getElementById("achievements-more");
    if (more) more.onclick = () => {
      achievementsExpanded = !achievementsExpanded;
      if (lastAchievementsView) renderAchievements(lastAchievementsView.achievements, lastAchievementsView.state);
    };
  } catch (e) {
    log("加载积分状态失败: " + e);
  }
}

function renderPointsLevel(state) {
  const el = (id) => document.getElementById(id);
  const lv = el("points-level");
  const title = el("points-level-title");
  const fill = el("points-exp-fill");
  const bar = el("points-exp-bar");
  const expText = el("points-exp-text");
  const longestStreak = el("points-longest-streak");

  const level = Number(state.level || 1);
  if (lv) lv.textContent = level;
  if (title) title.textContent = state.level_title || "-";

  const expIn = Number(state.experience_in_current || 0);
  const expNext = Number(state.experience_to_next || 0);
  const pct = expNext > 0 ? Math.min(100, Math.round((expIn / expNext) * 100)) : 100;
  if (fill) fill.style.width = pct + "%";
  // 折叠摘要行已有等级/总分/连续，这里只补"还差多少"；原始进度放 tooltip。
  if (expText) {
    expText.textContent = expNext > 0
      ? `距 Lv.${level + 1} 还差 ${formatNumber(Math.max(0, expNext - expIn))} 分`
      : "已是最高等级";
  }
  if (bar) bar.title = expNext > 0 ? `${formatNumber(expIn)} / ${formatNumber(expNext)}` : "";
  if (longestStreak) longestStreak.textContent = state.longest_streak_days || 0;

  // 折叠态摘要行：Lv2 熟悉 · 42 分 · 连续 3 天
  const recall = el("recall-summary");
  if (recall) {
    const parts = [`Lv.${level} ${state.level_title || ""}`.trim(), `${formatNumber(state.total_points || 0)} 分`];
    if (state.current_streak_days) parts.push(`连续 ${state.current_streak_days} 天`);
    recall.textContent = parts.join(" · ");
  }
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

  // 单行 caption：分类计数不值得一条横幅。
  container.innerHTML = items
    .filter(([, , v]) => v > 0)
    .map(([key, , value]) => `<span>${POINTS_CATEGORY_LABELS[key] || key} <b>${formatNumber(value)}</b></span>`)
    .join("") || "<span>暂无成长记录</span>";
}

// ── 贪吃蛇彩蛋：点等级徽章显式触发，跑 12 秒自动收起 ──
// 不再寄生在"热力图"上：那张图的格子是装饰性伪数据，已移除。
function togglePointsSnake() {
  const stage = document.getElementById("points-snake-stage");
  if (!stage) return;
  clearTimeout(togglePointsSnake._timer);
  if (!stage.classList.contains("hidden")) {
    stopPointsSnake();
    stage.classList.add("hidden");
    stage.innerHTML = "";
    return;
  }
  stage.classList.remove("hidden");
  stage.innerHTML = `
    <svg class="points-snake-layer" viewBox="0 0 828 108" aria-hidden="true">
      <polyline class="points-snake-line" points=""></polyline>
      <circle class="points-snake-head-dot" r="5"></circle>
    </svg>
    <span class="points-snake-note">彩蛋：小蛇散步中，再点一次徽章收起</span>
  `;
  pointsSnakePath = buildPointsSnakePath();
  pointsSnakeLength = 7;
  pointsSnakeOffset = 0;
  drawPointsSnake();
  startPointsSnake();
  togglePointsSnake._timer = setTimeout(() => {
    stopPointsSnake();
    stage.classList.add("hidden");
    stage.innerHTML = "";
  }, 12000);
}

function buildPointsSnakePath() {
  const path = [];
  const route = [
    [38, 5], [39, 5], [40, 5], [41, 5],
    [41, 4], [42, 4], [43, 4], [44, 4],
    [44, 3], [45, 3], [46, 3], [47, 3],
    [47, 4], [48, 4], [49, 4], [50, 4],
    [50, 5], [49, 5], [48, 5], [47, 5],
    [47, 6], [46, 6], [45, 6], [44, 6],
    [44, 5], [43, 5], [42, 5], [41, 5],
    [40, 5], [39, 5], [38, 5],
  ];

  for (const [col, row] of route) {
    const index = col * 7 + row;
    if (path[path.length - 1] !== index) {
      path.push(index);
    }
  }
  return path;
}

function startPointsSnake() {
  if (pointsSnakeRaf) return;
  pointsSnakeLastFrame = performance.now();
  pointsSnakeRaf = window.requestAnimationFrame(animatePointsSnake);
}

function stopPointsSnake() {
  if (!pointsSnakeRaf) return;
  window.cancelAnimationFrame(pointsSnakeRaf);
  pointsSnakeRaf = null;
}

function animatePointsSnake(now) {
  const elapsed = Math.min(80, now - pointsSnakeLastFrame);
  pointsSnakeLastFrame = now;
  if (pointsSnakePath.length) {
    pointsSnakeOffset = (pointsSnakeOffset + (elapsed / 1000) * 3.2) % pointsSnakePath.length;
    drawPointsSnake();
  }
  pointsSnakeRaf = window.requestAnimationFrame(animatePointsSnake);
}

function drawPointsSnake() {
  const stage = document.getElementById("points-snake-stage");
  const line = stage?.querySelector(".points-snake-line");
  const head = stage?.querySelector(".points-snake-head-dot");
  if (!line || !head || !pointsSnakePath.length || !pointsSnakeLength) return;

  const sampleCount = pointsSnakeLength * 5;
  const points = [];
  for (let i = 0; i <= sampleCount; i += 1) {
    const offset = pointsSnakeOffset - pointsSnakeLength + (i / sampleCount) * pointsSnakeLength;
    const point = pointsSnakePointAt(offset);
    points.push(`${point.x.toFixed(2)},${point.y.toFixed(2)}`);
  }

  const headPoint = pointsSnakePointAt(pointsSnakeOffset);
  line.setAttribute("points", points.join(" "));
  head.setAttribute("cx", headPoint.x.toFixed(2));
  head.setAttribute("cy", headPoint.y.toFixed(2));
}

function pointsSnakePointAt(offset) {
  const length = pointsSnakePath.length;
  const normalized = ((offset % length) + length) % length;
  const startIndex = Math.floor(normalized);
  const progress = normalized - startIndex;
  const from = pointsCellPoint(pointsSnakePath[startIndex]);
  const to = pointsCellPoint(pointsSnakePath[(startIndex + 1) % length]);
  return {
    x: from.x + (to.x - from.x) * progress,
    y: from.y + (to.y - from.y) * progress,
  };
}

function pointsCellPoint(index) {
  const size = 12;
  const gap = 4;
  const col = Math.floor(index / 7);
  const row = index % 7;
  return {
    col,
    row,
    x: col * (size + gap) + size / 2,
    y: row * (size + gap) + size / 2,
  };
}

function renderAchievements(achievements, state) {
  const grid = document.getElementById("achievements-grid");
  const countEl = document.getElementById("achievement-count");
  const more = document.getElementById("achievements-more");
  if (!grid) return;

  achievements = Array.isArray(achievements) ? achievements : [];
  lastAchievementsView = { achievements, state };
  const unlockedCount = achievements.filter((a) => a.unlocked).length;

  if (countEl) countEl.textContent = unlockedCount;

  if (achievements.length === 0) {
    grid.innerHTML = '<div class="points-empty">暂无成长记录</div>';
    if (more) more.hidden = true;
    return;
  }

  // 未解锁的灰卡不常驻两屏：默认只展示已解锁，折叠成一行名单按需展开。
  const locked = achievements.filter((a) => !a.unlocked);
  const shown = achievementsExpanded ? achievements : achievements.filter((a) => a.unlocked);
  grid.innerHTML = shown
    .map((a) => {
      const cls = a.unlocked ? "achievement-badge unlocked" : "achievement-badge locked";
      const icon = a.unlocked || !a.hidden ? a.icon : "?";
      return `<div class="${cls}" title="${escapeHtml(a.description)}">
        <span class="achievement-icon">${icon}</span>
        <span class="achievement-name">${escapeHtml(a.name)}</span>
        ${a.unlocked ? `<span class="achievement-bonus">+${a.points_reward}</span>` : ""}
      </div>`;
    })
    .join("") || '<div class="points-empty">还没有解锁的徽章</div>';

  if (more) {
    if (!locked.length) {
      more.hidden = true;
    } else if (achievementsExpanded) {
      more.hidden = false;
      more.textContent = "收起未解锁";
    } else {
      const names = locked.slice(0, 4).map((a) => a.name).join("、");
      more.hidden = false;
      more.textContent = `还有 ${locked.length} 枚：${names}${locked.length > 4 ? "…" : ""}（展开）`;
    }
  }
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
    InvasionPlayed: "桌面入侵",
    InvasionWon: "入侵守住",
    InvasionFlawlessWin: "无损守护",
    ScreenshotObserved: "截图观察",
    CameraObserved: "摄像头观察",
    PetPraised: "夸奖宠物",
    DailyLogin: "每日登录",
  };
  return map[kind] || kind || "-";
}

document.addEventListener("DOMContentLoaded", () => {
  bindGlobal();
  bindConnection();
  bindPetPreview();
  setupWizard();
  loadSnapshot();
});

if (typeof window !== "undefined") {
  window.__settingsTest = {
    confirmDialog,
    collectConnectionDraft,
    renderAi,
    bindConnection,
    testAiConnection,
    saveAiConnection,
    getDirty: () => ({ ...dirty }),
    markDirty,
    renderUsageBreakdown,
    renderMemoryImportance,
    memoryRelativeTime,
    renderPetAssetPreview,
    drawPetAssetPreview,
    petPreviewFrames,
    petPreviewFrameAt,
    availablePetPreviewStates,
    bindPetPreview,
    cyclePetPreview,
    stopPetPreview,
    setPetPreviewAsset: asset => { petPreview.asset = asset; petPreview.index = 0; },
    formatReminderSchedule,
    reminderDescription,
    renderReminders,
    setWizardStep,
    finishWizard,
    // 闭包内写 SNAPSHOT：eval 环境下顶层 let 不进全局词法，外部无法直接赋值。
    setSnapshot: (value) => { SNAPSHOT = value; },
    renderConnectors,
    renderConnectorHeadline,
    connectorState,
    connectorAgoLabel,
    // 闭包内写连接器状态，理由同 setSnapshot。
    setConnectorStatuses: (value) => { connectorStatuses = value; },
    setLatestAgentSnapshot: (value) => { latestAgentSnapshot = value; },
    renderActionItem,
    keyMetaText,
    positionLabel,
    shortcutChips,
    renderShortcutChips,
    updatePermissionGateSummary,
  };
}
