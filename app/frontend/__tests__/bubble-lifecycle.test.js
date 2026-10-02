// bubble-lifecycle.test.js — 用生产页面和脚本验证真实聊天交互。
// 通过 Tauri 边界控制异步返回，覆盖草稿、发送、进度和窗口收起之间的竞态。

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const bubbleHtml = readFileSync(resolve(process.cwd(), 'bubble.html'), 'utf8');
const bubbleScript = readFileSync(resolve(process.cwd(), 'js/bubble.js'), 'utf8');
const markedScript = readFileSync(resolve(process.cwd(), 'js/vendor/marked.min.js'), 'utf8');

function deferred() {
  let resolvePromise;
  let rejectPromise;
  const promise = new Promise((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });
  return { promise, resolve: resolvePromise, reject: rejectPromise };
}

async function flushPromises() {
  // IPC 的 then/finally 会继续排入微任务；不运行无限轮询定时器。
  for (let i = 0; i < 12; i++) await Promise.resolve();
}

function setScrollSize(element, scrollHeight = 1200, clientHeight = 240) {
  Object.defineProperties(element, {
    scrollHeight: { value: scrollHeight, configurable: true },
    clientHeight: { value: clientHeight, configurable: true },
  });
}

function isVisible(element) {
  if (!element || element.hidden) return false;
  for (let current = element; current; current = current.parentElement) {
    if (current.style.display === 'none' || current.hidden) return false;
  }
  return true;
}

describe('bubble production lifecycle', () => {
  let handlers;
  let invoke;
  let api;
  let dom;
  let registeredListeners;
  let currentWindow;

  beforeEach(async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-10-02T08:00:00Z'));
    localStorage.clear();

    registeredListeners = [];
    for (const target of [document, window]) {
      const original = target.addEventListener.bind(target);
      vi.spyOn(target, 'addEventListener').mockImplementation((type, callback, options) => {
        registeredListeners.push({ target, type, callback, options });
        original(type, callback, options);
      });
    }

    // 将动画帧纳入 fake timers，避免旧帧跨测试改变滚动位置。
    vi.stubGlobal('requestAnimationFrame', (callback) => setTimeout(() => callback(Date.now()), 16));
    vi.stubGlobal('cancelAnimationFrame', (id) => clearTimeout(id));
    vi.spyOn(console, 'error').mockImplementation(() => {});

    handlers = new Map();
    api = {
      consume: vi.fn().mockResolvedValue('我在这里。'),
      submit: vi.fn().mockResolvedValue(undefined),
      cancel: vi.fn().mockResolvedValue(undefined),
    };
    invoke = vi.fn((command, args) => {
      if (command === 'cmd_consume_bubble_text') return api.consume(args);
      if (command === 'cmd_submit_chat') return api.submit(args);
      if (command === 'cmd_cancel_chat') return api.cancel(args);
      return Promise.resolve(undefined);
    });
    currentWindow = {
      setSize: vi.fn().mockResolvedValue(undefined),
      innerSize: vi.fn().mockResolvedValue({ width: 360, height: 390 }),
      scaleFactor: vi.fn().mockResolvedValue(1),
      onResized: vi.fn().mockResolvedValue(() => {}),
      startResizeDragging: vi.fn().mockResolvedValue(undefined),
    };
    window.__TAURI__ = {
      core: { invoke },
      event: {
        listen: vi.fn((name, callback) => {
          handlers.set(name, callback);
          return Promise.resolve(() => handlers.delete(name));
        }),
      },
      window: {
        getCurrentWindow: vi.fn(() => currentWindow),
        LogicalSize: class LogicalSize {
          constructor(width, height) {
            this.width = width;
            this.height = height;
          }
        },
      },
    };

    await mountBubble();
  });

  async function mountBubble(initialText = '我在这里。') {
    // 冷启动用例重新挂载窗口；旧闭包和定时器不会留在新页面中。
    for (const { target, type, callback, options } of registeredListeners) {
      target.removeEventListener(type, callback, options);
    }
    registeredListeners = [];
    vi.clearAllTimers();
    handlers.clear();
    invoke.mockClear();
    api.consume.mockClear().mockResolvedValue(initialText);
    const page = new DOMParser().parseFromString(bubbleHtml, 'text/html');
    document.documentElement.innerHTML = page.documentElement.innerHTML;
    document.documentElement.removeAttribute('style');
    document.body.className = page.body.className;
    window.eval(markedScript);
    window.eval(bubbleScript);
    document.dispatchEvent(new Event('DOMContentLoaded'));
    await flushPromises();
    await vi.advanceTimersByTimeAsync(40);
    api.consume.mockResolvedValue('');
    dom = {
      content: document.getElementById('content'),
      body: document.getElementById('contentBody'),
      input: document.getElementById('chatInput'),
      inputRow: document.getElementById('inputRow'),
      send: document.getElementById('chatSend'),
      collapse: document.getElementById('collapseBtn'),
    };
  }

  afterEach(() => {
    for (const { target, type, callback, options } of registeredListeners || []) {
      target.removeEventListener(type, callback, options);
    }
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    delete window.__TAURI__;
    delete window.marked;
    for (const key of Object.keys(window)) {
      if (key.startsWith('__bubble_')) delete window[key];
    }
  });

  async function emit(name, payload) {
    expect(handlers.has(name), `生产脚本应监听 ${name}`).toBe(true);
    await handlers.get(name)({ payload });
    await flushPromises();
  }

  async function openChat() {
    await emit('chat-open');
    await vi.advanceTimersByTimeAsync(40);
  }

  function typeDraft(value) {
    dom.input.value = value;
    dom.input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
  }

  function pressEnter(options = {}) {
    const event = new KeyboardEvent('keydown', {
      key: 'Enter', bubbles: true, cancelable: true, ...options,
    });
    dom.input.dispatchEvent(event);
    return event;
  }

  function submittedTexts() {
    return invoke.mock.calls
      .filter(([command]) => command === 'cmd_submit_chat')
      .map(([, args]) => args.text);
  }

  function activeHeaderStatuses() {
    return ['thinking', 'toolProgress', 'controlNote']
      .map((id) => document.getElementById(id))
      .filter(isVisible)
      .map((element) => element.id);
  }

  async function beginReply(prompt = '帮我处理一件事') {
    await openChat();
    api.consume.mockResolvedValue('');
    typeDraft(prompt);
    pressEnter();
    await flushPromises();
    await emit('bubble-start');
  }

  it('keeps an active draft beyond five seconds and restores it after collapse', async () => {
    await openChat();
    typeDraft('还没写完，先想一下。');
    await vi.advanceTimersByTimeAsync(16000);

    expect(isVisible(dom.inputRow)).toBe(true);
    expect(document.body.classList.contains('hidden')).toBe(false);
    expect(dom.input.value).toBe('还没写完，先想一下。');

    dom.collapse.click();
    await vi.advanceTimersByTimeAsync(220);
    expect(document.body.classList.contains('hidden')).toBe(true);
    await openChat();
    expect(isVisible(dom.inputRow)).toBe(true);
    expect(dom.input.value).toBe('还没写完，先想一下。');
    expect(submittedTexts()).toEqual([]);
  });

  it('restores a failed submission and permits one explicit retry', async () => {
    const request = deferred();
    api.submit.mockImplementationOnce(() => request.promise);
    await openChat();
    typeDraft('帮我整理今天的安排');

    dom.send.click();
    // 用户也可能在发送结果返回前继续输入；这不能变成第二次提交。
    typeDraft('帮我整理今天的安排');
    dom.send.click();
    pressEnter();
    await flushPromises();
    expect(submittedTexts()).toEqual(['帮我整理今天的安排']);

    request.reject(new Error('暂时无法连接'));
    await flushPromises();
    expect(isVisible(dom.inputRow)).toBe(true);
    expect(dom.input.value).toBe('帮我整理今天的安排');
    expect(dom.send.disabled).toBe(false);
    expect(document.body.textContent).toMatch(/失败|没能|没有|再试|重试/);

    dom.send.click();
    await flushPromises();
    expect(submittedTexts()).toEqual(['帮我整理今天的安排', '帮我整理今天的安排']);
  });

  it('keeps the draft when Escape collapses the chat', async () => {
    await openChat();
    typeDraft('换个窗口后再接着写');
    const escape = new KeyboardEvent('keydown', {
      key: 'Escape', bubbles: true, cancelable: true,
    });
    dom.input.dispatchEvent(escape);
    await vi.advanceTimersByTimeAsync(220);
    expect(escape.defaultPrevented).toBe(true);
    expect(document.body.classList.contains('hidden')).toBe(true);
    await openChat();
    expect(dom.input.value).toBe('换个窗口后再接着写');
    expect(submittedTexts()).toEqual([]);
  });

  it('cancels a pending native hide when chat reopens during the collapse animation', async () => {
    await openChat();
    typeDraft('马上继续写');
    dom.collapse.click();
    await openChat();
    await vi.advanceTimersByTimeAsync(220);

    expect(document.body.classList.contains('hidden')).toBe(false);
    expect(dom.input.value).toBe('马上继续写');
    expect(invoke.mock.calls.filter(([command]) => command === 'cmd_hide_bubble')).toHaveLength(0);
  });

  it('keeps the composer during generation, preserves the next draft, and only stops the reply', async () => {
    await openChat();
    typeDraft('今天有点累');
    dom.send.click();
    await flushPromises();
    await vi.advanceTimersByTimeAsync(300);

    expect(isVisible(dom.inputRow)).toBe(true);
    expect(dom.input.disabled).toBe(false);
    expect(dom.input.readOnly).toBe(false);
    typeDraft('我也想说说明天的事');
    pressEnter();
    expect(submittedTexts()).toEqual(['今天有点累']);
    expect(dom.input.value).toBe('我也想说说明天的事');

    const stops = Array.from(document.querySelectorAll('button')).filter((button) =>
      isVisible(button) && /停止/.test(button.getAttribute('aria-label') || button.title || button.textContent));
    expect(stops).toHaveLength(1);
    stops[0].click();
    await flushPromises();
    expect(invoke.mock.calls.filter(([command]) => command === 'cmd_cancel_chat')).toHaveLength(1);
    expect(dom.input.value).toBe('我也想说说明天的事');
    expect(submittedTexts()).toEqual(['今天有点累']);
  });

  it('shows an enabled stop action immediately when a new stream starts', async () => {
    await openChat();
    typeDraft('下一句先写在这里');
    await emit('bubble-start');

    expect(dom.send.getAttribute('aria-label')).toBe('停止回复');
    expect(dom.send.disabled).toBe(false);
    expect(dom.input.value).toBe('下一句先写在这里');
    dom.send.click();
    await flushPromises();
    expect(invoke.mock.calls.filter(([command]) => command === 'cmd_cancel_chat')).toHaveLength(1);
  });

  it('can send the next draft after stopping a reply', async () => {
    await openChat();
    typeDraft('第一轮');
    dom.send.click();
    await flushPromises();
    typeDraft('第二轮');
    dom.send.click();
    await flushPromises();
    expect(dom.input.value).toBe('第二轮');

    dom.send.click();
    await flushPromises();
    expect(submittedTexts()).toEqual(['第一轮', '第二轮']);
    expect(invoke.mock.calls.filter(([command]) => command === 'cmd_cancel_chat')).toHaveLength(1);
    expect(dom.send.getAttribute('aria-label')).toBe('停止回复');
  });

  it('keeps the next draft and blocks sending until the cancellation acknowledgement arrives', async () => {
    const cancellation = deferred();
    api.cancel.mockImplementationOnce(() => cancellation.promise);
    await openChat();
    api.consume.mockResolvedValue('这轮回复还在生成。');
    typeDraft('先回答这一句');
    pressEnter();
    await flushPromises();
    await emit('bubble-start');
    typeDraft('停止后再发这一句');
    dom.send.click();
    await flushPromises();

    expect(dom.send.disabled).toBe(true);
    expect(dom.send.textContent).toContain('稍等');
    pressEnter();
    dom.send.click();
    await flushPromises();
    expect(submittedTexts()).toEqual(['先回答这一句']);
    expect(dom.input.value).toBe('停止后再发这一句');
    expect(api.cancel).toHaveBeenCalledTimes(1);

    cancellation.resolve();
    await flushPromises();
    expect(dom.send.disabled).toBe(false);
    pressEnter();
    dom.send.click();
    pressEnter();
    await flushPromises();
    expect(submittedTexts()).toEqual(['先回答这一句', '停止后再发这一句']);
    expect(api.cancel).toHaveBeenCalledTimes(1);
  });

  it('restores a retryable stop action and preserves the draft when cancellation fails', async () => {
    const cancellation = deferred();
    api.cancel.mockImplementationOnce(() => cancellation.promise);
    await openChat();
    api.consume.mockResolvedValue('回复仍在生成，不应该假装已经停止。');
    typeDraft('请接着回答');
    pressEnter();
    await flushPromises();
    await emit('bubble-start');
    typeDraft('这句草稿必须留着');
    dom.send.click();
    await flushPromises();
    cancellation.reject(new Error('无法确认停止结果'));
    await flushPromises();

    expect(dom.send.getAttribute('aria-label')).toBe('停止回复');
    expect(dom.send.textContent).toContain('停止');
    expect(dom.send.disabled).toBe(false);
    expect(dom.input.value).toBe('这句草稿必须留着');
    const feedback = document.getElementById('chatFeedback');
    expect(feedback.hidden).toBe(false);
    expect(feedback.textContent).toContain('停止没有成功');
    expect(feedback.textContent).toContain('再点停止');
    expect(activeHeaderStatuses()).not.toContain('controlNote');
    expect(submittedTexts()).toEqual(['请接着回答']);

    dom.send.click();
    await flushPromises();
    expect(api.cancel).toHaveBeenCalledTimes(2);
    expect(dom.input.value).toBe('这句草稿必须留着');
    expect(dom.send.getAttribute('aria-label')).toBe('发送消息');
    expect(feedback.hidden).toBe(true);
    expect(submittedTexts()).toEqual(['请接着回答']);
  });

  it('replaces thinking with tool progress and keeps that progress until the tool finishes', async () => {
    await beginReply();
    expect(activeHeaderStatuses()).toEqual(['thinking']);
    const tool = { tool_name: 'create_reminder', label: '设置提醒', kind: 'utility', internal_call_id: 'reminder-1' };
    await emit('bubble-tool-event', { ...tool, phase: 'planned' });
    expect(activeHeaderStatuses()).toEqual(['toolProgress']);
    const progressLabel = document.getElementById('toolProgress').textContent.trim();
    expect(progressLabel).toContain('正在设置提醒');
    expect(isVisible(document.getElementById('toolStatus'))).toBe(false);

    api.consume.mockResolvedValue('先写下正在处理的这段回复。');
    await vi.advanceTimersByTimeAsync(120);
    expect(dom.body.textContent).toContain('先写下正在处理的这段回复');
    expect(activeHeaderStatuses()).toEqual(['toolProgress']);
    expect(document.getElementById('toolProgress').textContent.trim()).toBe(progressLabel);
    expect(dom.content.textContent).not.toContain(progressLabel);

    await emit('bubble-tool-event', { ...tool, phase: 'finished' });
    expect(activeHeaderStatuses()).toEqual([]);
    expect(isVisible(document.getElementById('toolStatus'))).toBe(false);
    expect(dom.body.textContent.split('先写下正在处理的这段回复。')).toHaveLength(2);
    await emit('bubble-tool-event', { ...tool, phase: 'finished' });
    await vi.advanceTimersByTimeAsync(120);
    expect(activeHeaderStatuses()).toEqual([]);
    expect(dom.body.textContent.split('先写下正在处理的这段回复。')).toHaveLength(2);
  });

  it.each(['failed', 'blocked'])('keeps the full %s explanation after the reply ends and clears it for the next turn', async (phase) => {
    await beginReply();
    const label = '打开这份包含会议行动项、负责人与下周计划的完整工作说明';
    // 未知操作沿用事件提供的人话说明，检验长说明不会被顶部短文案替换。
    const tool = { tool_name: 'custom_operation', label, kind: 'utility', internal_call_id: 'operation-1' };
    await emit('bubble-tool-event', { ...tool, phase: 'planned' });
    expect(activeHeaderStatuses()).toEqual(['toolProgress']);
    await emit('bubble-tool-event', { ...tool, phase });
    const failure = document.getElementById('toolStatus');
    const fullExplanation = failure.textContent;
    expect(isVisible(failure)).toBe(true);
    expect(fullExplanation).toContain(label);
    expect(activeHeaderStatuses()).toEqual([]);

    await emit('bubble-end', { text: '这次操作没有完成，请按说明处理。' });
    expect(isVisible(failure)).toBe(true);
    expect(failure.textContent).toBe(fullExplanation);
    expect(activeHeaderStatuses()).toEqual([]);
    expect(dom.body.textContent).toContain('这次操作没有完成');

    typeDraft('换一件事继续聊');
    pressEnter();
    await flushPromises();
    await emit('bubble-start');
    expect(isVisible(failure)).toBe(false);
    expect(failure.textContent).toBe('');
    expect(activeHeaderStatuses()).toEqual(['thinking']);
  });

  it.each(['planned', 'finished', 'failed'])('does not revive progress when a late %s tool event arrives after stop', async (phase) => {
    await beginReply();
    const tool = { tool_name: 'create_reminder', label: '设置提醒', kind: 'utility', internal_call_id: 'stopped-reminder' };
    await emit('bubble-tool-event', { ...tool, phase: 'planned' });
    dom.send.click();
    await flushPromises();
    expect(activeHeaderStatuses()).toEqual(['controlNote']);
    expect(document.getElementById('controlNote').textContent).toBe('已停止');

    await emit('bubble-tool-event', { ...tool, phase });
    expect(activeHeaderStatuses()).toEqual(['controlNote']);
    expect(document.getElementById('controlNote').textContent).toBe('已停止');
    expect(isVisible(document.getElementById('toolProgress'))).toBe(false);
    expect(isVisible(document.getElementById('toolStatus'))).toBe(false);
    expect(dom.send.getAttribute('aria-label')).toBe('发送消息');
  });

  it.each(['thinking', 'tool'])('clears %s progress on collapse and starts the next reply without stale state', async (state) => {
    await beginReply();
    const tool = { tool_name: 'create_reminder', label: '设置提醒', kind: 'utility', internal_call_id: 'collapsed-reminder' };
    if (state === 'tool') await emit('bubble-tool-event', { ...tool, phase: 'planned' });
    dom.collapse.click();
    await vi.advanceTimersByTimeAsync(220);
    expect(activeHeaderStatuses()).toEqual([]);
    await emit('bubble-tool-event', { ...tool, phase: 'planned' });
    expect(activeHeaderStatuses()).toEqual([]);
    await emit('bubble-end', { text: '上一轮在后台结束。' });
    await openChat();
    expect(activeHeaderStatuses()).toEqual([]);

    typeDraft('开始下一轮');
    pressEnter();
    await flushPromises();
    await emit('bubble-start');
    expect(activeHeaderStatuses()).toEqual(['thinking']);
    expect(isVisible(document.getElementById('toolStatus'))).toBe(false);
    await emit('bubble-tool-event', { ...tool, internal_call_id: 'next-reminder', phase: 'planned' });
    expect(activeHeaderStatuses()).toEqual(['toolProgress']);
    await emit('bubble-end', { text: '下一轮也完成了。' });
    expect(activeHeaderStatuses()).toEqual([]);
    expect(isVisible(document.getElementById('toolStatus'))).toBe(false);
  });

  it('does not restart or disable a stream that starts before the send acknowledgement', async () => {
    const acknowledgement = deferred();
    api.submit.mockImplementationOnce(() => acknowledgement.promise);
    await openChat();
    api.consume.mockResolvedValue('已经开始回复了。');
    typeDraft('请回答');
    dom.send.click();
    await flushPromises();
    await emit('bubble-start');
    expect(dom.body.textContent).toContain('已经开始回复了');
    expect(dom.send.getAttribute('aria-label')).toBe('停止回复');
    expect(dom.send.textContent).toContain('停止');
    expect(dom.send.disabled).toBe(false);
    const readsAfterStart = api.consume.mock.calls.length;

    acknowledgement.resolve();
    await flushPromises();
    expect(api.consume.mock.calls.length).toBe(readsAfterStart);
    expect(dom.body.textContent).toContain('已经开始回复了');
    expect(submittedTexts()).toEqual(['请回答']);
  });

  it('does not revive a stopped reply when its send acknowledgement arrives late', async () => {
    const acknowledgement = deferred();
    api.submit.mockImplementationOnce(() => acknowledgement.promise);
    await openChat();
    typeDraft('这轮先停下');
    dom.send.click();
    await flushPromises();
    await emit('bubble-start');
    dom.send.click();
    await flushPromises();
    expect(invoke.mock.calls.filter(([command]) => command === 'cmd_cancel_chat')).toHaveLength(1);
    const readsAfterStop = api.consume.mock.calls.length;

    acknowledgement.resolve();
    await flushPromises();
    expect(api.consume.mock.calls.length).toBe(readsAfterStop);
    expect(dom.send.getAttribute('aria-label')).toBe('发送消息');
    expect(submittedTexts()).toEqual(['这轮先停下']);
  });

  it('does not restart a reply that completely finishes before the send acknowledgement', async () => {
    const acknowledgement = deferred();
    api.submit.mockImplementationOnce(() => acknowledgement.promise);
    await openChat();
    api.consume.mockResolvedValue('提醒已设置，五分钟后叫你。');
    typeDraft('五分钟后提醒我');
    dom.send.click();
    await flushPromises();
    await emit('bubble-start');
    await emit('bubble-end');
    expect(dom.body.textContent).toContain('提醒已设置，五分钟后叫你');
    expect(dom.content.classList.contains('streaming')).toBe(false);
    const completedReads = api.consume.mock.calls.length;

    acknowledgement.resolve();
    await flushPromises();
    await vi.advanceTimersByTimeAsync(400);
    expect(api.consume.mock.calls.length).toBe(completedReads);
    expect(dom.send.getAttribute('aria-label')).toBe('发送消息');
    expect(dom.content.classList.contains('streaming')).toBe(false);
    expect(submittedTexts()).toEqual(['五分钟后提醒我']);
  });

  it('does not poll a cold null response and still starts on an explicit bubble-start', async () => {
    await mountBubble(null);
    expect(api.consume).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(600);
    expect(api.consume).toHaveBeenCalledTimes(1);
    expect(document.body.classList.contains('hidden')).toBe(true);
    expect(document.getElementById('thinking').style.display).toBe('none');
    expect(dom.content.classList.contains('streaming')).toBe(false);

    api.consume.mockResolvedValue('收到明确的生成开始，再显示回复。');
    await emit('bubble-start');
    expect(api.consume).toHaveBeenCalledTimes(2);
    expect(dom.body.textContent).toContain('收到明确的生成开始');
    expect(document.body.classList.contains('hidden')).toBe(false);
    await vi.advanceTimersByTimeAsync(120);
    expect(api.consume).toHaveBeenCalledTimes(3);
  });

  it('restores the conversation and draft after a passive notice is shown while collapsed', async () => {
    await openChat();
    api.consume.mockResolvedValue('猫刚才认真回答的那句话。');
    typeDraft('刚才认真聊的话题');
    pressEnter();
    await flushPromises();
    await emit('bubble-end');
    typeDraft('我还没发出去的追问');
    dom.collapse.click();
    await vi.advanceTimersByTimeAsync(220);

    api.consume.mockResolvedValue('这是一条普通短通知。');
    window.__bubble_onShow();
    await flushPromises();
    expect(dom.body.textContent).toContain('这是一条普通短通知');
    expect(isVisible(dom.inputRow)).toBe(false);
    await openChat();

    expect(dom.body.textContent).toContain('猫刚才认真回答的那句话');
    expect(document.getElementById('userMessage').textContent).toBe('刚才认真聊的话题');
    expect(dom.input.value).toBe('我还没发出去的追问');
    expect(dom.content.textContent).not.toContain('这是一条普通短通知');
    expect(submittedTexts()).toEqual(['刚才认真聊的话题']);
  });

  it('restores an immutable end snapshot after the backend cache has been replaced by a notice', async () => {
    await openChat();
    api.consume.mockResolvedValue('回复还在生成，先收到这一部分。');
    typeDraft('请完整回答这个问题');
    pressEnter();
    await flushPromises();
    await emit('bubble-start');
    typeDraft('等我回来后继续发这一句');
    dom.collapse.click();
    await vi.advanceTimersByTimeAsync(220);

    // 后端已经完成并释放生成保护，普通通知先覆盖了共享正文。
    api.consume.mockResolvedValue('后来出现的普通短通知。');
    const readsBeforeEnd = api.consume.mock.calls.length;
    await emit('bubble-end', { text: '这是结束事件保存的真正完整回复。' });
    expect(api.consume.mock.calls.length).toBe(readsBeforeEnd);
    expect(document.body.classList.contains('hidden')).toBe(true);
    window.__bubble_onShow();
    await flushPromises();
    expect(dom.body.textContent).toContain('后来出现的普通短通知');

    await openChat();
    expect(dom.body.textContent).toContain('这是结束事件保存的真正完整回复');
    expect(dom.content.textContent).not.toContain('后来出现的普通短通知');
    expect(document.getElementById('conversationHistory').textContent).not.toContain('后来出现的普通短通知');
    expect(dom.input.value).toBe('等我回来后继续发这一句');
    expect(document.getElementById('userMessage').textContent).toBe('请完整回答这个问题');
  });

  it('treats an empty end snapshot as authoritative instead of reviving earlier text', async () => {
    await openChat();
    api.consume.mockResolvedValue('这是之前轮询到的临时正文。');
    typeDraft('这次结束结果为空');
    pressEnter();
    await flushPromises();
    await emit('bubble-start');
    expect(dom.body.textContent).toContain('这是之前轮询到的临时正文');
    const readsBeforeEnd = api.consume.mock.calls.length;

    await emit('bubble-end', { text: '' });
    expect(api.consume.mock.calls.length).toBe(readsBeforeEnd);
    expect(dom.content.classList.contains('streaming')).toBe(false);
    expect(dom.body.textContent).toBe('');
    expect(dom.send.getAttribute('aria-label')).toBe('发送消息');
    dom.collapse.click();
    await vi.advanceTimersByTimeAsync(220);
    await openChat();
    expect(dom.body.textContent).not.toContain('这是之前轮询到的临时正文');
  });

  it('ignores the old end snapshot between a new send acknowledgement and its start event', async () => {
    await openChat();
    api.consume.mockResolvedValue('上一轮停止前已经显示的部分。');
    typeDraft('先回答旧问题');
    pressEnter();
    await flushPromises();
    await emit('bubble-start');
    dom.send.click();
    await flushPromises();
    expect(invoke.mock.calls.filter(([command]) => command === 'cmd_cancel_chat')).toHaveLength(1);

    const nextAcknowledgement = deferred();
    api.submit.mockImplementationOnce(() => nextAcknowledgement.promise);
    api.consume.mockResolvedValue('');
    typeDraft('现在回答新问题');
    pressEnter();
    await flushPromises();
    nextAcknowledgement.resolve();
    await flushPromises();
    const history = document.getElementById('conversationHistory');
    const historyBeforeOldEnd = history.textContent;
    const readsBeforeOldEnd = api.consume.mock.calls.length;

    await emit('bubble-end', { text: '旧问题迟到的最终结束文字。' });
    expect(api.consume.mock.calls.length).toBe(readsBeforeOldEnd);
    expect(dom.send.getAttribute('aria-label')).toBe('停止回复');
    expect(dom.send.disabled).toBe(false);
    expect(document.getElementById('userMessage').textContent).toBe('现在回答新问题');
    expect(history.textContent).toBe(historyBeforeOldEnd);
    expect(dom.content.textContent).not.toContain('旧问题迟到的最终结束文字');

    await emit('bubble-cancelled');
    expect(dom.send.getAttribute('aria-label')).toBe('停止回复');
    expect(dom.send.disabled).toBe(false);
    expect(document.getElementById('userMessage').textContent).toBe('现在回答新问题');
    expect(history.textContent).toBe(historyBeforeOldEnd);

    api.consume.mockResolvedValue('这是新问题正在生成的回复。');
    await emit('bubble-start');
    await vi.advanceTimersByTimeAsync(120);
    await emit('bubble-end', { text: '这是新问题真正完成的回复。' });
    expect(dom.body.textContent).toContain('这是新问题真正完成的回复');
    expect(dom.content.textContent).not.toContain('旧问题迟到的最终结束文字');
    expect(document.getElementById('userMessage').textContent).toBe('现在回答新问题');
    expect(dom.send.getAttribute('aria-label')).toBe('发送消息');
    expect(submittedTexts()).toEqual(['先回答旧问题', '现在回答新问题']);
  });

  it('retains two previous rounds across consecutive turns without duplicating the current reply', async () => {
    await openChat();
    for (let round = 1; round <= 4; round++) {
      const reply = `这是第 ${round} 轮的回答。`;
      api.consume.mockResolvedValue(reply);
      typeDraft(`这是第 ${round} 轮的问题。`);
      pressEnter();
      await flushPromises();
      await emit('bubble-start');
      await emit('bubble-end');

      expect(dom.content.textContent.split(reply)).toHaveLength(2);
      const history = document.getElementById('conversationHistory');
      expect(history, '前两轮对话需要保留在生产页面里').toBeTruthy();
      expect(history.textContent).not.toContain('我在这里。');
      for (let prior = Math.max(1, round - 2); prior < round; prior++) {
        expect(history.textContent).toContain(`这是第 ${prior} 轮的问题。`);
        expect(history.textContent).toContain(`这是第 ${prior} 轮的回答。`);
      }
      expect(history.textContent).not.toContain(reply);
      if (round === 4) {
        expect(history.textContent).not.toContain('这是第 1 轮的问题。');
        expect(history.textContent).not.toContain('这是第 1 轮的回答。');
      }
    }
    expect(submittedTexts()).toHaveLength(4);
    expect(localStorage.length).toBe(0);
  });

  it('keeps the active chat size stable across short and long replies', async () => {
    await openChat();
    await vi.advanceTimersByTimeAsync(200);
    const initialSize = {
      width: document.body.style.width,
      height: document.body.style.minHeight,
    };
    expect(initialSize.width).not.toBe('');
    expect(initialSize.height).not.toBe('');
    currentWindow.setSize.mockClear();

    for (const [prompt, reply, height] of [
      ['说一句就好', '我在。', 100],
      ['展开说说', '这是一段需要滚动阅读的回复。'.repeat(100), 3000],
    ]) {
      setScrollSize(dom.content, height, 190);
      api.consume.mockResolvedValue(reply);
      typeDraft(prompt);
      dom.send.click();
      await flushPromises();
      await vi.advanceTimersByTimeAsync(200);
      await emit('bubble-end');
      await vi.advanceTimersByTimeAsync(200);
      expect(dom.body.textContent).toContain(reply);
      expect(document.body.style.width).toBe(initialSize.width);
      expect(document.body.style.minHeight).toBe(initialSize.height);
    }
    expect(currentWindow.setSize).not.toHaveBeenCalled();
  });

  it('automatically hides a passive notice while leaving active chat under user control', async () => {
    expect(document.body.classList.contains('notice')).toBe(true);
    expect(document.body.classList.contains('hidden')).toBe(false);
    expect(isVisible(dom.inputRow)).toBe(false);
    await vi.advanceTimersByTimeAsync(14900);
    expect(document.body.classList.contains('hidden')).toBe(false);
    await vi.advanceTimersByTimeAsync(300);
    expect(document.body.classList.contains('hidden')).toBe(true);
    expect(invoke.mock.calls.filter(([command]) => command === 'cmd_hide_bubble')).toHaveLength(1);
    expect(invoke.mock.calls.filter(([command]) => command === 'cmd_cancel_chat')).toHaveLength(0);
  });

  it.each([
    ['composition events', { composition: true }],
    ['KeyboardEvent.isComposing', { isComposing: true }],
    ['IME keyCode 229', { keyCode: 229 }],
  ])('does not submit Enter during %s', async (_name, options) => {
    await openChat();
    typeDraft('输入中文');
    if (options.composition) dom.input.dispatchEvent(new CompositionEvent('compositionstart'));
    const event = pressEnter(options);
    await flushPromises();
    expect(submittedTexts()).toEqual([]);
    expect(dom.input.value).toBe('输入中文');
    expect(event.defaultPrevented).toBe(false);
    if (options.composition) dom.input.dispatchEvent(new CompositionEvent('compositionend'));
  });

  it('allows Shift+Enter and submits a multiline textarea with plain Enter', async () => {
    await openChat();
    expect(dom.input.tagName).toBe('TEXTAREA');
    typeDraft('第一行\n第二行');
    const newline = pressEnter({ shiftKey: true });
    await flushPromises();
    expect(newline.defaultPrevented).toBe(false);
    expect(submittedTexts()).toEqual([]);
    expect(dom.input.value).toBe('第一行\n第二行');

    pressEnter();
    await flushPromises();
    expect(submittedTexts()).toEqual(['第一行\n第二行']);
  });

  it('preserves the reader position when streaming completes after an upward scroll', async () => {
    await openChat();
    setScrollSize(dom.content);
    api.consume.mockResolvedValue('先写一段正在生成的回复。');
    typeDraft('详细说说');
    dom.send.click();
    await flushPromises();
    await vi.advanceTimersByTimeAsync(80);
    dom.content.scrollTop = 960;
    dom.body.dispatchEvent(new WheelEvent('wheel', {
      deltaY: -180, bubbles: true, cancelable: true,
    }));
    const readerPosition = dom.content.scrollTop;

    api.consume.mockResolvedValue('完整回复已经结束，仍然保留正在阅读的位置。');
    await emit('bubble-end');
    await vi.advanceTimersByTimeAsync(80);
    expect(dom.body.textContent).toContain('完整回复已经结束');
    expect(dom.content.scrollTop).toBe(readerPosition);
  });

  it.each(['poll result', 'end already in flight', 'end received after collapse'])(
    'keeps the bubble collapsed when a late %s arrives', async (lateEvent) => {
      await openChat();
      const pending = deferred();
      api.consume.mockImplementation(() => pending.promise);
      typeDraft('请慢慢回答');
      dom.send.click();
      await flushPromises();
      if (lateEvent === 'end already in flight') {
        handlers.get('bubble-end')({ payload: undefined });
        await flushPromises();
      }

      dom.collapse.click();
      await vi.advanceTimersByTimeAsync(220);
      if (lateEvent === 'end received after collapse') {
        handlers.get('bubble-end')({ payload: undefined });
        await flushPromises();
      }
      pending.resolve('这条迟到的回复不能重新打开窗口。');
      await flushPromises();
      await vi.advanceTimersByTimeAsync(80);

      expect(document.body.classList.contains('hidden')).toBe(true);
      expect(document.body.classList.contains('show')).toBe(false);
      expect(isVisible(dom.inputRow)).toBe(false);
      expect(invoke.mock.calls.filter(([command]) => command === 'cmd_hide_bubble')).toHaveLength(1);
    },
  );

  it('ignores a previous reply poll after a new conversation starts', async () => {
    await openChat();
    const previous = deferred();
    api.consume.mockImplementationOnce(() => previous.promise);
    typeDraft('上一轮对话');
    dom.send.click();
    await flushPromises();
    await emit('bubble-start');
    await emit('bubble-cancelled');
    dom.collapse.click();
    await vi.advanceTimersByTimeAsync(220);

    await openChat();
    api.consume.mockResolvedValue('这是新一轮正在生成的回复。');
    typeDraft('新一轮对话');
    dom.send.click();
    await flushPromises();
    expect(dom.body.textContent).toContain('这是新一轮正在生成的回复');

    previous.resolve('上一轮已经过期的回复。');
    await flushPromises();
    expect(dom.body.textContent).toContain('这是新一轮正在生成的回复');
    expect(dom.body.textContent).not.toContain('上一轮已经过期的回复');
  });

  it('applies a bubbled vertical wheel event exactly once', async () => {
    await openChat();
    setScrollSize(dom.content);
    dom.content.scrollTop = 300;
    const event = new WheelEvent('wheel', { deltaY: 120, bubbles: true, cancelable: true });
    dom.body.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(dom.content.scrollTop).toBe(420);
  });

  it.each(['pre', 'table'])('lets horizontal wheel events reach a rendered %s', async (selector) => {
    await openChat();
    typeDraft('给我一个代码和表格示例');
    dom.send.click();
    await flushPromises();
    api.consume.mockResolvedValue('```js\nconst message = "一段很长的代码";\n```\n\n| 名称 | 说明 |\n| --- | --- |\n| 示例 | 一段较长的表格内容 |');
    await emit('bubble-end');
    await vi.advanceTimersByTimeAsync(80);
    const target = dom.body.querySelector(selector);
    expect(target).toBeTruthy();
    Object.defineProperties(target, {
      scrollWidth: { value: 900, configurable: true },
      clientWidth: { value: 180, configurable: true },
    });
    setScrollSize(dom.content);
    dom.content.scrollTop = 300;
    const event = new WheelEvent('wheel', {
      deltaX: 120, deltaY: 0, bubbles: true, cancelable: true,
    });
    target.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(dom.content.scrollTop).toBe(300);
  });

  it('preserves Shift+wheel for horizontal scrolling inside code', async () => {
    await openChat();
    typeDraft('给我一个代码示例');
    dom.send.click();
    await flushPromises();
    api.consume.mockResolvedValue('```js\nconst message = "一段很长的代码";\n```');
    await emit('bubble-end');
    await vi.advanceTimersByTimeAsync(80);
    setScrollSize(dom.content);
    dom.content.scrollTop = 300;
    const code = dom.body.querySelector('pre');
    Object.defineProperties(code, {
      scrollWidth: { value: 900, configurable: true },
      clientWidth: { value: 180, configurable: true },
    });
    const event = new WheelEvent('wheel', {
      deltaY: 120, shiftKey: true, bubbles: true, cancelable: true,
    });
    code.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(dom.content.scrollTop).toBe(300);
  });
});
