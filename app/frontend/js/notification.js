(function () {
  const tauri = window.__TAURI__;
  const listen = tauri?.event?.listen;
  const invoke = tauri?.core?.invoke;

  const root = document.getElementById("notification");
  const titleEl = document.getElementById("notificationTitle");
  const bodyEl = document.getElementById("notificationBody");
  const actionsEl = document.getElementById("notificationActions");

  let current = null;
  let hideTimer = null;
  let clickTimer = null;
  const queue = [];

  function escapeText(value) {
    return String(value ?? "");
  }

  function setTone(tone) {
    root.classList.remove("tone-info", "tone-success", "tone-warning", "tone-danger");
    root.classList.add(`tone-${tone || "info"}`);
  }

  function scheduleHide(ttlMs) {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => hide("timeout"), Math.max(1800, Number(ttlMs) || 6000));
  }

  function showNow(payload) {
    current = payload || {};
    titleEl.textContent = escapeText(current.title || "提醒");
    bodyEl.textContent = escapeText(current.body || "");
    actionsEl.replaceChildren(...(current.actions || []).map(renderAction));
    setTone(current.tone);
    root.classList.toggle("expanded", Boolean(current.body) || (current.actions || []).length > 0);
    root.classList.remove("hidden");
    scheduleHide(current.ttl_ms);
  }

  function notificationId(payload) {
    const id = payload && payload.id;
    return id == null ? "" : String(id);
  }

  function isDuplicateNotification(payload) {
    const id = notificationId(payload);
    if (!id) return false;
    if (notificationId(current) === id) return true;
    return queue.some((item) => notificationId(item) === id);
  }

  function show(payload) {
    if (isDuplicateNotification(payload)) return;
    if (current && !root.classList.contains("hidden")) {
      queue.push(payload || {});
      return;
    }
    showNow(payload);
  }

  function renderAction(action) {
    const button = document.createElement("button");
    button.className = "notification-action";
    button.type = "button";
    button.dataset.action = action.id;
    button.textContent = action.label || action.id;
    return button;
  }

  async function hide(reason) {
    clearTimeout(hideTimer);
    clearTimeout(clickTimer);
    root.classList.add("hidden");
    root.classList.remove("expanded");
    current = null;
    setTimeout(() => {
      if (queue.length) {
        showNow(queue.shift());
      } else if (reason !== "local-only" && invoke) {
        invoke("cmd_notification_hide").catch(() => {});
      }
    }, 180);
  }

  root.addEventListener("click", () => {
    if (!current) return;
    clearTimeout(clickTimer);
    clickTimer = setTimeout(() => hide("dismiss"), 180);
  });

  root.addEventListener("dblclick", () => {
    if (!current) return;
    clearTimeout(clickTimer);
    root.classList.add("expanded");
    scheduleHide(Math.max(current.ttl_ms || 6000, 8000));
  });

  actionsEl.addEventListener("click", (event) => {
    const button = event.target.closest("[data-action]");
    if (!button || !current) return;
    event.stopPropagation();
    const action = button.dataset.action;
    if (invoke) {
      invoke("cmd_notification_action", {
        action,
        reminderId: current.reminder_id || null,
      }).catch(() => hide("local-only"));
    }
    hide("local-only");
  });

  window.__notificationShow = show;

  if (listen) {
    listen("notification-show", (event) => show(event.payload)).catch(() => {});
  }
})();
