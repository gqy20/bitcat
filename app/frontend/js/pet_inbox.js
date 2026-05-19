(function() {
  'use strict';

  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  const body = document.getElementById('inbox-body');
  const title = document.getElementById('inbox-title');
  const closeBtn = document.getElementById('inbox-close');

  let snapshot = null;
  let hiddenScreenshots = 0;
  let recentScreenshots = [];

  function escapeHtml(value) {
    return String(value == null ? '' : value)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  function truncateText(text, max) {
    text = String(text || '').trim().replace(/\s+/g, ' ');
    if (text.length <= max) return text;
    return text.slice(0, Math.max(0, max - 1)) + '...';
  }

  function attentionSessions() {
    const sessions = snapshot?.sessions || [];
    return sessions.filter((session) => {
      if (!session || session.display?.quiet) return false;
      const status = String(session.status || '').toLowerCase();
      const tone = String(session.display?.tone || '').toLowerCase();
      return Boolean(session.needs_user) || tone === 'needs_user' || status === 'waiting' || status === 'error';
    });
  }

  function describeAgentSession(session) {
    const display = session?.display || {};
    const heading = display.headline || session.status_label || session.status || 'Agent 需要处理';
    const source = display.source_label || session.source || 'Agent';
    const project = display.project || session.workspace_name || '';
    const kind = display.action_label || '';
    return {
      heading,
      context: [source, project, kind].filter(Boolean).join(' / '),
      detail: display.detail || session.last_response_preview || '',
    };
  }

  function render() {
    const agentSessions = attentionSessions();
    const total = agentSessions.length + hiddenScreenshots;
    title.textContent = `待查看 ${total}`;
    if (!total) {
      body.innerHTML = '<div class="inbox-empty">暂时没有需要查看的内容</div>';
      return;
    }

    let html = '';
    if (agentSessions.length) {
      html += `<section class="inbox-section agent">
        <div class="section-title"><span>Agent 看管</span><span class="count-pill">${agentSessions.length}</span></div>`;
      html += agentSessions.slice(0, 2).map((session) => {
        const item = describeAgentSession(session);
        return `<div class="inbox-item"><strong>${escapeHtml(truncateText(item.heading, 34))}</strong><span>${escapeHtml(truncateText(item.context || item.detail, 46))}</span></div>`;
      }).join('');
      html += '<div class="inbox-actions"><button class="inbox-action" type="button" data-action="open-agent-watch">打开看管</button></div></section>';
    }

    if (hiddenScreenshots) {
      html += `<section class="inbox-section screenshot">
        <div class="section-title"><span>截图观察</span><span class="count-pill">${hiddenScreenshots}</span></div>`;
      if (recentScreenshots.length) {
        html += recentScreenshots.slice(0, 2).map((item) =>
          `<div class="inbox-item"><strong>${escapeHtml(item.day || '最近')}</strong><span>${escapeHtml(truncateText(item.description, 72))}</span></div>`
        ).join('');
      } else {
        html += '<div class="inbox-note">后台完成了截图分析，但气泡显示已关闭。</div>';
      }
      html += '<div class="inbox-actions"><button class="inbox-action" type="button" data-action="observe-now">立即观察</button><button class="inbox-action" type="button" data-action="clear-screenshots">清除计数</button></div></section>';
    }
    body.innerHTML = html;
  }

  async function refresh() {
    if (!invoke) return;
    try {
      const results = await Promise.all([
        invoke('cmd_get_agent_sessions'),
        invoke('cmd_get_hidden_screenshot_count'),
        invoke('cmd_get_recent_screenshot_analyses', { count: 3 }),
      ]);
      snapshot = results[0];
      hiddenScreenshots = Math.max(0, Number(results[1]) || 0);
      recentScreenshots = results[2] || [];
      render();
    } catch (_) {
      render();
    }
  }

  async function close() {
    try {
      await invoke?.('cmd_hide_pet_inbox');
    } catch (_) {}
  }

  closeBtn.addEventListener('click', close);
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') close();
  });

  body.addEventListener('click', async (event) => {
    const button = event.target.closest('[data-action]');
    if (!button || !invoke) return;
    event.preventDefault();
    const action = button.dataset.action;
    if (action === 'open-agent-watch') {
      await invoke('cmd_agent_watch_refresh').catch(() => {});
      await close();
    } else if (action === 'clear-screenshots') {
      hiddenScreenshots = await invoke('cmd_clear_hidden_screenshot_count').catch(() => 0);
      render();
      if (!attentionSessions().length && !hiddenScreenshots) await close();
    } else if (action === 'observe-now') {
      await invoke('cmd_screenshot_now').catch(() => {});
      await close();
    }
  });

  if (listen) {
    listen('screenshot-hidden-count-changed', (event) => {
      hiddenScreenshots = Math.max(0, Number(event.payload) || 0);
      refresh();
    });
    listen('agent-session-update', (event) => {
      snapshot = event.payload || snapshot;
      render();
    });
  }
  window.__petInboxRefresh = refresh;
  refresh();
  setInterval(refresh, 3000);
})();
