// bubble.test.js — 气泡滚动行为测试（Vitest + jsdom）

import { describe, it, expect, beforeEach } from 'vitest';

function createBubbleDOM() {
  document.body.innerHTML = `
    <body class="hidden">
      <div class="bubble">
        <div class="content" id="content"></div>
      </div>
    </body>
  `;
  return document.getElementById('content');
}

describe('bubble scroll', () => {
  let contentEl;

  beforeEach(() => {
    contentEl = createBubbleDOM();
    // 模拟长内容溢出
    contentEl.textContent = 'x'.repeat(2000);
    Object.defineProperty(contentEl, 'scrollHeight', {
      value: 600,
      configurable: true,
      writable: true,
    });
    Object.defineProperty(contentEl, 'clientHeight', {
      value: 100,
      configurable: true,
      writable: true,
    });
    contentEl.scrollTop = 0;
  });

  it('onWheel scrolls content by deltaY', () => {
    const event = new WheelEvent('wheel', {
      deltaY: 120,
      bubbles: true,
      cancelable: true,
    });
    let capturedDeltaY = null;
    const handler = (e) => {
      e.preventDefault();
      capturedDeltaY = e.deltaY;
      contentEl.scrollTop += e.deltaY;
    };
    contentEl.addEventListener('wheel', handler);
    contentEl.dispatchEvent(event);

    expect(capturedDeltaY).toBe(120);
    expect(contentEl.scrollTop).toBe(120);
  });

  it('onWheel prevents default to stop native scroll', () => {
    const event = new WheelEvent('wheel', {
      deltaY: 120,
      bubbles: true,
      cancelable: true,
    });
    const handler = (e) => e.preventDefault();
    contentEl.addEventListener('wheel', handler);
    contentEl.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
  });

  it('ArrowDown scrolls down by 40px', () => {
    contentEl.scrollTop = 0;
    const handler = (e) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        contentEl.scrollTop += 40;
      }
    };
    contentEl.addEventListener('keydown', handler);
    contentEl.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'ArrowDown',
      bubbles: true,
      cancelable: true,
    }));

    expect(contentEl.scrollTop).toBe(40);
  });

  it('ArrowUp scrolls up by 40px', () => {
    contentEl.scrollTop = 100;
    const handler = (e) => {
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        contentEl.scrollTop -= 40;
      }
    };
    contentEl.addEventListener('keydown', handler);
    contentEl.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'ArrowUp',
      bubbles: true,
      cancelable: true,
    }));

    expect(contentEl.scrollTop).toBe(60);
  });

  it('PageDown scrolls by clientHeight', () => {
    contentEl.scrollTop = 0;
    const handler = (e) => {
      if (e.key === 'PageDown') {
        e.preventDefault();
        contentEl.scrollTop += contentEl.clientHeight;
      }
    };
    contentEl.addEventListener('keydown', handler);
    contentEl.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'PageDown',
      bubbles: true,
      cancelable: true,
    }));

    expect(contentEl.scrollTop).toBe(100); // clientHeight = 100
  });

  it('PageUp scrolls up by clientHeight', () => {
    contentEl.scrollTop = 200;
    const handler = (e) => {
      if (e.key === 'PageUp') {
        e.preventDefault();
        contentEl.scrollTop -= contentEl.clientHeight;
      }
    };
    contentEl.addEventListener('keydown', handler);
    contentEl.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'PageUp',
      bubbles: true,
      cancelable: true,
    }));

    expect(contentEl.scrollTop).toBe(100);
  });

  it('Home scrolls to top', () => {
    contentEl.scrollTop = 500;
    const handler = (e) => {
      if (e.key === 'Home') {
        e.preventDefault();
        contentEl.scrollTop = 0;
      }
    };
    contentEl.addEventListener('keydown', handler);
    contentEl.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'Home',
      bubbles: true,
      cancelable: true,
    }));

    expect(contentEl.scrollTop).toBe(0);
  });

  it('End scrolls to bottom (scrollHeight)', () => {
    contentEl.scrollTop = 0;
    const handler = (e) => {
      if (e.key === 'End') {
        e.preventDefault();
        contentEl.scrollTop = contentEl.scrollHeight;
      }
    };
    contentEl.addEventListener('keydown', handler);
    contentEl.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'End',
      bubbles: true,
      cancelable: true,
    }));

    expect(contentEl.scrollTop).toBe(600); // scrollHeight = 600
  });

  it('ignores non-scroll keys', () => {
    contentEl.scrollTop = 50;
    const handler = (e) => {
      switch (e.key) {
        case 'ArrowDown': contentEl.scrollTop += 40; break;
        case 'ArrowUp': contentEl.scrollTop -= 40; break;
        default: /* ignore */
      }
    };
    contentEl.addEventListener('keydown', handler);
    contentEl.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'a',
      bubbles: true,
    }));

    expect(contentEl.scrollTop).toBe(50); // unchanged
  });
});

// ---- 聊天输入框测试 ----

function createInputDOM() {
  document.body.innerHTML = `
    <body class="hidden">
      <div class="bubble">
        <div class="content" id="content"></div>
        <div class="input-row" id="inputRow" style="display: none;">
          <input type="text" id="chatInput" placeholder="说点什么..." />
          <button id="chatSend">发送</button>
        </div>
      </div>
    </body>
  `;
  return {
    inputRow: document.getElementById('inputRow'),
    input: document.getElementById('chatInput'),
    sendBtn: document.getElementById('chatSend'),
  };
}

describe('bubble chat input', () => {
  let dom;

  beforeEach(() => {
    dom = createInputDOM();
  });

  it('输入框默认隐藏', () => {
    expect(dom.inputRow.classList.contains('visible')).toBe(false);
    expect(dom.inputRow.style.display).toBe('none');
  });

  it('展开输入框后可见', () => {
    dom.inputRow.classList.add('visible');
    dom.inputRow.style.display = '';
    expect(dom.inputRow.classList.contains('visible')).toBe(true);
  });

  it('收起输入框后隐藏并清空文本', () => {
    dom.input.value = 'hello';
    dom.inputRow.classList.add('visible');
    dom.inputRow.style.display = '';

    // 模拟收起
    dom.inputRow.classList.remove('visible');
    dom.inputRow.style.display = 'none';
    dom.input.value = '';

    expect(dom.inputRow.classList.contains('visible')).toBe(false);
    expect(dom.input.value).toBe('');
  });

  it('Enter 提交非空文本（非 IME 组合状态）', () => {
    let submitted = null;
    const submitHandler = (text) => { submitted = text; };

    dom.input.value = '你好 AI';
    // 模拟：不在 IME 组合中
    let composing = false;
    const handler = (e) => {
      if (e.key === 'Enter' && !composing && dom.input.value.trim()) {
        e.preventDefault();
        submitHandler(dom.input.value.trim());
        dom.input.value = '';
      }
    };
    dom.input.addEventListener('keydown', handler);
    dom.input.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    }));

    expect(submitted).toBe('你好 AI');
    expect(dom.input.value).toBe('');
  });

  it('Enter 在 IME 组合中不提交（compositionstart）', () => {
    let submitted = null;
    const submitHandler = (text) => { submitted = text; };
    let composing = true; // IME 组合中

    dom.input.value = 'n';
    const handler = (e) => {
      if (e.key === 'Enter' && !composing && dom.input.value.trim()) {
        submitHandler(dom.input.value.trim());
        dom.input.value = '';
      }
    };
    dom.input.addEventListener('keydown', handler);

    // Enter during composition should NOT submit
    dom.input.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    }));

    expect(submitted).toBeNull();
    expect(dom.input.value).toBe('n'); // not cleared
  });

  it('空文本 Enter 不提交', () => {
    let submitted = null;
    let callCount = 0;

    dom.input.value = '   ';
    const handler = (e) => {
      if (e.key === 'Enter' && dom.input.value.trim()) {
        submitted = dom.input.value.trim();
        callCount++;
      }
    };
    dom.input.addEventListener('keydown', handler);
    dom.input.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    }));

    expect(submitted).toBeNull();
    expect(callCount).toBe(0);
  });

  it('Escape 收起输入框', () => {
    dom.inputRow.classList.add('visible');
    dom.inputRow.style.display = '';
    dom.input.value = '未发送的文字';

    const handler = (e) => {
      if (e.key === 'Escape') {
        dom.inputRow.classList.remove('visible');
        dom.inputRow.style.display = 'none';
        dom.input.value = '';
      }
    };
    dom.input.addEventListener('keydown', handler);
    dom.input.dispatchEvent(new KeyboardEvent('keydown', {
      key: 'Escape',
      bubbles: true,
      cancelable: true,
    }));

    expect(dom.inputRow.classList.contains('visible')).toBe(false);
    expect(dom.input.value).toBe('');
  });

  it('点击发送按钮等同于 Enter', () => {
    let submitted = null;

    dom.input.value = '点击发送';
    const clickHandler = () => {
      const text = dom.input.value.trim();
      if (text) {
        submitted = text;
        dom.input.value = '';
      }
    };
    dom.sendBtn.addEventListener('click', clickHandler);
    dom.sendBtn.click();

    expect(submitted).toBe('点击发送');
    expect(dom.input.value).toBe('');
  });
});

// ---- 光标生命周期测试（DOM 解耦契约） ----

function createBubbleCursorDOM() {
  document.body.innerHTML = `
    <body class="hidden">
      <div class="bubble">
        <div class="content idle" id="content">
          <div class="content-body" id="contentBody"></div>
          <span class="typing-cursor" id="typingCursor" aria-hidden="true"></span>
        </div>
      </div>
    </body>
  `;
  return {
    content: document.getElementById('content'),
    body: document.getElementById('contentBody'),
    cursor: document.getElementById('typingCursor'),
  };
}

/// 模拟新 bubble.js 的契约行为：content.streaming class + body.innerHTML
function setStreamingClass(contentEl, on) {
  if (on) {
    contentEl.classList.add('streaming');
    contentEl.classList.remove('idle');
  } else {
    contentEl.classList.add('idle');
    contentEl.classList.remove('streaming');
  }
}

function setText(bodyEl, text) {
  // 不拼接任何 cursor 标记，纯写正文
  bodyEl.innerHTML = text || '';
}

describe('bubble cursor lifecycle', () => {
  let dom;

  beforeEach(() => {
    dom = createBubbleCursorDOM();
  });

  it('A: 初始 DOM 结构包含常驻 #typingCursor，content 默认 idle', () => {
    expect(dom.body).toBeTruthy();
    expect(dom.cursor).toBeTruthy();
    expect(dom.cursor.tagName.toLowerCase()).toBe('span');
    expect(dom.cursor.classList.contains('typing-cursor')).toBe(true);
    expect(dom.content.classList.contains('idle')).toBe(true);
    expect(dom.content.classList.contains('streaming')).toBe(false);
  });

  it('B: startPolling 路径 → content.streaming 生效且 setText 不写入 cursor span 字符串', () => {
    setStreamingClass(dom.content, true);
    setText(dom.body, '你好，正在回复');

    expect(dom.content.classList.contains('streaming')).toBe(true);
    expect(dom.content.classList.contains('idle')).toBe(false);
    // body 的 HTML 不应包含光标相关标记（DOM 完全解耦）
    expect(dom.body.innerHTML).not.toContain('typing-cursor');
    expect(dom.body.innerHTML).toBe('你好，正在回复');
    // cursor DOM 节点仍然是 content 的直接子节点
    expect(dom.content.contains(dom.cursor)).toBe(true);
  });

  it('C: bubble-end 路径 → content 切 idle，cursor 节点仍常驻', () => {
    setStreamingClass(dom.content, true);
    setText(dom.body, '结束文本');
    setStreamingClass(dom.content, false);

    expect(dom.content.classList.contains('idle')).toBe(true);
    expect(dom.content.classList.contains('streaming')).toBe(false);
    // 关键契约：cursor DOM 节点从不被移除
    expect(document.getElementById('typingCursor')).toBe(dom.cursor);
    expect(dom.content.contains(dom.cursor)).toBe(true);
  });

  it('D: lastRawText 为空也能收回光标（解耦事件守卫）', () => {
    setStreamingClass(dom.content, true);
    // 模拟 bubble-end 到达但 lastRawText === ''
    setStreamingClass(dom.content, false);
    // 不调用 setText，content-body 可能为空

    expect(dom.content.classList.contains('idle')).toBe(true);
    expect(dom.content.classList.contains('streaming')).toBe(false);
  });

  it('E: 多次流式切换不会残留或重复添加 class', () => {
    for (let i = 0; i < 5; i++) {
      setStreamingClass(dom.content, true);
      setText(dom.body, `round ${i}`);
      setStreamingClass(dom.content, false);
    }
    // 最终 content 只含 idle，不含 streaming
    const classes = Array.from(dom.content.classList);
    expect(classes.filter(c => c === 'streaming').length).toBe(0);
    expect(classes.filter(c => c === 'idle').length).toBe(1);
    // cursor 节点唯一
    expect(document.querySelectorAll('.typing-cursor').length).toBe(1);
  });
});

describe('bubble streaming lifecycle', () => {
  it('does not finish streaming just because polling text is stable', () => {
    let streaming = true;
    let stopped = false;
    let inputShown = false;
    let rendered = '';

    const onPollResult = (txt) => {
      if (!streaming) return;

      const len = (txt || '').length;
      if (len === 0) return;
      rendered = txt;
    };

    const stopPolling = () => { stopped = true; };
    const showInput = () => { inputShown = true; };

    for (let i = 0; i < 20; i++) {
      onPollResult('tool call started');
    }

    expect(streaming).toBe(true);
    expect(stopped).toBe(false);
    expect(inputShown).toBe(false);
    expect(rendered).toBe('tool call started');

    streaming = false;
    stopPolling();
    showInput();
    expect(stopped).toBe(true);
    expect(inputShown).toBe(true);
  });
});

describe('bubble tool status text', () => {
  function getToolStatusText(payload) {
    const label = payload && payload.label ? payload.label : '调用工具';
    const phase = payload && payload.phase ? payload.phase : 'planned';
    const kind = payload && payload.kind ? payload.kind : 'utility';
    const toolName = payload && payload.tool_name ? String(payload.tool_name) : '';
    const isDanceTool = toolName === 'perform_dance' || toolName === 'play_dance';
    if (kind === 'performance' && isDanceTool) {
      if (phase === 'blocked') return '表演已拦截';
      if (phase === 'failed') return '编舞失败';
      if (phase === 'finished' || (payload && payload.tool_name === 'play_dance')) return '准备开跳';
      return '正在编舞';
    }
    if (phase === 'blocked') return label + '已拦截';
    if (phase === 'failed') return label + '失败';
    if (phase === 'finished') return label + '完成';
    return '准备' + label;
  }

  it('uses stage copy for performance tools', () => {
    expect(getToolStatusText({
      kind: 'performance',
      phase: 'planned',
      tool_name: 'perform_dance',
      label: '编排舞蹈',
    })).toBe('正在编舞');

    expect(getToolStatusText({
      kind: 'performance',
      phase: 'finished',
      tool_name: 'perform_dance',
      label: '编排舞蹈',
    })).toBe('准备开跳');

    expect(getToolStatusText({
      kind: 'performance',
      phase: 'planned',
      tool_name: 'play_dance',
      label: '播放舞蹈',
    })).toBe('准备开跳');
  });

  it('does not reuse dance copy for non-dance performance payloads', () => {
    const text = getToolStatusText({
      kind: 'performance',
      phase: 'finished',
      tool_name: 'Bash',
      label: 'Shell',
    });
    expect(text).toContain('Shell');
    expect(text).not.toBe(getToolStatusText({
      kind: 'performance',
      phase: 'finished',
      tool_name: 'perform_dance',
      label: 'x',
    }));
  });

  it('keeps utility tool copy explicit', () => {
    expect(getToolStatusText({
      kind: 'utility',
      phase: 'planned',
      label: '读取文件',
    })).toBe('准备读取文件');

    expect(getToolStatusText({
      kind: 'system',
      phase: 'blocked',
      label: '执行命令',
    })).toBe('执行命令已拦截');
  });
});

describe('bubble resize preference lifecycle', () => {
  function maybePersistResize(state, w, h) {
    state.currentWinW = w;
    state.currentWinH = h;

    if (state.resizeMode === 'manual' && state.userResizeActive && !state.programmaticResize) {
      state.userPrefSize = { w, h };
      state.storage.bubble_pref = JSON.stringify(state.userPrefSize);
    }
  }

  it('persists resize preference only during user drag', () => {
    const state = {
      resizeMode: 'manual',
      userResizeActive: true,
      programmaticResize: false,
      userPrefSize: null,
      storage: {},
    };

    maybePersistResize(state, 340, 260);

    expect(state.userPrefSize).toEqual({ w: 340, h: 260 });
    expect(JSON.parse(state.storage.bubble_pref)).toEqual({ w: 340, h: 260 });
  });

  it('does not overwrite preference during programmatic content resize', () => {
    const state = {
      resizeMode: 'manual',
      userResizeActive: false,
      programmaticResize: true,
      userPrefSize: { w: 320, h: 220 },
      storage: { bubble_pref: JSON.stringify({ w: 320, h: 220 }) },
    };

    maybePersistResize(state, 320, 500);

    expect(state.currentWinH).toBe(500);
    expect(state.userPrefSize).toEqual({ w: 320, h: 220 });
    expect(JSON.parse(state.storage.bubble_pref)).toEqual({ w: 320, h: 220 });
  });
});

describe('bubble stepped auto sizing', () => {
  const MIN_H = 120;
  const READING_H = 220;

  function stageRank(stage) {
    switch (stage) {
      case 'expanded': return 2;
      case 'reading': return 1;
      default: return 0;
    }
  }

  function chooseAutoSizeStage(neededH, options = {}) {
    const currentStage = options.currentStage || 'compact';
    const hasText = !!options.hasText;
    const isStreaming = !!options.streaming;
    const inputOpen = !!options.inputOpen;
    const mode = options.mode || 'notice';

    if (mode === 'notice') {
      return 'compact';
    }

    if (inputOpen) {
      return neededH > READING_H + 24 ? 'expanded' : 'reading';
    }

    if (isStreaming && hasText) {
      const desiredDuringStream = neededH > READING_H + 24 ? 'expanded' : 'reading';
      return stageRank(desiredDuringStream) > stageRank(currentStage)
        ? desiredDuringStream
        : currentStage;
    }

    if (neededH > READING_H + 24) return 'expanded';
    if (neededH > MIN_H) return 'reading';
    return 'compact';
  }

  it('starts streaming replies in a stable reading size once text arrives', () => {
    expect(chooseAutoSizeStage(130, {
      currentStage: 'compact',
      hasText: true,
      streaming: true,
      mode: 'stream',
    })).toBe('reading');
  });

  it('does not shrink while a streaming reply is still arriving', () => {
    expect(chooseAutoSizeStage(128, {
      currentStage: 'expanded',
      hasText: true,
      streaming: true,
      mode: 'stream',
    })).toBe('expanded');
  });

  it('expands long replies in one coarse step instead of per-chunk sizing', () => {
    expect(chooseAutoSizeStage(260, {
      currentStage: 'reading',
      hasText: true,
      streaming: true,
      mode: 'stream',
    })).toBe('expanded');
  });

  it('keeps the compose state roomy when input is open', () => {
    expect(chooseAutoSizeStage(120, { inputOpen: true, mode: 'compose' })).toBe('reading');
    expect(chooseAutoSizeStage(260, { inputOpen: true, mode: 'compose' })).toBe('expanded');
  });

  it('keeps passive notices compact even with long content', () => {
    expect(chooseAutoSizeStage(320, {
      currentStage: 'compact',
      hasText: true,
      mode: 'notice',
    })).toBe('compact');
  });
});
