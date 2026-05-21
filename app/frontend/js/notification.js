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

  function show(payload) {
    current = payload || {};
    titleEl.textContent = escapeText(current.title || "提醒");
    bodyEl.textContent = escapeText(current.body || "");
    actionsEl.replaceChildren(...(current.actions || []).map(renderAction));
    setTone(current.tone);
    root.classList.toggle("expanded", Boolean(current.body) || (current.actions || []).length > 0);
    root.classList.remove("hidden");
    scheduleHide(current.ttl_ms);
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
    root.classList.add("hidden");
    root.classList.remove("expanded");
    if (reason !== "local-only" && invoke) {
      setTimeout(() => invoke("cmd_notification_hide").catch(() => {}), 180);
    }
  }

  root.addEventListener("click", () => {
    if (!current) return;
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
