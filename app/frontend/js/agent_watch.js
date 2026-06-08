(function () {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  const shell = document.getElementById("agent-watch");
  const watchHeader = document.getElementById("watch-header");
  const watchTitle = document.getElementById("watch-title");
  const stack = document.getElementById("stack");
  const watchCount = document.getElementById("watch-count");
  const expanded = new Set(JSON.parse(localStorage.getItem("agentWatchExpanded") || "[]"));
  let folded = localStorage.getItem("agentWatchFolded") === "true";
  let latest = null;
  let suppressNextClick = false;

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

  async function resizeWatch() {
    if (!invoke) return;
    try {
      if (folded) {
        await invoke("cmd_agent_watch_set_folded", { folded: true });
        return;
      }
      const height = measuredWatchHeight();
      await invoke("cmd_agent_watch_resize", { height });
    } catch (e) {
      log("resize failed", e);
    }
  }

  function cssPixels(styles, name) {
    const value = Number.parseFloat(styles.getPropertyValue(name));
    return Number.isFinite(value) ? value : 0;
  }

  function measuredWatchHeight() {
    const shellStyles = window.getComputedStyle(shell);
    const stackStyles = window.getComputedStyle(stack);
    const shellPadding =
      cssPixels(shellStyles, "padding-top") + cssPixels(shellStyles, "padding-bottom");
    const stackVisible = stackStyles.display !== "none";
    const stackHeight = stackVisible
      ? cssPixels(stackStyles, "margin-top") + stack.scrollHeight
      : 0;
    return Math.ceil(shellPadding + watchHeader.offsetHeight + stackHeight);
  }

  function setFolded(next, syncWindow = true) {
    folded = Boolean(next);
    localStorage.setItem("agentWatchFolded", String(folded));
    shell.classList.toggle("folded", folded);
    watchHeader.title = folded ? "展开 Agent Watch" : "隐藏任务列表";
    watchHeader.setAttribute("aria-label", watchHeader.title);
    watchHeader.setAttribute("aria-expanded", String(!folded));
    if (syncWindow) resizeWatch();
  }

  function saveExpanded() {
    localStorage.setItem("agentWatchExpanded", JSON.stringify([...expanded]));
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

  function viewOf(session) {
    const display = session.display || {};
    const status = String(session.status || "").toLowerCase();
    const project = display.project || session.workspace_name || "";
    const headline = display.headline || session.status_label || statusLabel(status);
    return {
      title: project || headline || "Agent task",
      headline,
      kind: display.action_label || "Task",
      detail: display.detail || "",
      project,
      source: display.source_label || agentSourceLabel(session.source),
      machine: session.machine || "",
      age: display.age_label || ageLabel(session.age_sec),
      tone: display.tone || session.status || "idle",
      quiet: Boolean(display.quiet),
      statusText: statusLine(session, display),
    };
  }

  function statusLine(session, display) {
    const status = String(session.status || "").toLowerCase();
    if (display.detail) return display.detail;
    if (display.headline && !isDoneStatus(status)) return display.headline;
    return statusLabel(status);
  }

  function statusLabel(status) {
    if (status === "waiting") return "等待你处理";
    if (status === "error") return "需要检查";
    if (status === "working") return "正在处理";
    if (status === "tool_running") return "工具运行中";
    if (status === "compacting") return "正在压缩上下文";
    if (status === "done") return "已完成";
    return "任务更新";
  }

  function isDoneStatus(status) {
    return String(status || "").toLowerCase() === "done";
  }

  function lineParts(view, session, expandedRow = false) {
    if (expandedRow) return [];
    const parts = [];
    if (!isNoisyActionLabel(view.kind, session)) {
      parts.push({ value: view.kind, className: "task-kind" });
    }
    if (!expandedRow && shouldShowStatusText(view, session)) {
      parts.push({ value: view.statusText, className: "task-status-text" });
    }
    return parts;
  }

  function shouldShowStatusText(view, session) {
    if (!view.statusText || isDoneStatus(session?.status)) return false;
    const value = normalizeLine(view.statusText);
    if (!value) return false;
    const duplicates = [
      view.title,
      view.project,
      view.headline,
      session?.workspace_name,
      session?.display?.project,
    ].map(normalizeLine).filter(Boolean);
    return !duplicates.includes(value);
  }

  function normalizeLine(value) {
    return String(value || "").trim().toLowerCase();
  }

  function deviceHue(machine) {
    const value = String(machine || "").trim();
    let hash = 0;
    for (let i = 0; i < value.length; i += 1) {
      hash = ((hash << 5) - hash + value.charCodeAt(i)) | 0;
    }
    return 190 + (Math.abs(hash) % 135);
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

  function compactSourceLabel(source) {
    if (source === "Claude Code") return "Claude";
    if (source === "Codex") return "Codex";
    return source;
  }

  function renderMetaItem(item) {
    const cls = item.className ? ` ${item.className}` : "";
    return `<span class="task-meta-item${cls}" title="${escapeAttr(item.value)}">${escapeHtml(item.value)}</span>`;
  }

  function renderTopline(view) {
    const machine = view.machine
      ? `<span class="task-device" title="${escapeAttr(view.machine)}">${escapeHtml(view.machine)}</span>`
      : "";
    const source = view.source
      ? `<span class="task-source" title="${escapeAttr(view.source)}">${escapeHtml(compactSourceLabel(view.source))}</span>`
      : "";
    return `
      <span class="task-dot" aria-hidden="true"></span>
      <strong class="task-title" title="${escapeAttr(view.title)}">${escapeHtml(view.title)}</strong>
      ${machine}
      ${source}
      ${view.age ? `<span class="task-age">${escapeHtml(view.age)}</span>` : ""}`;
  }

  function detailText(view, session) {
    const status = String(session.status || "").toLowerCase();
    const lines = [];
    if (status === "waiting" || status === "error") lines.push(statusLabel(status));
    pushUniqueLine(lines, view.headline, view.title);
    pushUniqueLine(lines, view.detail, view.headline);
    if (!lines.length && isDoneStatus(status)) lines.push("任务已完成");
    return lines.filter(Boolean).join("\n");
  }

  function pushUniqueLine(lines, value, compareTo) {
    const text = String(value || "").trim();
    if (!text) return;
    const compare = String(compareTo || "").trim();
    if (compare && redundantDetailLine(text, compare)) return;
    if (lines.some((line) => redundantDetailLine(text, line))) return;
    lines.push(text);
  }

  function redundantDetailLine(value, existing) {
    if (value === existing) return true;
    return existing.endsWith(value) && /^(正在|将|已|需要|等待|工具|任务)/.test(existing);
  }

  function summaryText(sessions) {
    const visible = sessions.filter((session) => !shouldHideSession(session));
    const waiting = visible.filter((session) => {
      const status = String(session.status || "").toLowerCase();
      return status === "waiting" || status === "error";
    }).length;
    const active = visible.filter((session) => {
      const status = String(session.status || "").toLowerCase();
      return status === "working" || status === "tool_running" || status === "compacting";
    }).length;
    if (waiting) return `${waiting} 需要处理`;
    if (active) return `${active} 运行中`;
    return `${visible.length} 条记录`;
  }

  function render(snapshot) {
    latest = snapshot || latest;
    const sessions = latest?.sessions || [];
    const renderableSessions = sortedSessions(sessions).filter((session) => !shouldHideSession(session));
    watchCount.textContent = String(sessions.length);
    if (watchTitle) watchTitle.textContent = renderableSessions.length ? `Agent Watch ${renderableSessions.length}` : "Agent Watch";
    const summary = summaryText(sessions);
    if (watchCount) watchCount.textContent = summary;
    if (!sessions.length || !renderableSessions.length) {
      stack.innerHTML = `<div class="empty">暂无 Agent 任务</div>`;
      setFolded(false);
      return;
    }
    stack.innerHTML = renderableSessions.map((session) => {
      const id = session.session_id;
      const status = session.status || "idle";
      const view = viewOf(session);
      const isExpanded = expanded.has(id);
      const expandedClass = isExpanded ? "expanded" : "";
      const detail = detailText(view, session);
      const items = lineParts(view, session, isExpanded);
      const meta = items.length
        ? `<div class="task-meta">${items.map(renderMetaItem).join("")}</div>`
        : "";
      const compactClass = items.length ? "" : " compact";
      const style = view.machine ? ` style="--device-hue: ${deviceHue(view.machine)}"` : "";
      return `
        <article class="task-card ${escapeAttr(status)} tone-${escapeAttr(view.tone)} ${view.quiet ? "quiet" : ""} ${expandedClass}${compactClass}"${style} data-id="${escapeAttr(id)}" data-status="${escapeAttr(status)}" tabindex="0" role="button" aria-expanded="${isExpanded ? "true" : "false"}" aria-label="${escapeAttr(detail || view.title)}">
          <div class="task-main">
            <div class="task-topline">
              ${renderTopline(view)}
            </div>
            ${meta}
            ${isExpanded && detail ? `<p class="task-detail">${escapeHtml(detail)}</p>` : ""}
          </div>
          <button class="task-dismiss" type="button" data-action="dismiss" title="隐藏这条任务" aria-label="隐藏这条任务">×</button>
        </article>`;
    }).join("");
    resizeWatch();
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
    try {
      expanded.delete(id);
      saveExpanded();
      render(await invoke("cmd_dismiss_agent_session", { sessionId: id }));
    } catch (e) {
      log("dismiss failed", e);
    }
  }

  function toggleFolded() {
    invoke?.("cmd_agent_watch_mark_user_placed").catch(() => {});
    setFolded(!folded);
  }

  function toggleTaskDetail(id) {
    if (!id) return;
    if (expanded.has(id)) expanded.delete(id);
    else expanded.add(id);
    saveExpanded();
    render(latest);
  }

  stack.addEventListener("click", (event) => {
    if (suppressNextClick) {
      suppressNextClick = false;
      return;
    }
    const button = event.target.closest("button[data-action]");
    const card = event.target.closest(".task-card");
    const id = card?.dataset.id;
    if (button && id) {
      event.stopPropagation();
      if (button.dataset.action === "dismiss") dismiss(id);
      return;
    }
    toggleTaskDetail(id);
  });

  stack.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    const card = event.target.closest(".task-card");
    if (!card) return;
    event.preventDefault();
    toggleTaskDetail(card.dataset.id);
  });

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
      pointerDown = { id: event.pointerId, x: event.clientX, y: event.clientY };
    });

    watchHeader.addEventListener("pointermove", async (event) => {
      if (!pointerDown || event.pointerId !== pointerDown.id) return;
      if (Math.hypot(event.clientX - pointerDown.x, event.clientY - pointerDown.y) < 5) return;
      pointerDown = null;
      suppressNextClick = true;
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

  watchHeader?.addEventListener("click", () => {
    if (suppressNextClick) {
      suppressNextClick = false;
      return;
    }
    toggleFolded();
  });
  watchHeader?.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    toggleFolded();
  });

  window.__agentWatchRefresh = refresh;
  window.__agentWatchTest = {
    agentSourceLabel,
    renderMetaItem,
    lineParts,
    deviceHue,
    render,
    viewOf,
    detailText,
    summaryText,
  };
  setFolded(folded, false);
  resizeWatch();
  setupWindowDrag();
  refresh();
  if (listen) {
    listen("agent-watch-update", (event) => render(event.payload));
  }
  setInterval(refresh, 2000);
})();
