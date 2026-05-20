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

  function log(msg, error) {
    const text = error ? `${msg}: ${error.message || error}` : msg;
    if (invoke) invoke("cmd_agent_watch_log", { msg: text }).catch(() => {});
    console.warn("[agent-watch]", text);
  }

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

  async function syncFoldedWindow() {
    if (!invoke) return;
    try {
      await invoke("cmd_agent_watch_set_folded", { folded });
    } catch (e) {
      log("resize failed", e);
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

  function attentionRank(session) {
    const status = String(session.status || "").toLowerCase();
    if (status === "done") return 0;
    if (status === "waiting" || status === "error") return 1;
    if (status === "working" || status === "tool_running" || status === "compacting") return 2;
    return 3;
  }

  function sortedSessions(sessions) {
    return [...sessions].sort((left, right) => {
      const byAttention = attentionRank(left) - attentionRank(right);
      if (byAttention) return byAttention;
      return Number(left.age_sec ?? 0) - Number(right.age_sec ?? 0);
    });
  }

  function viewOf(session) {
    const display = session.display || {};
    return {
      kind: display.action_label || "Task",
      target: display.headline || session.status_label || "任务更新",
      detail: display.detail || display.project || session.workspace_name || "",
      project: display.project || session.workspace_name || "",
      source: display.source_label || agentSourceLabel(session.source),
      machine: session.machine || "",
      age: display.age_label || ageLabel(session.age_sec),
      tone: display.tone || session.status || "idle",
      quiet: Boolean(display.quiet),
    };
  }

  function lineParts(view, session, isCollapsed) {
    const parts = [];
    if (view.machine) parts.push({ value: view.machine, className: "task-device" });
    if (view.project) parts.push({ value: view.project, className: "task-project" });
    if (view.source) parts.push({ value: compactSourceLabel(view.source), className: "task-source" });
    if (!isNoisyActionLabel(view.kind, session)) {
      parts.push({ value: view.kind, className: "task-kind" });
    }
    if (!isCollapsed && view.age) {
      parts.push({ value: view.age, className: "task-age" });
    }
    return parts;
  }

  function isNoisyActionLabel(label, session) {
    const value = String(label || "").trim();
    if (!value) return true;
    if (value === "Task") return !(session?.background || session?.task_id);
    return value.toLowerCase().includes("mcp");
  }

  function shouldHideSession(session) {
    if (session.display?.quiet) return true;
    return String(session.status || "").toLowerCase() === "idle";
  }

  function ageLabel(ageSec) {
    if (typeof ageSec !== "number") return "";
    if (ageSec < 60) return `${ageSec}s`;
    if (ageSec < 3600) return `${Math.floor(ageSec / 60)}m`;
    return `${Math.floor(ageSec / 3600)}h`;
  }

  function agentSourceLabel(source) {
    if (source === "codex") return "Codex";
    if (source === "claude_code") return "Claude Code";
    return source || "Agent";
  }

  function renderMetaItem(item) {
    const cls = item.className ? ` ${item.className}` : "";
    return `<span class="task-meta-item${cls}" title="${escapeAttr(item.value)}">${escapeHtml(item.value)}</span>`;
  }

  function compactSourceLabel(source) {
    if (source === "Claude Code") return "Claude";
    return source;
  }

  function render(snapshot) {
    latest = snapshot || latest;
    const sessions = latest?.sessions || [];
    const visibleCount = sessions.filter((session) => !shouldHideSession(session)).length;
    watchCount.textContent = String(sessions.length);
    if (watchTitle) {
      const count = visibleCount || sessions.length;
      watchTitle.textContent = count ? `Agent 看管 ${count}` : "Agent 看管";
    }
    updateExpandToggle(sessions);
    if (!sessions.length) {
      stack.innerHTML = `<div class="empty">暂无 Claude Code 任务</div>`;
      setFolded(false);
      return;
    }
    const renderableSessions = sortedSessions(sessions)
      .filter((session) => !shouldHideSession(session));
    stack.innerHTML = renderableSessions.map((session) => {
      const id = session.session_id;
      const isCollapsed = collapsed.has(id);
      const status = session.status || "idle";
      const view = viewOf(session);
      const items = lineParts(view, session, isCollapsed);
      return `
        <article class="task-card ${escapeAttr(status)} tone-${escapeAttr(view.tone)} ${view.quiet ? "quiet" : ""} ${isCollapsed ? "collapsed" : ""}" data-id="${escapeAttr(id)}">
          <span class="task-rail" aria-hidden="true"></span>
          <div class="task-main">
            <div class="task-meta">
              <span class="task-dot"></span>
              ${items.map(renderMetaItem).join("")}
            </div>
          </div>
          <div class="task-actions">
            <button class="task-toggle" type="button" data-action="toggle" title="${isCollapsed ? "展开" : "折叠"}" aria-label="${isCollapsed ? "展开" : "折叠"}">${isCollapsed ? "+" : "-"}</button>
          </div>
        </article>`;
    }).join("");
    const quietCount = sessions.length - visibleCount;
    if (quietCount > 0) {
      stack.innerHTML += `<div class="quiet-note">已收起 ${quietCount} 个低优先级任务</div>`;
    }
  }

  async function refresh() {
    if (!invoke) return;
    try {
      render(await invoke("cmd_get_agent_sessions"));
    } catch (e) {
      log("refresh failed", e);
    }
  }

  async function dismiss(id) {
    if (!invoke) return;
    collapsed.delete(id);
    saveCollapsed();
    try {
      render(await invoke("cmd_dismiss_agent_session", { sessionId: id }));
    } catch (e) {
      log("dismiss failed", e);
    }
  }

  stack.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button) return;
    const card = button.closest(".task-card");
    const id = card?.dataset.id;
    if (!id) return;
    const action = button.dataset.action;
    if (action === "toggle") {
      if (collapsed.has(id)) collapsed.delete(id);
      else collapsed.add(id);
      saveCollapsed();
      render(latest);
    }
  });

  function toggleFolded() {
    invoke?.("cmd_agent_watch_mark_user_placed").catch(() => {});
    setFolded(!folded);
  }

  function unfold() {
    if (folded) {
      invoke?.("cmd_agent_watch_mark_user_placed").catch(() => {});
      setFolded(false);
    }
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
        log("drag failed", e);
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
  window.__agentWatchTest = {
    agentSourceLabel,
    renderMetaItem,
    lineParts,
    render,
    viewOf,
  };
  setFolded(folded, false);
  setupWindowDrag();
  refresh();
  if (listen) {
    listen("agent-watch-update", (event) => render(event.payload));
  }
  setInterval(refresh, 2000);
})();
