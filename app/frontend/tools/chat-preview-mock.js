// 静态对话预览的 Tauri 边界，只提供示例数据和窗口尺寸，不连接 AI 或执行工具。
// 由 chat-preview.html 注入生产气泡页面，保持产品页面自身的加载路径不变。
(function () {
  'use strict';

  const previewOrigin = new URL(document.baseURI).origin;
  const listeners = new Map();
  const resizeListeners = new Set();
  const state = {
    text: null, next: 'short', timer: null, startTimer: null,
    generation: false, interaction: false, runId: 0, shortCount: 0,
    submissions: [], sizes: [],
  };
  const shortReplies = [
    '我在呢。今天可以慢一点。\n\n先做一件小事，剩下的我们再一起想。',
    '先选一件十分钟内能完成的小事吧。\n\n做完以后歇一会儿，再决定下一步。',
    '好，我们就按这个节奏来。\n\n你可以接着说，我会认真听。',
  ];
  const longReply = [
    '## 先留一点余量',
    '今天可以先把最重要的事写下来，再留出一段不被打断的时间。计划只要能帮助你开始，就已经够用了。',
    '### 一个轻松的安排',
    '- 整理今天的目标，只留下最想完成的一件事。\n- 完成一个明确的小任务。\n- 起身走一走，再决定是否继续。',
    '### 把下一步写清楚',
    '任务可以小一点，比如读完一页资料，或者写出一个能运行的例子。先把它做完，再考虑后面的部分。',
    '```javascript\nconst nextStep = "先完成一小步";\nconsole.log(nextStep);\n```',
    '### 给休息留个位置',
    '| 时段 | 安排 | 备注 |\n| --- | --- | --- |\n| 上午 | 主要任务 | 保持安静 |\n| 下午 | 整理复盘 | 留些余量 |\n| 傍晚 | 散步休息 | 放下未完成的事 |',
    '如果中途累了，就停一下。把已经完成的部分记下来，明天也更容易接着做。',
    '### 今天结束之前',
    '回顾一件做成的小事，再写一句明天想继续的内容。剩下的安排可以慢慢调整。',
    '如果你今天只想安静待着，也可以直接告诉我。我会按你的节奏来。',
  ].join('\n\n');

  function emit(name, payload) {
    (listeners.get(name) || new Set()).forEach(callback => callback({ payload }));
  }
  function notifyParent(data) {
    parent.postMessage(data, previewOrigin);
  }
  function generate(text, runId, kind) {
    if (runId !== state.runId) return;
    clearInterval(state.timer);
    state.text = '';
    state.generation = true;
    emit('bubble-start');
    let cursor = 0;
    state.timer = setInterval(() => {
      if (runId !== state.runId) { clearInterval(state.timer); return; }
      cursor += kind === 'long' ? 12 : 3;
      state.text = text.slice(0, cursor);
      if (cursor >= text.length) {
        clearInterval(state.timer);
        state.generation = false;
        emit('bubble-end', { text: state.text });
      }
    }, 90);
  }

  const currentWindow = {
    label: 'bubble-preview',
    setSize(size) {
      state.sizes.push([size.width, size.height]);
      notifyParent({ type: 'size', width: size.width, height: size.height });
      return Promise.resolve();
    },
    innerSize: () => Promise.resolve({ width: innerWidth, height: innerHeight }),
    scaleFactor: () => Promise.resolve(1),
    onResized(callback) {
      resizeListeners.add(callback);
      return Promise.resolve(() => resizeListeners.delete(callback));
    },
    startResizeDragging: () => Promise.resolve(),
  };
  window.addEventListener('resize', () => resizeListeners.forEach(callback => callback()));
  window.__TAURI__ = {
    core: {
      invoke(command, args) {
        if (command === 'cmd_consume_bubble_text') return Promise.resolve(state.text);
        if (command === 'cmd_enter_chat') state.interaction = true;
        if (command === 'cmd_exit_chat') state.interaction = false;
        if (command === 'cmd_hide_bubble') notifyParent({ type: 'hidden' });
        if (command === 'cmd_cancel_chat') {
          state.runId += 1;
          clearTimeout(state.startTimer);
          clearInterval(state.timer);
          state.generation = false;
          emit('bubble-cancelled');
        }
        if (command === 'cmd_submit_chat') {
          const kind = state.next;
          state.submissions.push(args.text);
          if (kind === 'error') {
            state.next = 'short';
            return Promise.reject(new Error('preview submission failure'));
          }
          const reply = kind === 'long' ? longReply : shortReplies[state.shortCount++ % shortReplies.length];
          const runId = ++state.runId;
          state.text = '';
          clearTimeout(state.startTimer);
          state.startTimer = setTimeout(() => generate(reply, runId, kind), 110);
        }
        return Promise.resolve();
      },
    },
    event: {
      listen(name, callback) {
        if (!listeners.has(name)) listeners.set(name, new Set());
        listeners.get(name).add(callback);
        return Promise.resolve(() => listeners.get(name).delete(callback));
      },
    },
    window: {
      getCurrentWindow: () => currentWindow,
      LogicalSize: class { constructor(width, height) { this.width = width; this.height = height; } },
    },
  };
  window.__preview = {
    state,
    open() { notifyParent({ type: 'shown' }); emit('chat-open'); },
    next(kind) { state.next = kind; },
    notice() {
      if (state.generation || state.interaction) return false;
      state.text = '我在这里，慢慢来。';
      notifyParent({ type: 'shown' });
      window.__bubble_onShow();
      return true;
    },
    tool(phase, details) {
      if (!state.generation) {
        state.runId += 1;
        clearTimeout(state.startTimer);
        clearInterval(state.timer);
        state.text = '';
        state.generation = true;
        emit('bubble-start');
      }
      emit('bubble-tool-event', Object.assign({ tool_name: 'create_reminder', phase, kind: 'utility' }, details));
      if (phase === 'failed' || phase === 'blocked' || phase === 'finished') {
        clearTimeout(state.startTimer);
        clearInterval(state.timer);
        state.generation = false;
        emit('bubble-end', { text: state.text || '' });
      }
    },
  };
  window.addEventListener('pagehide', () => {
    clearTimeout(state.startTimer);
    clearInterval(state.timer);
  });
})();
