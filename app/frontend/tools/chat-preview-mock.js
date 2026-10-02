// 静态对话预览的 Tauri 边界，只提供示例数据和窗口尺寸，不连接 AI 或执行工具。
// 由 chat-preview.html 注入生产气泡页面，保持产品页面自身的加载路径不变。
(function () {
  'use strict';

  const previewOrigin = new URL(document.baseURI).origin;
  const listeners = new Map();
  const resizeListeners = new Set();
  const state = {
    text: '', noticeText: null, next: 'short', timer: null, startTimer: null,
    generation: false, interaction: false, requestId: null, userText: null, source: null,
    nextId: 0, cancelledThrough: 0, queue: [], shortCount: 0, submissions: [], sizes: [],
  };
  const shortReplies = [
    '我在呢。今天可以慢一点。\n\n想聊就说，安静待一会儿也可以。',
    '先看眼前这一小步就好。\n\n剩下的可以等你准备好了再说。',
    '好，我听着。\n\n我们按你觉得舒服的节奏来。',
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
  function requestMeta(request) {
    return { request_id: request.id, user_text: request.userText, source: request.source };
  }

  function scheduleNext() {
    clearTimeout(state.startTimer);
    if (!state.generation && state.queue.length > 0) state.startTimer = setTimeout(startNext, 110);
  }

  function startNext() {
    if (state.generation) return;
    const request = state.queue.shift();
    if (!request) return;
    if (request.id <= state.cancelledThrough) { scheduleNext(); return; }
    clearInterval(state.timer);
    state.requestId = request.id;
    state.userText = request.userText;
    state.source = request.source;
    state.noticeText = null;
    state.text = '';
    state.generation = true;
    emit('bubble-start', requestMeta(request));
    if (request.kind === 'tool') {
      showTool(request.phase, request.details);
      return;
    }
    let cursor = 0;
    state.timer = setInterval(() => {
      if (state.requestId !== request.id || request.id <= state.cancelledThrough) return;
      cursor += request.kind === 'long' ? 12 : 3;
      state.text = request.reply.slice(0, cursor);
      if (cursor >= request.reply.length) {
        clearInterval(state.timer);
        state.generation = false;
        emit('bubble-end', { request_id: request.id, text: state.text });
        scheduleNext();
      }
    }, 90);
  }

  function accept(userText, source, kind) {
    const request = {
      id: ++state.nextId, userText, source, kind,
      reply: kind === 'long' ? longReply : shortReplies[state.shortCount++ % shortReplies.length],
    };
    state.queue.push(request);
    emit('bubble-queued', requestMeta(request));
    scheduleNext();
    return { request_id: request.id };
  }

  function showTool(phase, details) {
    const payload = Object.assign({ request_id: state.requestId, tool_name: 'create_reminder', phase, kind: 'utility' }, details);
    emit('bubble-tool-event', payload);
    if (payload.request_id !== state.requestId) return;
    if (payload.phase === 'failed' || payload.phase === 'blocked' || payload.phase === 'finished') {
      clearInterval(state.timer);
      state.generation = false;
      emit('bubble-end', { request_id: state.requestId, text: state.text || '' });
      scheduleNext();
    }
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
        if (command === 'cmd_consume_bubble_text') return Promise.resolve(state.noticeText);
        if (command === 'cmd_get_bubble_snapshot') return Promise.resolve({
          request_id: state.requestId, user_text: state.userText, source: state.source,
          text: state.text, streaming: state.generation,
        });
        if (command === 'cmd_enter_chat') state.interaction = true;
        if (command === 'cmd_exit_chat') state.interaction = false;
        if (command === 'cmd_hide_bubble') notifyParent({ type: 'hidden' });
        if (command === 'cmd_cancel_chat') {
          const through = args && args.throughRequestId || state.nextId;
          state.cancelledThrough = Math.max(state.cancelledThrough, through);
          state.queue = state.queue.filter(request => request.id > through);
          clearTimeout(state.startTimer);
          if (state.requestId <= through) {
            clearInterval(state.timer);
            state.generation = false;
          }
          emit('bubble-cancelled', { request_id: through });
          scheduleNext();
          return Promise.resolve({ request_id: through });
        }
        if (command === 'cmd_submit_chat') {
          const kind = state.next;
          state.submissions.push(args.text);
          if (kind === 'error') {
            state.next = 'short';
            return Promise.reject(new Error('preview submission failure'));
          }
          return Promise.resolve(accept(args.text, 'text', kind));
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
    enqueue(text, source = 'voice') { return accept(text, source, state.next); },
    notice() {
      if (state.generation || state.interaction || state.queue.length > 0) return false;
      state.noticeText = '我在这里，慢慢来。';
      notifyParent({ type: 'shown' });
      window.__bubble_onShow();
      return true;
    },
    tool(phase, details) {
      if (!state.generation) {
        const request = {
          id: ++state.nextId, userText: '预览中的提醒操作', source: 'gamepad', kind: 'tool', phase, details,
        };
        state.queue.push(request);
        emit('bubble-queued', requestMeta(request));
        scheduleNext();
        return;
      }
      showTool(phase, details);
    },
  };
  window.addEventListener('pagehide', () => {
    clearTimeout(state.startTimer);
    clearInterval(state.timer);
  });
})();
