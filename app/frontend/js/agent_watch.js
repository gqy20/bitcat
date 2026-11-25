(function () {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  const AgentIcons = window.AgentIcons;
  const shell = document.getElementById("agent-watch");
  const watchHeader = document.getElementById("watch-header");
  const watchTitle = document.getElementById("watch-title");
  const stack = document.getElementById("stack");
  const watchCount = document.getElementById("watch-count");
  const tooltip = document.getElementById("watch-tooltip");
  const expanded = new Set(JSON.parse(localStorage.getItem("agentWatchExpanded") || "[]"));
  let folded = localStorage.getItem("agentWatchFolded") === "true";
  let latest = null;
  let suppressNextClick = false;
  // 拖拽协调：拖拽中不碰 DOM/窗口几何，松手一次性追平
  let dragging = false;
  let pendingRender = false;
  let pendingResize = false;
  // 只在卡片结构变化时 resize；年龄/状态文字更新不触发窗口几何变化。
  let lastResizeSignature = null;

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

  function showTooltip(element) {
    if (!tooltip || !element) return;
    const text = element.dataset.tooltip;
    if (!text) return;
    tooltip.textContent = text;
    tooltip.classList.add("visible");
    tooltip.setAttribute("aria-hidden", "false");
    const rect = element.getBoundingClientRect();
    const tipRect = tooltip.getBoundingClientRect();
    let left = Math.min(Math.max(8, rect.left), window.innerWidth - tipRect.width - 8);
    let top = rect.bottom + 6;
    if (top + tipRect.height > window.innerHeight - 8) top = rect.top - tipRect.height - 6;
    tooltip.style.left = `${Math.max(8, left)}px`;
    tooltip.style.top = `${Math.max(8, top)}px`;
  }

  function hideTooltip() {
    if (!tooltip) return;
    tooltip.classList.remove("visible");
    tooltip.setAttribute("aria-hidden", "true");
  }

  async function resizeWatch() {
    // 拖拽中不改窗口几何：记 pending，松手后 flush 一次
    if (dragging) {
      pendingResize = true;
      return;
    }
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

  function compactMachine(machine) {
    return String(machine || "").trim().slice(0, 6).toUpperCase();
  }

  function viewOf(session) {
    const display = session.display || {};
    const status = String(session.status || "").toLowerCase();
    const project = display.project || session.workspace_name || "未知项目";
    const headline = display.headline || session.status_label || statusLabel(status);
    const tokens = Number(session.tokens_in ?? 0) + Number(session.tokens_out ?? 0);
    const usageTooltip = tokens
      ? `本轮用量 ${tokens.toLocaleString()} tokens`
      : "本轮用量";
    const machine = session.machine || "";
    return {
      title: project,
      fullPath: session.workspace || session.session_id || project,
      headline,
      kind: display.action_label || "Task",
      detail: display.detail || "",
      actionPreview: display.detail || session.tool_input_preview || session.last_response_preview || "",
      project,
      source: display.source_label || window.AgentSources.label(session.source),
      sourceBadge: window.AgentSources.badge(session.source),
      machine,
      deviceBadge: compactMachine(machine),
      age: display.age_label || ageLabel(session.age_sec),
      tone: display.tone || session.status || "idle",
      iconName: AgentIcons?.iconForSession(session) || "activity",
      quiet: Boolean(display.quiet),
      usage: String(display.usage_label || ""),
      usageTooltip,
      subtaskLabel: String(display.subtask_label || ""),
      statusText: headline,
    };
  }

  function statusLabel(status) {
    if (status === "waiting") return "等你处理";
    if (status === "error") return "执行失败";
    if (status === "compacting") return "整理上下文";
    if (status === "done") return "完成";
    return "";
  }

  function isDoneStatus(status) {
    return String(status || "").toLowerCase() === "done";
  }

  function shouldHideSession(session, allSessions) {
    if (session.display?.quiet) return true;
    if (String(session.status || "").toLowerCase() === "idle") return true;
    // background 子会话聚合进主卡片，不独立占位；父会话不在列表时兜底独立展示。
    if (session.background && session.parent_session_id && allSessions) {
      const parentKnown = allSessions.some(
        (candidate) => candidate.session_id === session.parent_session_id
      );
      if (parentKnown) return true;
    }
    return false;
  }

  function ageLabel(ageSec) {
    if (typeof ageSec !== "number") return "";
    if (ageSec < 60) return `${ageSec}s`;
    if (ageSec < 3600) return `${Math.floor(ageSec / 60)}m`;
    return `${Math.floor(ageSec / 3600)}h`;
  }

  function renderContext(view) {
    const machine = view.machine
      ? `<span class="task-device" data-tooltip="设备 ${escapeAttr(view.machine)}">${escapeHtml(view.machine)}</span>`
      : "";
    const source = view.source
      ? `<span class="task-source" data-tooltip="${escapeAttr(view.source)}">${escapeHtml(view.source)}</span>`
      : "";
    const usage = view.usage
      ? `<span class="task-usage" data-tooltip="${escapeAttr(view.usageTooltip)}">${escapeHtml(view.usage)}</span>`
      : "";
    if (!machine && !source && !usage) return "";
    return `${source}${machine}${usage}`;
  }

  function detailText(view, session) {
    const lines = [];
    pushUniqueLine(lines, view.headline, view.title);
    pushUniqueLine(lines, view.actionPreview, view.headline);
    if (!lines.length && isDoneStatus(session?.status)) lines.push("任务完成");
    return lines.filter(Boolean).join("\n");
  }

  function pushUniqueLine(lines, value, compareTo) {
    const text = String(value || "").trim();
    if (!text) return;
    const compare = String(compareTo || "").trim();
    if (compare && text === compare) return;
    if (lines.includes(text)) return;
    lines.push(text);
  }

  function summaryState(sessions) {
    const visible = sessions.filter((session) => !shouldHideSession(session, sessions));
    const count = (statuses) => visible.filter((session) => statuses.includes(String(session.status || "").toLowerCase())).length;
    const errors = count(["error"]);
    const waiting = count(["waiting"]);
    const active = count(["working", "tool_running", "compacting"]);
    const done = count(["done"]);
    if (errors) return { symbol: "×", count: errors, state: "error", label: `${errors} 个任务失败` };
    if (waiting) return { symbol: "!", count: waiting, state: "waiting", label: `${waiting} 个任务待处理` };
    if (active) return { symbol: "▶", count: active, state: "active", label: `${active} 个任务运行中` };
    if (done) return { symbol: "✓", count: done, state: "success", label: `${done} 个任务完成` };
    return { symbol: "–", count: 0, state: "muted", label: "暂无任务" };
  }

  function subtaskRows(session, allSessions) {
    return allSessions
      .filter(
        (candidate) =>
          candidate.background &&
          candidate.parent_session_id === session.session_id &&
          !candidate.display?.quiet &&
          String(candidate.status || "").toLowerCase() !== "idle"
      )
      .map((candidate) => {
        const display = candidate.display || {};
        const status = String(candidate.status || "").toLowerCase();
        const cls =
          status === "waiting" || status === "error"
            ? " subtask-needs"
            : status === "done"
              ? " subtask-done"
              : "";
        const label = display.action_label || display.headline || statusLabel(status);
        return `<li class="task-subtask${cls}" data-tooltip="${escapeAttr(display.detail || label)}">
          <span class="subtask-name">${escapeHtml(label)}</span>
          <span class="subtask-age">${escapeHtml(display.age_label || ageLabel(candidate.age_sec))}</span>
        </li>`;
      });
  }

  function cardMarkup(isExpanded, hasSubtasks) {
    return `
      <div class="task-main">
        <div class="task-identity">
          <strong class="task-title"></strong>
          <span class="task-badges">
            <span class="task-source-badge"></span>
            <span class="task-device-badge"></span>
            <span class="task-subtask-count"></span>
          </span>
          <span class="task-age"></span>
        </div>
        <div class="task-activity">
          <span class="task-icon" aria-hidden="true"></span>
          <span class="task-status-text"></span>
          <span class="task-action-preview"></span>
        </div>
        ${isExpanded ? `
          <div class="task-context"></div>
          <p class="task-detail"></p>
          ${hasSubtasks ? `<ul class="task-subtasks"></ul>` : ""}
          <div class="task-actions"><button class="task-action" type="button" data-action="open-workspace" title="打开这个任务的文件夹" aria-label="打开这个任务的文件夹">打开目录</button></div>
        ` : ""}
      </div>
      <button class="task-dismiss" type="button" data-action="dismiss" title="隐藏这条任务" aria-label="隐藏这条任务">×</button>`;
  }

  function updateCard(card, session, view, isExpanded, allSessions) {
    const status = session.status || "idle";
    const oldStatus = card.dataset.status || "";
    const oldIcon = card.dataset.icon || "";
    const oldExpanded = card.dataset.expanded === "true";
    const focusedAction = document.activeElement?.closest?.(".task-card") === card
      ? document.activeElement.dataset.action
      : "";
    const subtasks = isExpanded ? subtaskRows(session, allSessions) : [];
    const markup = cardMarkup(isExpanded, subtasks.length > 0);
    card.className = `task-card ${status} tone-${view.tone} ${view.quiet ? "quiet" : ""} ${isExpanded ? "expanded" : ""}`;
    card.dataset.id = session.session_id;
    card.dataset.status = status;
    card.dataset.icon = view.iconName;
    card.dataset.expanded = String(isExpanded);
    card.tabIndex = 0;
    card.setAttribute("role", "button");
    card.setAttribute("aria-expanded", String(isExpanded));
    card.setAttribute("aria-label", detailText(view, session) || view.title);
    if (card.__markup !== markup) {
      card.innerHTML = markup;
      card.__markup = markup;
    }

    const title = card.querySelector(".task-title");
    AgentIcons?.setText(title, view.title);
    title.dataset.tooltip = view.fullPath;
    title.title = view.fullPath;

    const sourceBadge = card.querySelector(".task-source-badge");
    AgentIcons?.setText(sourceBadge, view.sourceBadge);
    sourceBadge.dataset.tooltip = view.source;
    sourceBadge.title = view.source;

    const deviceBadge = card.querySelector(".task-device-badge");
    AgentIcons?.setText(deviceBadge, view.deviceBadge);
    deviceBadge.hidden = !view.deviceBadge;
    deviceBadge.dataset.tooltip = view.machine ? `设备 ${view.machine}` : "";
    deviceBadge.title = view.machine ? `设备 ${view.machine}` : "";

    const subtaskCount = card.querySelector(".task-subtask-count");
    AgentIcons?.setText(subtaskCount, view.subtaskLabel);
    subtaskCount.hidden = !view.subtaskLabel;
    subtaskCount.dataset.tooltip = view.subtaskLabel;

    const age = card.querySelector(".task-age");
    AgentIcons?.setText(age, view.age);
    age.hidden = !view.age;

    const icon = card.querySelector(".task-icon");
    if (icon) {
      icon.innerHTML = AgentIcons?.render(view.iconName) || "";
      if (oldIcon !== view.iconName && AgentIcons?.stateForStatus(status) === "active") {
        AgentIcons.pulse(icon.firstElementChild);
      }
    }

    const statusText = card.querySelector(".task-status-text");
    AgentIcons?.setText(statusText, view.statusText);
    if (oldStatus !== status) AgentIcons?.pulse(statusText);

    const actionPreview = card.querySelector(".task-action-preview");
    AgentIcons?.setText(actionPreview, view.actionPreview);
    actionPreview.hidden = !view.actionPreview;
    actionPreview.dataset.tooltip = view.actionPreview;
    actionPreview.title = view.actionPreview;

    if (isExpanded) {
      const context = card.querySelector(".task-context");
      const contextHtml = renderContext(view);
      if (context.__html !== contextHtml) {
        context.innerHTML = contextHtml;
        context.__html = contextHtml;
      }
      const detail = card.querySelector(".task-detail");
      AgentIcons?.setText(detail, detailText(view, session));
      detail.hidden = !detail.textContent;
      const list = card.querySelector(".task-subtasks");
      if (list) {
        const subtaskHtml = subtasks.join("");
        if (list.__html !== subtaskHtml) {
          list.innerHTML = subtaskHtml;
          list.__html = subtaskHtml;
        }
      }
    }
    if (focusedAction) card.querySelector(`[data-action="${focusedAction}"]`)?.focus();
    if (oldExpanded !== isExpanded) hideTooltip();
  }

  function render(snapshot) {
    latest = snapshot || latest;
    if (dragging) {
      pendingRender = true;
      return;
    }
    const sessions = latest?.sessions || [];
    const renderableSessions = sortedSessions(sessions).filter(
      (session) => !shouldHideSession(session, sessions)
    );
    if (watchTitle) watchTitle.textContent = "Agent Watch";
    if (watchCount) {
      const summary = summaryState(sessions);
      watchCount.textContent = `${summary.symbol} ${summary.count}`;
      watchCount.dataset.state = summary.state;
      watchCount.title = summary.label;
      watchCount.setAttribute("aria-label", summary.label);
    }

    if (!renderableSessions.length) {
      if (!stack.querySelector(".empty") || stack.children.length !== 1) {
        stack.replaceChildren();
        const empty = document.createElement("div");
        empty.className = "empty";
        empty.textContent = "暂无 Agent 任务";
        stack.append(empty);
        lastResizeSignature = "empty";
        resizeWatch();
      }
      setFolded(false);
      return;
    }

    stack.querySelector(".empty")?.remove();
    const existing = new Map(
      [...stack.querySelectorAll(".task-card")].map((card) => [card.dataset.id, card])
    );
    const ordered = [];
    for (const session of renderableSessions) {
      let card = existing.get(session.session_id);
      if (!card) {
        card = document.createElement("article");
        stack.append(card);
      }
      updateCard(card, session, viewOf(session), expanded.has(session.session_id), sessions);
      existing.delete(session.session_id);
      ordered.push(card);
    }
    for (const card of existing.values()) card.remove();
    ordered.forEach((card, index) => {
      if (stack.children[index] !== card) stack.insertBefore(card, stack.children[index] || null);
    });

    const resizeSignature = ordered
      .map((card) => `${card.dataset.id}:${card.dataset.expanded}:${card.querySelectorAll(".task-subtask").length}:${card.dataset.status}`)
      .join("|");
    if (resizeSignature !== lastResizeSignature) {
      lastResizeSignature = resizeSignature;
      resizeWatch();
    }
  }

  // 拖拽结束追平：一次 render + 一次 resize，避免拖拽期间逐帧改 DOM/几何
  function flushAfterDrag() {
    const needRender = pendingRender;
    pendingRender = false;
    if (needRender) {
      render(latest);
      pendingResize = false;
    }
    if (pendingResize) {
      pendingResize = false;
      resizeWatch();
    }
  }

  async function refresh() {
    if (!invoke) return;
    // 窗口隐藏时跳过轮询：setInterval 不受 visibility 节流，
    // 隐藏期间的每次 invoke 都是纯浪费（显示时会由 eval 推送刷新）。
    if (typeof document !== "undefined" && document.hidden) return;
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

  async function openWorkspace(id) {
    if (!invoke || !id) return;
    try {
      await invoke("cmd_open_agent_workspace", { sessionId: id });
    } catch (e) {
      log("open workspace failed", e);
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

  stack.addEventListener("pointerover", (event) => {
    const target = event.target.closest("[data-tooltip]");
    if (target) showTooltip(target);
  });
  stack.addEventListener("pointerout", (event) => {
    const target = event.target.closest("[data-tooltip]");
    if (target && !target.contains(event.relatedTarget)) hideTooltip();
  });
  stack.addEventListener("focusin", (event) => {
    const target = event.target.closest("[data-tooltip]");
    if (target) showTooltip(target);
  });
  stack.addEventListener("focusout", (event) => {
    const target = event.target.closest("[data-tooltip]");
    if (target && !target.contains(event.relatedTarget)) hideTooltip();
  });
  window.addEventListener("scroll", hideTooltip, true);
  window.addEventListener("resize", hideTooltip);

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
      else if (button.dataset.action === "open-workspace") openWorkspace(id);
      return;
    }
    toggleTaskDetail(id);
  });

  // ---- 手柄导航：方向键移动焦点卡，A 展开，B 收起 ----
  function focusableCards() {
    return [...stack.querySelectorAll(".task-card")];
  }

  function focusCardAt(index) {
    const cards = focusableCards();
    if (!cards.length) return;
    cards.forEach((card) => card.classList.remove("gamepad-focus"));
    const target = cards[Math.max(0, Math.min(cards.length - 1, index))];
    target.classList.add("gamepad-focus");
    target.scrollIntoView({ block: "nearest" });
  }

  function focusIndex() {
    const cards = focusableCards();
    const current = cards.findIndex((card) => card.classList.contains("gamepad-focus"));
    return current >= 0 ? current : 0;
  }

  function setupGamepadNavigation() {
    if (!listen) return;
    listen("agent-watch-nav", (event) => {
      const [, dy] = event.payload || [];
      if (dy === 1) focusCardAt(focusIndex() + 1);
      else if (dy === -1) focusCardAt(focusIndex() - 1);
      else focusCardAt(0);
    });
    listen("agent-watch-confirm", () => {
      const cards = focusableCards();
      const card = cards[focusIndex()] || cards[0];
      if (card?.dataset.id) toggleTaskDetail(card.dataset.id);
    });
    listen("agent-watch-back", () => {
      const cards = focusableCards();
      const card = cards[focusIndex()] || cards[0];
      if (card?.dataset.id) toggleTaskDetail(card.dataset.id);
    });
  }

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
      // 上报用户摆放 fire-and-forget：不阻塞起手，避免 follower 抢位造成抓取瞬间跳一下
      try {
        invoke?.("cmd_agent_watch_mark_user_placed")?.catch?.(() => {});
      } catch (_) {}
      dragging = true;
      document.body.classList.add("dragging");
      try {
        // promise 在 OS 拖拽结束时才 resolve，用作拖拽态的天然边界
        await win.startDragging();
      } catch (e) {
        log("drag failed", e);
      }
      dragging = false;
      document.body.classList.remove("dragging");
      flushAfterDrag();
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
    agentSourceLabel: source => window.AgentSources.label(source),
    render,
    viewOf,
    detailText,
    summaryState,
    showTooltip,
    hideTooltip,
  };
  setFolded(folded, false);
  resizeWatch();
  setupWindowDrag();
  setupGamepadNavigation();
  refresh();
  if (listen) {
    listen("agent-watch-update", (event) => render(event.payload));
  }
  setInterval(refresh, 2000);
})();
