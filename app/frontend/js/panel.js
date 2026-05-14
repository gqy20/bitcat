const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;

let actions = [];
let columns = 3;
let rows = 3;
let selectedIndex = 0;

function render() {
  const grid = document.getElementById('grid');
  grid.innerHTML = '';
  document.documentElement.style.setProperty('--panel-columns', String(columns));
  document.documentElement.style.setProperty('--panel-rows', String(rows));
  actions.forEach((a, i) => {
    const cell = document.createElement('div');
    cell.className = a.enabled ? 'cell' : 'cell disabled';
    cell.dataset.index = i;
    cell.innerHTML = `
      <div class="icon">${a.icon}</div>
      <div class="label">${a.label}</div>
    `;
    cell.addEventListener('click', () => {
      if (!a.enabled) return;
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
  if (actions.length === 0) return;
  const x = selectedIndex % columns;
  const y = Math.floor(selectedIndex / columns);
  const nx = Math.max(0, Math.min(columns - 1, x + dx));
  const ny = Math.max(0, Math.min(rows - 1, y - dy));
  selectedIndex = Math.min(actions.length - 1, ny * columns + nx);
  updateSelection();
}

async function activateSelected() {
  const a = actions[selectedIndex];
  if (!a || !a.enabled) return;
  if (!invoke) {
    console.error('Tauri invoke 不可用');
    return;
  }
  try {
    await invoke('cmd_execute_panel_action', { id: a.id });
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

async function loadPanelActions() {
  if (!invoke) {
    actions = [];
    render();
    return;
  }
  try {
    const vm = await invoke('cmd_get_panel_actions');
    columns = Math.max(1, Number(vm.columns) || 3);
    rows = Math.max(1, Number(vm.rows) || 3);
    actions = Array.isArray(vm.actions) ? vm.actions : [];
    selectedIndex = Math.min(selectedIndex, Math.max(0, actions.length - 1));
    render();
  } catch (e) {
    console.error('加载面板动作失败:', e);
    actions = [];
    render();
  }
}

// 阻止透明窗口的滚轮弹回（WebView2 native scroll 不稳定）
document.addEventListener('wheel', (e) => e.preventDefault(), { passive: false });

// 重新显示时复位到第一项
window.addEventListener('focus', () => {
  selectedIndex = 0;
  loadPanelActions();
});

loadPanelActions();
