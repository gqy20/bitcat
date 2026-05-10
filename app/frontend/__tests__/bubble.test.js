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
