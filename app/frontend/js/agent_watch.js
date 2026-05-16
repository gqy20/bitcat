(function () {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  const shell = document.getElementById("agent-watch");
  const stack = document.getElementById("stack");
  const foldAll = document.getElementById("fold-all");
  const collapsed = new Set(JSON.parse(localStorage.getItem("agentWatchCollapsed") || "[]"));
  let folded = localStorage.getItem("agentWatchFolded") === "true";
  let latest = null;

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

  function setFolded(next) {
    folded = next;
    localStorage.setItem("agentWatchFolded", String(folded));
    shell.classList.toggle("folded", folded);
    foldAll.title = folded ? "展开任务栈" : "折叠任务栈";
    foldAll.setAttribute("aria-label", foldAll.title);
  }

  function summaryOf(session) {
    return session.user_prompt_preview
      || session.tool_input_preview
      || session.last_response_preview
      || session.workspace
      || session.session_id;
  }

  function metaOf(session) {
    const parts = [session.status_label || session.status];
    if (session.tool_name) parts.push(session.tool_name);
    if (typeof session.age_sec === "number") parts.push(`${session.age_sec}s 前`);
    return parts.join(" · ");
  }

  function render(snapshot) {
    latest = snapshot || latest;
    const sessions = latest?.sessions || [];
    if (!sessions.length) {
      stack.innerHTML = `<div class="empty">暂无 Claude Code 任务</div>`;
      setFolded(false);
      return;
    }
    stack.innerHTML = sessions.slice(0, 5).map((session) => {
      const id = session.session_id;
      const isCollapsed = collapsed.has(id);
      const status = session.status || "idle";
      return `
        <article class="task-card ${escapeAttr(status)} ${isCollapsed ? "collapsed" : ""}" data-id="${escapeAttr(id)}">
          <button class="task-close" type="button" data-action="dismiss" title="从列表移除" aria-label="从列表移除">×</button>
          <div class="task-main">
            <h2 class="task-title">${escapeHtml(session.workspace_name || "Claude Code")}</h2>
            <p class="task-summary">${escapeHtml(summaryOf(session))}</p>
            <div class="task-meta">
              <span class="task-dot"></span>
              <span>${escapeHtml(metaOf(session))}</span>
            </div>
          </div>
          <div class="task-actions">
            <button class="task-open" type="button" data-action="open" title="打开工作目录">打开</button>
            <button class="task-toggle" type="button" data-action="toggle" title="${isCollapsed ? "展开" : "折叠"}" aria-label="${isCollapsed ? "展开" : "折叠"}">${isCollapsed ? "›" : "⌄"}</button>
          </div>
        </article>`;
    }).join("");
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
    if (action === "dismiss") {
      dismiss(id);
    } else if (action === "open") {
      openWorkspace(id);
    } else if (action === "toggle") {
      if (collapsed.has(id)) collapsed.delete(id);
      else collapsed.add(id);
      saveCollapsed();
      render(latest);
    }
  });

  foldAll.addEventListener("click", () => setFolded(!folded));

  window.__agentWatchRefresh = refresh;
  setFolded(folded);
  refresh();
  if (listen) {
    listen("agent-watch-update", (event) => render(event.payload));
  }
  setInterval(refresh, 2000);
})();
