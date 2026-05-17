(function () {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  const shell = document.getElementById("agent-watch");
  const watchHeader = document.getElementById("watch-header");
  const watchTitle = document.getElementById("watch-title");
  const stack = document.getElementById("stack");
  const watchCount = document.getElementById("watch-count");
  const watchActions = document.querySelector(".watch-actions");
  const watchExpandToggle = document.getElementById("watch-expand-toggle");
  const collapsed = new Set(JSON.parse(localStorage.getItem("agentWatchCollapsed") || "[]"));
  let folded = localStorage.getItem("agentWatchFolded") === "true";
  let latest = null;
  let suppressNextHeaderClick = false;

  function escapeHtml(value) {
    return String(value ?? "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function escapeAttr(value) {
    return escapeHtml(value).replace(/'/g, "&#39;");
  }

  function saveCollapsed() {
    localStorage.setItem("agentWatchCollapsed", JSON.stringify([...collapsed]));
  }

  function basename(path) {
    const parts = String(path || "").split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] || path;
  }

  function middleEllipsis(value, max = 64) {
    const text = String(value ?? "").replace(/\s+/g, " ").trim();
    if (text.length <= max) return text;
    const head = Math.max(12, Math.floor(max * 0.58));
    const tail = Math.max(8, max - head - 1);
    return `${text.slice(0, head)}…${text.slice(-tail)}`;
  }

  function markdownText(value) {
    return String(value || "")
      .replace(/```[\s\S]*?```/g, " 代码块 ")
      .replace(/`([^`]+)`/g, "$1")
      .replace(/!\[([^\]]*)\]\([^)]+\)/g, "$1")
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
      .replace(/^\s{0,3}#{1,6}\s*/gm, "")
      .replace(/^\s{0,3}>\s?/gm, "")
      .replace(/^\s{0,3}(?:[-*+]|\d+[.)])\s+/gm, "")
      .replace(/\*\*([^*]+)\*\*/g, "$1")
      .replace(/__([^_]+)__/g, "$1")
      .replace(/\*([^*]+)\*/g, "$1")
      .replace(/_([^_]+)_/g, "$1")
      .replace(/~~([^~]+)~~/g, "$1")
      .replace(/\s*[-—]{3,}\s*/g, " ")
      .trim();
  }

  function compactText(value, max = 72) {
    return middleEllipsis(markdownText(value).replace(/\s+/g, " "), max);
  }

  function tryParseJson(text) {
    const value = String(text || "").trim();
    if (!value.startsWith("{") && !value.startsWith("[")) return null;
    try {
      return JSON.parse(value);
    } catch (_) {
      return null;
    }
  }

  function unescapeJsonPreview(value) {
    return String(value || "")
      .replace(/\\\\/g, "\\")
      .replace(/\\"/g, "\"")
      .replace(/\\n/g, " ")
      .replace(/\\r/g, " ")
      .replace(/\\t/g, " ")
      .trim();
  }

  function previewField(text, key) {
    const source = String(text || "");
    const pattern = new RegExp(`"${key}"\\s*:\\s*"((?:\\\\.|[^"\\\\])*)`);
    const match = source.match(pattern);
    return match ? unescapeJsonPreview(match[1]) : "";
  }

  function extractPreviewFields(text) {
    const input = {};
    for (const key of [
      "description",
      "task_description",
      "command",
      "file_path",
      "path",
      "pattern",
      "url",
      "task_id",
      "prompt",
      "subagent_type",
    ]) {
      const value = previewField(text, key);
      if (value) input[key] = value;
    }
    const timeoutMatch = String(text || "").match(/"timeout"\s*:\s*"?(\d+)/);
    if (timeoutMatch) input.timeout = timeoutMatch[1];
    return Object.keys(input).length ? input : null;
  }

  function commandSummary(command) {
    const text = String(command || "").replace(/\s+/g, " ").trim();
    if (!text) return "";
    const pip = text.match(/\bpip(?:\d+)?\s+install\s+([^;&|]+)/i);
    if (pip) return `pip install ${middleEllipsis(pip[1], 34)}`;
    const npm = text.match(/\b(?:npm|pnpm|yarn)\s+([a-z]+)(?:\s+([^;&|]+))?/i);
    if (npm) return `${npm[1]} ${middleEllipsis(npm[2] || "", 34)}`.trim();
    const cargo = text.match(/\bcargo\s+([a-z-]+)(?:\s+([^;&|]+))?/i);
    if (cargo) return `cargo ${cargo[1]} ${middleEllipsis(cargo[2] || "", 34)}`.trim();
    const python = text.match(/\bpython(?:\.exe|3)?\s+(-m\s+\S+|\S+)/i);
    if (python) return `python ${middleEllipsis(python[1], 36)}`;
    return middleEllipsis(text, 58);
  }

  function toolVerb(tool) {
    const name = String(tool || "").toLowerCase();
    if (name.includes("bash")) return "执行";
    if (name.includes("read")) return "读取";
    if (name.includes("edit")) return "编辑";
    if (name.includes("write")) return "写入";
    if (name.includes("grep")) return "搜索";
    if (name.includes("agent")) return "子任务";
    if (name.includes("playwright") || name.includes("browser")) return "浏览器";
    return "工具";
  }

  function toolLabel(tool) {
    const raw = String(tool || "").trim();
    if (!raw) return "";
    const lower = raw.toLowerCase();
    if (lower === "bash") return "Shell";
    if (lower === "read") return "Read";
    if (lower === "edit") return "Edit";
    if (lower === "write") return "Write";
    if (lower === "grep") return "Search";
    if (lower === "agent") return "Agent";
    if (lower.includes("playwright") || lower.includes("browser")) return "Browser";
    return raw.replace(/^mcp__/, "").replace(/__/g, " / ");
  }

  function classifySession(session) {
    const tool = String(session.tool_name || "").toLowerCase();
    if (tool.includes("read")) return "READ";
    if (tool.includes("edit") || tool.includes("write")) return "WRITE";
    if (tool.includes("bash")) return "COMMAND";
    if (tool.includes("grep")) return "SEARCH";
    if (tool.includes("agent")) return "AGENT";
    if (tool.includes("playwright") || tool.includes("browser")) return "BROWSER";
    if ((session.status || "").toLowerCase() === "waiting") return "WAITING";
    if ((session.status || "").toLowerCase() === "done") return "DONE";
    return "TASK";
  }

  function targetOf(session, input) {
    if (input) {
      if (input.file_path) return basename(input.file_path);
      if (input.path) return basename(input.path);
      if (input.command) return commandSummary(input.command);
      if (input.pattern) return input.pattern;
      if (input.description) return input.description;
      if (input.task_description) return input.task_description;
      if (input.prompt) return input.prompt;
      if (input.url) return input.url;
    }
    return session.workspace_name || session.workspace || session.session_id;
  }

  function describeToolInput(session, input) {
    const tool = session.tool_name || "";
    const name = String(tool).toLowerCase();
    const description = input.description || input.task_description;
    if (name.includes("agent")) {
      return description
        ? `子任务 ${middleEllipsis(description, 56)}`
        : `子任务 ${middleEllipsis(input.prompt || input.subagent_type || "", 56)}`;
    }
    if (description && name.includes("bash")) {
      return `${middleEllipsis(description, 44)} · ${commandSummary(input.command)}`;
    }
    if (input.command) return `执行 ${commandSummary(input.command)}`;
    if (input.file_path && name.includes("read")) {
      const range = input.offset ? `:${input.offset}${input.limit ? `+${input.limit}` : ""}` : "";
      return `读取 ${middleEllipsis(basename(input.file_path), 38)}${range}`;
    }
    if (input.file_path && name.includes("edit")) {
      return `编辑 ${middleEllipsis(basename(input.file_path), 42)}`;
    }
    if (input.file_path && name.includes("write")) {
      return `写入 ${middleEllipsis(basename(input.file_path), 42)}`;
    }
    if (input.file_path) return `${toolVerb(tool)} ${middleEllipsis(basename(input.file_path), 42)}`;
    if (input.path && input.pattern) {
      return `搜索 ${middleEllipsis(input.pattern, 28)} · ${middleEllipsis(basename(input.path), 24)}`;
    }
    if (input.pattern) return `搜索 ${middleEllipsis(input.pattern, 50)}`;
    if (input.url) return `打开 ${middleEllipsis(input.url, 58)}`;
    if (input.task_id) {
      const timeout = input.timeout ? ` · ${Math.round(Number(input.timeout) / 1000)}s` : "";
      return `任务 ${middleEllipsis(input.task_id, 18)}${timeout}`;
    }
    if (description) return `${toolVerb(tool)} ${middleEllipsis(description, 58)}`;
    const key = Object.keys(input).find((item) => input[item] != null);
    return key ? `${key}: ${middleEllipsis(input[key], 50)}` : toolVerb(tool);
  }

  async function syncFoldedWindow() {
    if (!invoke) return;
    try {
      await invoke("cmd_agent_watch_set_folded", { folded });
    } catch (e) {
      console.error("[agent-watch] resize failed", e);
    }
  }

  function setFolded(next, syncWindow = true) {
    folded = next;
    localStorage.setItem("agentWatchFolded", String(folded));
    shell.classList.toggle("folded", folded);
    const title = folded ? "展开任务栈" : "折叠任务栈";
    watchHeader.title = title;
    watchHeader.setAttribute("aria-label", title);
    watchHeader.setAttribute("aria-expanded", String(!folded));
    if (syncWindow) syncFoldedWindow();
  }

  function isDone(session) {
    return String(session.status || "").toLowerCase() === "done";
  }

  function isError(session) {
    return String(session.status || "").toLowerCase() === "error";
  }

  function attentionRank(session) {
    const status = String(session.status || "").toLowerCase();
    if (status === "waiting" || status === "error") return 0;
    if (status === "working" || status === "tool_running" || status === "compacting") return 1;
    if (status === "done") return 2;
    return 3;
  }

  function sortedSessions(sessions) {
    return [...sessions].sort((left, right) => {
      const byAttention = attentionRank(left) - attentionRank(right);
      if (byAttention) return byAttention;
      return Number(left.age_sec ?? 0) - Number(right.age_sec ?? 0);
    });
  }

  function responseExcerpt(value) {
    const lines = markdownText(value)
      .split(/\r?\n/)
      .map((part) => part.replace(/\s+/g, " ").trim())
      .filter(Boolean)
      .filter((part) => !/^(好的|已完成|完成了|直说[:：]?)$/i.test(part))
      .filter((part) => !/^#+$/.test(part));
    const selected = [];
    let total = 0;
    for (const line of lines) {
      selected.push(line.replace(/^[-*]\s+/, ""));
      total += line.length;
      if (selected.length >= 3 || total >= 110) break;
    }
    return selected.join(" · ");
  }

  function summaryOf(session) {
    const done = isDone(session);
    const raw = done || isError(session)
      ? session.last_response_preview
        || session.user_prompt_preview
        || session.tool_input_preview
        || session.workspace
        || session.session_id
      : session.user_prompt_preview
        || session.tool_input_preview
        || session.last_response_preview
        || session.workspace
        || session.session_id;
    const parsed = tryParseJson(raw);
    if (parsed && !Array.isArray(parsed)) {
      return describeToolInput(session, parsed);
    }
    const previewInput = extractPreviewFields(raw);
    if (previewInput) return describeToolInput(session, previewInput);
    if (done) return responseExcerpt(raw) || "任务已完成";
    return compactText(raw, 72);
  }

  function viewOf(session) {
    if (session.display) {
      return {
        kind: session.display.action_label || classifySession(session),
        target: session.display.headline || session.workspace_name || targetOf(session, null),
        detail: session.display.detail || session.display.project || summaryOf(session),
        project: session.display.project || session.workspace_name || "",
        source: session.display.source_label || agentSourceLabel(session.source),
        age: session.display.age_label || ageLabel(session.age_sec),
        tone: session.display.tone || session.status || "idle",
        quiet: Boolean(session.display.quiet),
      };
    }
    const done = isDone(session);
    const raw = done || isError(session)
      ? session.last_response_preview
        || session.user_prompt_preview
        || session.tool_input_preview
        || ""
      : session.user_prompt_preview
        || session.tool_input_preview
        || session.last_response_preview
        || "";
    const parsed = tryParseJson(raw);
    const input = parsed && !Array.isArray(parsed) ? parsed : extractPreviewFields(raw);
    const kind = classifySession(session);
    const target = middleEllipsis(done ? session.workspace_name || targetOf(session, input) : targetOf(session, input), 42);
    const detail = input && !done ? describeToolInput(session, input) : summaryOf(session);
    return {
      kind,
      target,
      detail: compactText(detail, 96),
      project: session.workspace_name || "",
      source: agentSourceLabel(session.source),
      age: ageLabel(session.age_sec),
      tone: session.status || "idle",
      quiet: false,
    };
  }

  function ageLabel(ageSec) {
    if (typeof ageSec !== "number") return "";
    if (ageSec < 60) return `${ageSec}s`;
    if (ageSec < 3600) return `${Math.floor(ageSec / 60)}m`;
    return `${Math.floor(ageSec / 3600)}h`;
  }

  function metaOf(session) {
    const parts = [session.status_label || session.status];
    if (session.tool_name) parts.push(toolLabel(session.tool_name));
    if (typeof session.age_sec === "number") parts.push(`${session.age_sec}s 前`);
    return parts.join(" / ");
  }

  function agentSourceLabel(source) {
    if (source === "codex") return "Codex";
    if (source === "claude_code") return "Claude Code";
    return source || "Agent";
  }

  function agentSourceShort(source) {
    if (source === "codex") return "CX";
    if (source === "claude_code") return "CC";
    return "AI";
  }

  function agentSourceClass(source) {
    if (source === "codex") return "codex";
    if (source === "claude_code") return "claude";
    return "agent";
  }

  function render(snapshot) {
    latest = snapshot || latest;
    const sessions = latest?.sessions || [];
    const visibleCount = sessions.filter((session) => !session.display?.quiet).length;
    watchCount.textContent = String(sessions.length);
    if (watchTitle) {
      const waiting = sessions.filter((session) => session.needs_user || session.display?.tone === "needs_user").length;
      const active = sessions.filter((session) => ["active", "error"].includes(session.display?.tone)).length;
      watchTitle.textContent = waiting ? `${waiting} 个需要处理` : active ? `${active} 个进行中` : "Agent Watch";
    }
    updateExpandToggle(sessions);
    if (!sessions.length) {
      stack.innerHTML = `<div class="empty">暂无 Claude Code 任务</div>`;
      setFolded(false);
      return;
    }
    const visibleSessions = sortedSessions(sessions)
      .filter((session) => !session.display?.quiet)
      .slice(0, 3);
    stack.innerHTML = visibleSessions.map((session) => {
      const id = session.session_id;
      const isCollapsed = collapsed.has(id);
      const status = session.status || "idle";
      const view = viewOf(session);
      return `
        <article class="task-card ${escapeAttr(status)} tone-${escapeAttr(view.tone)} ${view.quiet ? "quiet" : ""} ${isCollapsed ? "collapsed" : ""}" data-id="${escapeAttr(id)}">
          <span class="task-rail" aria-hidden="true"></span>
          <div class="task-main">
            <div class="task-headline">
              <h2 class="task-title">${escapeHtml(view.target)}</h2>
              <span class="task-age">${escapeHtml(view.age)}</span>
            </div>
            <p class="task-summary">${escapeHtml(view.detail)}</p>
            <div class="task-meta">
              <span class="task-dot"></span>
              <span>${escapeHtml(view.project)}</span>
              <span>${escapeHtml(view.source)}</span>
              <span>${escapeHtml(view.kind)}</span>
            </div>
          </div>
          <div class="task-actions">
            <button class="task-open" type="button" data-action="open" title="打开工作目录">打开</button>
            <button class="task-toggle" type="button" data-action="toggle" title="${isCollapsed ? "展开" : "折叠"}" aria-label="${isCollapsed ? "展开" : "折叠"}">${isCollapsed ? "+" : "-"}</button>
          </div>
        </article>`;
    }).join("");
    if (visibleCount < sessions.length) {
      stack.innerHTML += `<div class="quiet-note">已收起 ${sessions.length - visibleCount} 个完成任务</div>`;
    }
  }

  async function refresh() {
    if (!invoke) return;
    try {
      render(await invoke("cmd_get_agent_sessions"));
    } catch (e) {
      console.error("[agent-watch] refresh failed", e);
    }
  }

  async function dismiss(id) {
    if (!invoke) return;
    collapsed.delete(id);
    saveCollapsed();
    try {
      render(await invoke("cmd_dismiss_agent_session", { sessionId: id }));
    } catch (e) {
      console.error("[agent-watch] dismiss failed", e);
    }
  }

  async function openWorkspace(id) {
    if (!invoke) return;
    try {
      await invoke("cmd_open_agent_workspace", { sessionId: id });
    } catch (e) {
      console.error("[agent-watch] open workspace failed", e);
    }
  }

  stack.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button) return;
    const card = button.closest(".task-card");
    const id = card?.dataset.id;
    if (!id) return;
    const action = button.dataset.action;
    if (action === "open") {
      openWorkspace(id);
    } else if (action === "toggle") {
      if (collapsed.has(id)) collapsed.delete(id);
      else collapsed.add(id);
      saveCollapsed();
      render(latest);
    }
  });

  function toggleFolded() {
    setFolded(!folded);
  }

  function unfold() {
    if (folded) setFolded(false);
  }

  function expandAllTasks() {
    collapsed.clear();
    saveCollapsed();
    render(latest);
  }

  function collapseAllTasks() {
    const sessions = latest?.sessions || [];
    for (const session of sessions) {
      if (session.session_id) collapsed.add(session.session_id);
    }
    saveCollapsed();
    render(latest);
  }

  function hasCollapsedTask(sessions = latest?.sessions || []) {
    return sessions.some((session) => session.session_id && collapsed.has(session.session_id));
  }

  function updateExpandToggle(sessions = latest?.sessions || []) {
    if (!watchExpandToggle) return;
    const shouldExpand = hasCollapsedTask(sessions);
    watchExpandToggle.textContent = shouldExpand ? "+" : "-";
    watchExpandToggle.title = shouldExpand ? "Expand all" : "Collapse all";
    watchExpandToggle.setAttribute("aria-label", watchExpandToggle.title);
  }

  function currentWindow() {
    try {
      return window.__TAURI__?.window?.getCurrentWindow?.();
    } catch (_) {
      return null;
    }
  }

  function setupWindowDrag() {
    if (!watchHeader) return;
    let pointerDown = null;

    watchHeader.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      if (event.target.closest("button")) return;
      pointerDown = { id: event.pointerId, x: event.clientX, y: event.clientY };
    });

    watchHeader.addEventListener("pointermove", async (event) => {
      if (!pointerDown || event.pointerId !== pointerDown.id) return;
      if (Math.hypot(event.clientX - pointerDown.x, event.clientY - pointerDown.y) < 5) return;
      pointerDown = null;
      suppressNextHeaderClick = true;
      const win = currentWindow();
      if (!win) return;
      try {
        await invoke?.("cmd_agent_watch_mark_user_placed");
      } catch (_) {}
      try {
        await win.startDragging();
      } catch (e) {
        console.error("[agent-watch] drag failed", e);
      }
    });

    for (const type of ["pointerup", "pointercancel", "pointerleave"]) {
      watchHeader.addEventListener(type, () => {
        pointerDown = null;
      });
    }
  }

  if (watchActions) {
    watchActions.addEventListener("click", (event) => {
      event.stopPropagation();
      const action = event.target.closest("[data-watch-action]")?.dataset.watchAction;
      if (action === "toggle-all") {
        if (hasCollapsedTask()) expandAllTasks();
        else collapseAllTasks();
      }
    });
  }

  watchHeader.addEventListener("click", () => {
    if (suppressNextHeaderClick) {
      suppressNextHeaderClick = false;
      return;
    }
    toggleFolded();
  });
  watchHeader.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    toggleFolded();
  });
  watchHeader.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    toggleFolded();
  });
  shell.addEventListener("dblclick", unfold);
  shell.addEventListener("contextmenu", (event) => {
    if (!folded) return;
    event.preventDefault();
    unfold();
  });

  window.__agentWatchRefresh = refresh;
  setFolded(folded, false);
  setupWindowDrag();
  refresh();
  if (listen) {
    listen("agent-watch-update", (event) => render(event.payload));
  }
  setInterval(refresh, 2000);
})();
