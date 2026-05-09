// Phase A: 硬编码 6 个按钮，点击调用后端 cmd_execute_panel_action
const ACTIONS = [
  { id: 'vscode',     icon: '💻', label: 'VSCode' },
  { id: 'browser',    icon: '🌐', label: '浏览器' },
  { id: 'explorer',   icon: '📁', label: '资源管理器' },
  { id: 'powershell', icon: '⚡', label: 'PowerShell' },
  { id: 'notepad',    icon: '📝', label: '记事本' },
  { id: 'ai',         icon: '🤖', label: '问 AI' },
];

const invoke = window.__TAURI__?.core?.invoke;

function render() {
  const grid = document.getElementById('grid');
  grid.innerHTML = '';
  ACTIONS.forEach(a => {
    const cell = document.createElement('div');
    cell.className = a.id === 'ai' ? 'cell disabled' : 'cell';
    cell.innerHTML = `
      <div class="icon">${a.icon}</div>
      <div class="label">${a.label}</div>
    `;
    cell.addEventListener('click', () => onClick(a.id));
    grid.appendChild(cell);
  });
}

async function onClick(id) {
  if (id === 'ai') {
    console.log('AI 入口待 Phase D 实现');
    return;
  }
  if (!invoke) {
    console.error('Tauri invoke 不可用');
    return;
  }
  try {
    await invoke('cmd_execute_panel_action', { id });
    await invoke('cmd_hide_panel');
  } catch (e) {
    console.error('执行动作失败:', e);
  }
}

document.addEventListener('keydown', async (e) => {
  if (e.key === 'Escape' && invoke) {
    await invoke('cmd_hide_panel');
  }
});

render();
