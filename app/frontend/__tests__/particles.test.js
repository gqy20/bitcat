import { describe, expect, it } from 'vitest';
import { JSDOM } from 'jsdom';
import fs from 'node:fs';
import { resolve } from 'node:path';

// 金币粒子的夹取语义：普通掉落单次上限 8，下班结算喷泉放宽到 16，
// 保证调度器发的 15 枚仪式不被前端夹半。
const script = fs.readFileSync(resolve(process.cwd(), 'js/particles.js'), 'utf8');

function loadParticles() {
  const dom = new JSDOM('<!doctype html><body><div id="particles"></div></body>', {
    url: 'http://localhost/pet.html',
    runScripts: 'outside-only',
  });
  dom.window.eval(script);
  return dom;
}

const settle = (ms) => new Promise(r => setTimeout(r, ms));

describe('coin drop clamping', () => {
  it('caps a normal drop at 8 coins', async () => {
    const dom = loadParticles();
    dom.window.Particles.dropCoins(15);
    await settle(8 * 90 + 50);
    expect(dom.window.document.querySelectorAll('.p-coin').length).toBe(8);
    dom.window.close();
  });

  it('lets the settlement fountain through up to 16', async () => {
    const dom = loadParticles();
    dom.window.Particles.dropCoins(15, true);
    await settle(15 * 60 + 50);
    const coins = dom.window.document.querySelectorAll('.p-coin');
    expect(coins.length).toBe(15);
    // 像素币是 CSS 画的方块，不带文字 glyph。
    expect(coins[0].textContent).toBe('');
    dom.window.close();
  });

  it('always drops at least one coin', async () => {
    const dom = loadParticles();
    dom.window.Particles.dropCoins(0);
    await settle(150);
    expect(dom.window.document.querySelectorAll('.p-coin').length).toBe(1);
    dom.window.close();
  });
});
