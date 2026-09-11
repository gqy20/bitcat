import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function loadPanel() {
  const dom = new JSDOM(
    `<!doctype html>
    <div class="panel">
      <div class="panel-header">
        <span class="title" id="panel-title-main">BitCat</span>
        <span class="title hidden" id="panel-title">游戏库 · B 返回</span>
      </div>
      <div class="grid" id="grid"></div>
    </div>`,
    {
      url: 'http://localhost/panel.html',
      runScripts: 'outside-only',
    },
  );
  const script = readFileSync(resolve(process.cwd(), 'js/panel.js'), 'utf8');
  dom.window.eval(script);
  return dom;
}

const MAIN_ACTIONS = [
  { id: 'invasion', label: '桌面保卫战', icon: '🛡', enabled: true },
  { id: 'gamelib', label: '游戏库', icon: '🎲', enabled: true },
];
const LIBRARY_ACTIONS = [
  { id: 'game', label: '毛线球大作战', icon: '🎮', enabled: true },
  { id: 'memory', label: '翻牌配对', icon: 'M', enabled: true },
  { id: 'catch', label: '接食物', icon: 'C', enabled: true },
];

describe('panel game library', () => {
  let dom;
  let test;

  beforeEach(() => {
    dom = loadPanel();
    test = dom.window.__panelTest;
    test.setActions(MAIN_ACTIONS, LIBRARY_ACTIONS);
  });

  afterEach(() => {
    dom?.window?.close();
  });

  it('main view shows only non-library actions', () => {
    expect(test.currentActions()).toEqual(['invasion', 'gamelib']);
    const cells = dom.window.document.querySelectorAll('.cell');
    expect(cells).toHaveLength(2);
    expect(
      dom.window.document.getElementById('panel-title-main').classList.contains('hidden'),
    ).toBe(false);
  });

  it('game library entry swaps to the secondary view', () => {
    test.showLibrary();
    expect(test.currentActions()).toEqual(['game', 'memory', 'catch']);
    const cells = dom.window.document.querySelectorAll('.cell');
    expect(cells).toHaveLength(3);
    expect(dom.window.document.getElementById('panel-title').classList.contains('hidden')).toBe(
      false,
    );
    expect(
      dom.window.document.getElementById('panel-title-main').classList.contains('hidden'),
    ).toBe(true);
  });

  it('B key returns to main view before closing the panel', async () => {
    test.showLibrary();
    // closePanel 在库视图里应先返回主视图而不是隐藏窗口。
    await test.closePanel();
    expect(test.currentActions()).toEqual(['invasion', 'gamelib']);
  });
});
