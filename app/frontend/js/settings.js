// 8Bit Cat 设置界面逻辑
// - 启动拉取 cmd_settings_load
// - 左侧 tab 切换 + dirty 检测
// - 底部保存/取消/重置，Esc 关闭

const { invoke } = window.__TAURI__.core;

const ACTION_TYPES = ["launch", "hotkey", "script", "voice"];

// 全量快照（来自后端）
let SNAPSHOT = null;
// 各分类 dirty 标记
const dirty = { ai: false, actions: false, prompts: false, appearance: false };
// 当前激活 tab
let currentTab = "ai";

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
}

// ---- 渲染各分类 ----

function renderAi(ai) {
  $("ai-key").value = ai.overlay.api_key || "";
  $("ai-baseurl").value = ai.overlay.base_url || "";
  $("ai-model").value = ai.overlay.model || "";
  $("ai-maxtokens").value = ai.overlay.max_tokens == null ? "" : ai.overlay.max_tokens;

  const eff = ai.effective;
  $("ai-effective").innerHTML =
    `<div>当前生效：</div>` +
    `<div>• API Key：<b>${ai.has_effective_key ? "已配置" : "（空）"}</b></div>` +
    `<div>• Base URL：<b>${escapeHtml(eff.base_url)}</b></div>` +
    `<div>• 模型：<b>${escapeHtml(eff.model)}</b></div>` +
    `<div>• Max Tokens：<b>${eff.max_tokens}</b></div>`;

  ["ai-key", "ai-baseurl", "ai-model", "ai-maxtokens"].forEach(id => {
    $(id).oninput = () => markDirty("ai");
  });
}

function renderActions(actionsView) {
  $("actions-term").value = actionsView.defaults.terminal || "";
  $("actions-win").value = actionsView.defaults.window || "";
  $("actions-term").oninput = () => markDirty("actions");
  $("actions-win").oninput = () => markDirty("actions");

  const list = $("actions-list");
  list.innerHTML = "";
  const keys = Object.keys(actionsView.actions).sort();
  for (const key of keys) {
    list.appendChild(renderActionItem(key, actionsView.actions[key]));
  }
}

function renderActionItem(key, def) {
  const el = document.createElement("div");
  el.className = "action-item";
  el.dataset.key = key;
  el.innerHTML = `
    <div class="ai-head">
      <span class="key">${escapeHtml(key)}</span>
      <select class="a-type">
        ${ACTION_TYPES.map(t => `<option value="${t}" ${t === def.action_type ? "selected" : ""}>${t}</option>`).join("")}
      </select>
      <span class="hint" style="margin:0; font-size:11px;">trigger: ${def.trigger ? escapeHtml(def.trigger.join("+")) : "—"}</span>
    </div>
    <div class="ai-body"></div>
  `;

  const body = el.querySelector(".ai-body");
  renderActionBody(body, def);

  const sel = el.querySelector(".a-type");
  sel.addEventListener("change", () => {
    def.action_type = sel.value;
    renderActionBody(body, def);
    markDirty("actions");
  });
  return el;
}

function renderActionBody(body, def) {
  body.innerHTML = "";
  const mk = (label, id, val, type = "text") => {
    const row = document.createElement("div");
    row.className = "row";
    row.innerHTML = `<label>${label}</label><input data-field="${id}" type="${type}" value="${escapeAttr(val ?? "")}" />`;
    body.appendChild(row);
    row.querySelector("input").oninput = () => markDirty("actions");
    return row;
  };
  const mkToggle = (label, id, val) => {
    const row = document.createElement("div");
    row.className = "row toggle";
    row.innerHTML = `<label>${label}</label><input data-field="${id}" type="checkbox" ${val ? "checked" : ""} />`;
    body.appendChild(row);
    row.querySelector("input").onchange = () => markDirty("actions");
  };

  const t = def.action_type;
  if (t === "launch") {
    mk("程序 program", "program", def.program || "");
    mk("参数 args", "args", def.args || "");
    mk("工作目录 workdir", "workdir", def.workdir || "");
    mkToggle("终端启动 terminal", "terminal", !!def.terminal);
  } else if (t === "hotkey" || t === "script") {
    mk("命令 command", "command", def.command || "");
  } else if (t === "voice") {
    const trig = def.voice?.trigger?.join(",") ?? "";
    const delay = def.voice?.delay ?? 1.0;
    mk("语音触发键 trigger (逗号分隔)", "voice-trigger", trig);
    mk("延迟 delay (秒)", "voice-delay", delay, "number");
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

  ["a-top","a-collapsed","a-tts"].forEach(id => { $(id).onchange = () => markDirty("appearance"); });
  $("a-shortcut").oninput = () => markDirty("appearance");
}

function collectAppearance() {
  return {
    always_on_top: $("a-top").checked,
    default_collapsed: $("a-collapsed").checked,
    tts_enabled: $("a-tts").checked,
    global_shortcut: $("a-shortcut").value.trim() || "CommandOrControl+Alt+Space",
  };
}

function renderAbout(a) {
  $("about-version").textContent = a.version;
  $("about-settings-path").textContent = a.app_settings_path;
  $("about-actions-hint").textContent = a.actions_yml_hint;
  $("about-prompts-hint").textContent = a.prompts_yml_hint;
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
  if (currentTab === "about") return;
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

// 启动
document.addEventListener("DOMContentLoaded", () => {
  bindGlobal();
  loadSnapshot();
});
