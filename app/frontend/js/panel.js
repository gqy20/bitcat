// Phase A: 硬编码 6 个按钮，点击调用后端 cmd_execute_panel_action
const ACTIONS = [
  { id: 'vscode',     icon: '💻', label: 'VSCode' },
  { id: 'browser',    icon: '🌐', label: '浏览器' },
  { id: 'explorer',   icon: '📁', label: '资源管理器' },
  { id: 'powershell', icon: '⚡', label: 'PowerShell' },
  { id: 'notepad',    icon: '📝', label: '记事本' },
  { id: 'ai',         icon: '🤖', label: '问 AI' },
];

const COLS = 3;
const ROWS = 2;

const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;

let selectedIndex = 0;

function render() {
  const grid = document.getElementById('grid');
  grid.innerHTML = '';
  ACTIONS.forEach((a, i) => {
    const cell = document.createElement('div');
    cell.className = a.id === 'ai' ? 'cell disabled' : 'cell';
    cell.dataset.index = i;
    cell.innerHTML = `
      <div class="icon">${a.icon}</div>
      <div class="label">${a.label}</div>
    `;
    cell.addEventListener('click', () => {
      selectedIndex = i;
      updateSelection();
      activateSelected();
    });
    cell.addEventListener('mouseenter', () => {
      selectedIndex = i;
      updateSelection();
    });
    grid.appendChild(cell);
  });
  updateSelection();
}

function updateSelection() {
  document.querySelectorAll('.cell').forEach((cell, i) => {
    cell.classList.toggle('selected', i === selectedIndex);
  });
}

// dx: 列偏移；dy: 手柄约定（1=上，-1=下），需转屏幕坐标
function moveSelection(dx, dy) {
  const x = selectedIndex % COLS;
  const y = Math.floor(selectedIndex / COLS);
  const nx = Math.max(0, Math.min(COLS - 1, x + dx));
  const ny = Math.max(0, Math.min(ROWS - 1, y - dy));
  selectedIndex = ny * COLS + nx;
  updateSelection();
}

async function activateSelected() {
  const a = ACTIONS[selectedIndex];
  if (!a || a.id === 'ai') {
    console.log('AI 入口待 Phase D 实现');
    return;
  }
  if (!invoke) {
    console.error('Tauri invoke 不可用');
    return;
  }
  try {
    await invoke('cmd_execute_panel_action', { id: a.id });
    await invoke('cmd_hide_panel');
  } catch (e) {
    console.error('执行动作失败:', e);
  }
}

async function closePanel() {
  if (invoke) {
    await invoke('cmd_hide_panel');
  }
}

// 键盘导航
document.addEventListener('keydown', async (e) => {
  switch (e.key) {
    case 'Escape':
      e.preventDefault();
      await closePanel();
      break;
    case 'ArrowUp':
      e.preventDefault();
      moveSelection(0, 1);
      break;
    case 'ArrowDown':
      e.preventDefault();
      moveSelection(0, -1);
      break;
    case 'ArrowLeft':
      e.preventDefault();
      moveSelection(-1, 0);
      break;
    case 'ArrowRight':
      e.preventDefault();
      moveSelection(1, 0);
      break;
    case 'Enter':
    case ' ':
      e.preventDefault();
      await activateSelected();
      break;
  }
});

// 手柄事件（dpad / A / B 通过后端 emit 转发）
async function initEvents() {
  const log = (msg) => {
    if (invoke) invoke('cmd_panel_log', { msg }).catch(() => {});
    console.log('[panel]', msg);
  };
  const tauri = window.__TAURI__;
  log(`__TAURI__ keys: ${tauri ? Object.keys(tauri).join(',') : '<undefined>'}`);
  const evt = tauri?.event;
  log(`event API: ${evt ? Object.keys(evt).join(',') : '<undefined>'}`);
  if (!evt?.listen) {
    log('event.listen 不可用 —— 手柄导航失效');
    return;
  }
  try {
    await evt.listen('panel-nav', (e) => {
      log(`panel-nav payload=${JSON.stringify(e.payload)}`);
      const p = e.payload;
      const dx = Array.isArray(p) ? p[0] : p.dx;
      const dy = Array.isArray(p) ? p[1] : p.dy;
      moveSelection(dx | 0, dy | 0);
    });
    await evt.listen('panel-confirm', () => activateSelected());
    await evt.listen('panel-close', () => closePanel());
    log('listen 注册完成');
  } catch (e) {
    log(`listen 异常: ${e}`);
  }
}

initEvents();

// 阻止透明窗口的滚轮弹回（WebView2 native scroll 不稳定）
document.addEventListener('wheel', (e) => e.preventDefault(), { passive: false });

// 重新显示时复位到第一项
window.addEventListener('focus', () => {
  selectedIndex = 0;
  updateSelection();
});

render();
