// Agent Watch / 通知岛 / 设置页共用的运行时状态图标。
// 场景图标回答“在做什么”，状态色与文字回答“进行到哪一步”；不引入字体图标或 emoji。
(function () {
  'use strict';

  const PATHS = {
    shell: '<rect x="3" y="4" width="18" height="16" rx="3"/><path d="m7 9 3 3-3 3M13 15h4"/>',
    edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4Z"/>',
    read: '<path d="M6 2h8l4 4v16H6z"/><path d="M14 2v5h5M9 12h6M9 16h6"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/>',
    test: '<path d="M9 3h6M10 3v6l-5 9a2 2 0 0 0 2 3h10a2 2 0 0 0 2-3l-5-9V3M8 15h8"/>',
    agent: '<rect x="3" y="3" width="7" height="7" rx="2"/><rect x="14" y="14" width="7" height="7" rx="2"/><path d="M10 6h4a3 3 0 0 1 3 3v5M6 10v4a3 3 0 0 0 3 3h5"/>',
    permission: '<path d="M12 3 20 6v6c0 5-4 8-8 9-4-1-8-4-8-9V6z"/><path d="m9 12 2 2 4-4"/>',
    reminder: '<path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9M10 21h4"/>',
    connector: '<path d="M8 3v5M16 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4"/>',
    activity: '<path d="M3 12h4l3-7 4 14 3-7h4"/>',
    compacting: '<path d="m12 3 9 5-9 5-9-5zM3 12l9 5 9-5M3 16l9 5 9-5"/>',
    done: '<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>',
    error: '<circle cx="12" cy="12" r="9"/><path d="m9 9 6 6M15 9l-6 6"/>',
    interrupted: '<circle cx="12" cy="12" r="9"/><rect x="9" y="9" width="6" height="6" rx="1"/>',
    generic: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="2"/>',
  };

  function render(name, className) {
    const key = PATHS[name] ? name : 'generic';
    const cls = className ? ` ${className}` : '';
    return `<svg class="agent-state-icon state-${stateForName(key)}${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${PATHS[key]}</svg>`;
  }

  function stateForName(name) {
    if (name === 'done') return 'success';
    if (name === 'error') return 'danger';
    if (name === 'interrupted') return 'muted';
    if (name === 'permission' || name === 'reminder') return 'waiting';
    if (name === 'activity' || name === 'compacting') return 'active';
    return 'muted';
  }

  function stateForStatus(status) {
    switch (String(status || '').toLowerCase()) {
      case 'working':
      case 'tool_running':
      case 'compacting':
        return 'active';
      case 'waiting':
        return 'waiting';
      case 'done':
        return 'success';
      case 'error':
        return 'danger';
      default:
        return 'muted';
    }
  }

  function iconForSession(session) {
    const status = String(session?.status || '').toLowerCase();
    if (status === 'done') return 'done';
    if (status === 'error') return 'error';
    if (status === 'interrupted') return 'interrupted';
    if (status === 'waiting') return 'permission';
    if (status === 'compacting') return 'compacting';
    if (session?.background || session?.agent_id || session?.task_id) return 'agent';

    const action = String(session?.display?.action_label || '').toLowerCase();
    if (action === 'shell') return 'shell';
    if (action === 'patch' || action === 'edit' || action === 'write') return 'edit';
    if (action === 'read') return 'read';
    if (action === 'search') return 'search';
    if (action === 'agent' || action === 'task') return 'agent';

    const tool = String(session?.tool_name || '').toLowerCase();
    if (tool.includes('bash') || tool.includes('shell') || tool.includes('powershell')) return 'shell';
    if (tool.includes('edit') || tool.includes('write') || tool.includes('patch')) return 'edit';
    if (tool.includes('read')) return 'read';
    if (tool.includes('grep') || tool.includes('glob') || tool.includes('search')) return 'search';
    if (tool.includes('test')) return 'test';
    return 'activity';
  }

  function setText(element, text) {
    if (!element) return;
    const next = String(text ?? '');
    if (element.textContent === next) return;
    const reduceMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
    if (reduceMotion || typeof element.animate !== 'function') {
      element.textContent = next;
      return;
    }
    element.textContent = next;
    element.animate(
      [
        { opacity: 0, transform: 'translateY(2px)' },
        { opacity: 1, transform: 'translateY(0)' },
      ],
      { duration: 160, easing: 'ease-out' }
    );
  }

  function pulse(element) {
    if (!element || typeof element.animate !== 'function') return;
    if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return;
    element.animate(
      [
        { opacity: 0.55, transform: 'scale(0.92)' },
        { opacity: 1, transform: 'scale(1)' },
      ],
      { duration: 900, easing: 'ease-out' }
    );
  }

  window.AgentIcons = {
    render,
    stateForStatus,
    iconForSession,
    setText,
    pulse,
  };
})();
