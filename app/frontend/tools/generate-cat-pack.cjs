const { existsSync, rmSync, writeFileSync } = require('node:fs');
const { execFileSync } = require('node:child_process');
const path = require('node:path');
const { deflateSync } = require('node:zlib');

const outDir = path.join(__dirname, '..', '__fixtures__', 'pets', 'cat');
const frameWidth = 192;
const frameHeight = 208;
const columns = 8;
const rows = 8;
const frameCount = 64;
const sheetWidth = frameWidth * columns;
const sheetHeight = frameHeight * rows;
const tempPngPath = path.join(outDir, 'spritesheet.tmp.png');
const webpPath = path.join(outDir, 'spritesheet.webp');
const pixels = new Uint8ClampedArray(sheetWidth * sheetHeight * 4);

const rgba = (hex, alpha = 255) => {
  const n = Number.parseInt(hex.replace('#', ''), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, alpha];
};

const C = {
  shadow: rgba('#070812', 72),
  outline: rgba('#3d2730', 245),
  furDark: rgba('#8e5b3d', 255),
  fur: rgba('#d88b52', 255),
  furLight: rgba('#ffd2a4', 255),
  cream: rgba('#fff0d6', 255),
  blush: rgba('#ff9db1', 210),
  eye: rgba('#1b2638', 255),
  cyan: rgba('#76f6e5', 255),
  cyanDim: rgba('#39b8bf', 230),
  green: rgba('#8df09d', 255),
  yellow: rgba('#ffe06b', 255),
  magenta: rgba('#f674d6', 255),
  red: rgba('#ff5972', 255),
  white: rgba('#fff8ee', 255),
  screen: rgba('#132031', 230),
};

function blendPixel(x, y, color) {
  if (x < 0 || y < 0 || x >= sheetWidth || y >= sheetHeight) return;
  const i = (Math.floor(y) * sheetWidth + Math.floor(x)) * 4;
  const a = color[3] / 255;
  const ia = 1 - a;
  pixels[i] = Math.round(color[0] * a + pixels[i] * ia);
  pixels[i + 1] = Math.round(color[1] * a + pixels[i + 1] * ia);
  pixels[i + 2] = Math.round(color[2] * a + pixels[i + 2] * ia);
  pixels[i + 3] = Math.round(255 * (a + (pixels[i + 3] / 255) * ia));
}

function rect(fx, fy, x, y, w, h, color) {
  const ox = fx * frameWidth;
  const oy = fy * frameHeight;
  for (let yy = Math.floor(y); yy < Math.ceil(y + h); yy += 1) {
    for (let xx = Math.floor(x); xx < Math.ceil(x + w); xx += 1) blendPixel(ox + xx, oy + yy, color);
  }
}

function ellipse(fx, fy, cx, cy, rx, ry, color) {
  const ox = fx * frameWidth;
  const oy = fy * frameHeight;
  for (let y = Math.floor(cy - ry); y <= Math.ceil(cy + ry); y += 1) {
    for (let x = Math.floor(cx - rx); x <= Math.ceil(cx + rx); x += 1) {
      const dx = (x + 0.5 - cx) / rx;
      const dy = (y + 0.5 - cy) / ry;
      if (dx * dx + dy * dy <= 1) blendPixel(ox + x, oy + y, color);
    }
  }
}

function line(fx, fy, x0, y0, x1, y1, width, color) {
  const steps = Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0)) * 2;
  for (let i = 0; i <= steps; i += 1) {
    const t = steps === 0 ? 0 : i / steps;
    ellipse(fx, fy, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, width / 2, width / 2, color);
  }
}

function softEllipse(fx, fy, cx, cy, rx, ry, inner, outer) {
  const ox = fx * frameWidth;
  const oy = fy * frameHeight;
  for (let y = Math.floor(cy - ry); y <= Math.ceil(cy + ry); y += 1) {
    for (let x = Math.floor(cx - rx); x <= Math.ceil(cx + rx); x += 1) {
      const dx = (x + 0.5 - cx) / rx;
      const dy = (y + 0.5 - cy) / ry;
      const d = dx * dx + dy * dy;
      if (d <= 1) {
        const t = Math.max(0, Math.min(1, Math.sqrt(d)));
        blendPixel(ox + x, oy + y, [
          Math.round(inner[0] * (1 - t) + outer[0] * t),
          Math.round(inner[1] * (1 - t) + outer[1] * t),
          Math.round(inner[2] * (1 - t) + outer[2] * t),
          Math.round(inner[3] * (1 - t) + outer[3] * t),
        ]);
      }
    }
  }
}

function glow(fx, fy, cx, cy, rx, ry, color) {
  for (let i = 0; i < 5; i += 1) {
    ellipse(fx, fy, cx, cy, rx + i * 4, ry + i * 3, [color[0], color[1], color[2], Math.max(10, 48 - i * 8)]);
  }
  ellipse(fx, fy, cx, cy, rx, ry, color);
}

function triangle(fx, fy, ax, ay, bx, by, cx, cy, color) {
  const ox = fx * frameWidth;
  const oy = fy * frameHeight;
  const minX = Math.floor(Math.min(ax, bx, cx));
  const maxX = Math.ceil(Math.max(ax, bx, cx));
  const minY = Math.floor(Math.min(ay, by, cy));
  const maxY = Math.ceil(Math.max(ay, by, cy));
  const area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      const w0 = ((bx - ax) * (y - ay) - (by - ay) * (x - ax)) / area;
      const w1 = ((cx - bx) * (y - by) - (cy - by) * (x - bx)) / area;
      const w2 = ((ax - cx) * (y - cy) - (ay - cy) * (x - cx)) / area;
      if (w0 >= -0.02 && w1 >= -0.02 && w2 >= -0.02) blendPixel(ox + x, oy + y, color);
    }
  }
}

function roundRect(fx, fy, x, y, w, h, r, color) {
  rect(fx, fy, x + r, y, w - r * 2, h, color);
  rect(fx, fy, x, y + r, r, h - r * 2, color);
  rect(fx, fy, x + w - r, y + r, r, h - r * 2, color);
  ellipse(fx, fy, x + r, y + r, r, r, color);
  ellipse(fx, fy, x + w - r, y + r, r, r, color);
  ellipse(fx, fy, x + r, y + h - r, r, r, color);
  ellipse(fx, fy, x + w - r, y + h - r, r, r, color);
}

function clearFrame(fx, fy) {
  rect(fx, fy, 0, 0, frameWidth, frameHeight, rgba('#000000', 0));
}

function drawSpark(fx, fy, cx, cy, size, color) {
  line(fx, fy, cx, cy - size, cx, cy + size, 4, color);
  line(fx, fy, cx - size, cy, cx + size, cy, 4, color);
  line(fx, fy, cx - size * 0.7, cy - size * 0.7, cx + size * 0.7, cy + size * 0.7, 3, color);
  line(fx, fy, cx + size * 0.7, cy - size * 0.7, cx - size * 0.7, cy + size * 0.7, 3, color);
}

function drawQuestion(fx, fy, cx, cy, color) {
  line(fx, fy, cx - 10, cy - 12, cx, cy - 18, 5, color);
  line(fx, fy, cx, cy - 18, cx + 11, cy - 10, 5, color);
  line(fx, fy, cx + 11, cy - 10, cx + 2, cy + 1, 5, color);
  line(fx, fy, cx + 2, cy + 1, cx + 2, cy + 8, 5, color);
  ellipse(fx, fy, cx + 2, cy + 20, 4, 4, color);
}

function drawWarning(fx, fy, cx, cy, color) {
  line(fx, fy, cx, cy - 18, cx - 18, cy + 15, 5, color);
  line(fx, fy, cx - 18, cy + 15, cx + 18, cy + 15, 5, color);
  line(fx, fy, cx + 18, cy + 15, cx, cy - 18, 5, color);
  rect(fx, fy, cx - 2, cy - 4, 4, 13, color);
  ellipse(fx, fy, cx, cy + 14, 3, 3, color);
}

function drawLens(fx, fy, cx, cy, color) {
  ellipse(fx, fy, cx, cy, 13, 13, [color[0], color[1], color[2], 80]);
  ellipse(fx, fy, cx, cy, 10, 10, [color[0], color[1], color[2], 36]);
  line(fx, fy, cx + 9, cy + 9, cx + 23, cy + 23, 5, color);
}

function drawEyes(fx, fy, cfg, y) {
  if (cfg.mode === 'failed') {
    line(fx, fy, 73, y + 4, 84, y + 15, 5, C.red);
    line(fx, fy, 84, y + 4, 73, y + 15, 5, C.red);
    line(fx, fy, 108, y + 4, 119, y + 15, 5, C.red);
    line(fx, fy, 119, y + 4, 108, y + 15, 5, C.red);
    rect(fx, fy, 81, y + 32, 30, 5, C.red);
    return;
  }
  if (cfg.mode === 'review' || cfg.mode === 'delight') {
    drawSpark(fx, fy, 78, y + 11, 7, cfg.mode === 'delight' ? C.yellow : C.green);
    drawSpark(fx, fy, 114, y + 11, 7, cfg.mode === 'delight' ? C.green : C.magenta);
    line(fx, fy, 82, y + 28, 96, y + 35, 5, C.eye);
    line(fx, fy, 96, y + 35, 110, y + 28, 5, C.eye);
    return;
  }
  if (cfg.mode === 'waiting') {
    rect(fx, fy, 72, y + 12, 18, 5, cfg.blink ? C.eye : C.cyanDim);
    rect(fx, fy, 106, y + 12, 18, 5, cfg.blink ? C.eye : C.cyanDim);
    rect(fx, fy, 87, y + 31, 18, 4, C.eye);
    return;
  }
  if (cfg.mode === 'working') {
    line(fx, fy, 70 + cfg.scan, y + 13, 82 + cfg.scan, y + 7, 5, C.cyan);
    line(fx, fy, 82 + cfg.scan, y + 7, 94 + cfg.scan, y + 16, 5, C.cyan);
    rect(fx, fy, 106, y + 17, 20, 5, C.cyan);
    glow(fx, fy, 68 + cfg.scan, y - 5, 5, 2, C.cyanDim);
    return;
  }
  ellipse(fx, fy, 78, y + 12, 8, 10, C.eye);
  ellipse(fx, fy, 114, y + 12, 8, 10, C.eye);
  ellipse(fx, fy, 81, y + 8, 2, 3, C.white);
  ellipse(fx, fy, 117, y + 8, 2, 3, C.white);
  line(fx, fy, 87, y + 31, 96, y + 37, 4, C.eye);
  line(fx, fy, 96, y + 37, 105, y + 31, 4, C.eye);
}

function drawCat(index, cfg) {
  const fx = index % columns;
  const fy = Math.floor(index / columns);
  clearFrame(fx, fy);

  const bob = cfg.bob || 0;
  const lean = cfg.lean || 0;
  const arm = cfg.arm || 0;
  const tail = cfg.tail || 0;
  const headY = 72 + bob;
  const bodyY = 126 + bob;

  ellipse(fx, fy, 97 + lean * 0.2, 184, 50, 10, C.shadow);
  line(fx, fy, 132 + lean, bodyY + 21, 161 + lean + tail, bodyY - 10, 15, C.outline);
  line(fx, fy, 132 + lean, bodyY + 21, 161 + lean + tail, bodyY - 10, 10, C.fur);
  ellipse(fx, fy, 162 + lean + tail, bodyY - 12, 8, 8, C.furLight);

  triangle(fx, fy, 48 + lean, headY - 31, 68 + lean, headY - 76, 88 + lean, headY - 31, C.outline);
  triangle(fx, fy, 104 + lean, headY - 31, 124 + lean, headY - 76, 144 + lean, headY - 31, C.outline);
  triangle(fx, fy, 56 + lean, headY - 35, 69 + lean, headY - 62, 81 + lean, headY - 35, C.blush);
  triangle(fx, fy, 112 + lean, headY - 35, 124 + lean, headY - 62, 136 + lean, headY - 35, C.blush);
  softEllipse(fx, fy, 96 + lean, headY, 61, 55, C.furLight, C.furDark);
  ellipse(fx, fy, 55 + lean, headY + 5, 15, 22, C.fur);
  ellipse(fx, fy, 137 + lean, headY + 5, 15, 22, C.fur);
  ellipse(fx, fy, 96 + lean, headY + 20, 30, 20, C.cream);
  ellipse(fx, fy, 96 + lean, headY + 23, 5, 4, C.blush);
  line(fx, fy, 54 + lean, headY + 20, 25 + lean, headY + 12, 3, C.outline);
  line(fx, fy, 55 + lean, headY + 28, 26 + lean, headY + 30, 3, C.outline);
  line(fx, fy, 138 + lean, headY + 20, 167 + lean, headY + 12, 3, C.outline);
  line(fx, fy, 137 + lean, headY + 28, 166 + lean, headY + 30, 3, C.outline);
  drawEyes(fx, fy, cfg, headY - 7);

  roundRect(fx, fy, 63 + lean, bodyY, 68, 46, 15, C.furDark);
  roundRect(fx, fy, 69 + lean, bodyY + 5, 56, 32, 11, C.fur);
  ellipse(fx, fy, 96 + lean, bodyY + 23, 24, 24, C.cream);
  ellipse(fx, fy, 79 + lean, bodyY + 42, 12, 16, C.furDark);
  ellipse(fx, fy, 113 + lean, bodyY + 42, 12, 16, C.furDark);
  ellipse(fx, fy, 45 + lean + arm, bodyY + 20, 11, 27, C.furDark);
  ellipse(fx, fy, 147 + lean - arm, bodyY + 20, 11, 27, C.furDark);

  if (cfg.mode === 'working') {
    rect(fx, fy, 79 + lean, bodyY + 23, 36 + (cfg.scan || 0), 5, C.cyan);
  } else if (cfg.mode === 'failed') {
    rect(fx, fy, 82 + lean, bodyY + 25, 29, 5, C.red);
  } else if (cfg.mode === 'review' || cfg.mode === 'delight') {
    glow(fx, fy, 86 + lean, bodyY + 25, 6, 4, C.green);
    glow(fx, fy, 108 + lean, bodyY + 25, 6, 4, C.magenta);
  } else {
    line(fx, fy, 80 + lean, bodyY + 24, 88 + lean, bodyY + 32, 5, C.white);
    line(fx, fy, 88 + lean, bodyY + 32, 103 + lean, bodyY + 17, 5, C.white);
  }

  rect(fx, fy, 63, 190, 66, 4, rgba('#3b242a', 160));

  if (cfg.symbol === 'spark') {
    drawSpark(fx, fy, 41, 43 + bob, 12, C.yellow);
    drawSpark(fx, fy, 151, 45 + bob, 10, C.green);
  } else if (cfg.symbol === 'question') {
    drawQuestion(fx, fy, 154, 48 + bob, C.yellow);
  } else if (cfg.symbol === 'warning') {
    drawWarning(fx, fy, 151, 45 + bob, C.red);
  } else if (cfg.symbol === 'lens') {
    drawLens(fx, fy, 151, 48 + bob, C.cyan);
  } else if (cfg.symbol === 'ping') {
    glow(fx, fy, 52 + lean, headY - 40, 8, 8, C.cyan);
    glow(fx, fy, 140 + lean, headY - 40, 8, 8, C.magenta);
  } else if (cfg.symbol === 'grip') {
    line(fx, fy, 38 + lean, bodyY + 16, 26 + lean, bodyY + 2, 5, C.white);
    line(fx, fy, 154 + lean, bodyY + 16, 166 + lean, bodyY + 2, 5, C.white);
  }
}

const frameSpecs = [
  ...Array.from({ length: 7 }, (_, i) => ['idle', { bob: [0, -1, -2, -1, 0, 1, 0][i], tail: [-2, -1, 0, 1, 2, 1, 0][i] }]),
  ...Array.from({ length: 4 }, (_, i) => ['idle', { bob: [0, -2, 0, 1][i], arm: [-2, 1, 2, -1][i], lean: [-2, 1, 2, -1][i], tail: [4, 2, -2, -4][i] }]),
  ['waiting', { bob: 1, blink: true }],
  ['waiting', { bob: 0, blink: false }],
  ['idle', { bob: 0 }],
  ['idle', { bob: 0, scan: 4 }],
  ['idle', { bob: 0, scan: -4 }],
  ['review', { bob: -1, mode: 'review' }],
  ['review', { bob: -2, mode: 'review', scan: 4 }],
  ['review', { bob: -1, mode: 'review', scan: -4 }],
  ['waiting', { bob: 0, blink: false }],
  ['waiting', { bob: 0, blink: true }],
  ...Array.from({ length: 4 }, (_, i) => ['working', { bob: [0, -1, 0, 1][i], scan: [0, 6, 12, 2][i], mode: 'working' }]),
  ...Array.from({ length: 4 }, (_, i) => ['working', { bob: [-1, 0, 1, 0][i], scan: [0, 5, 10, 15][i], arm: [0, 2, 0, -2][i], mode: 'working' }]),
  ['idle', { bob: -6, arm: 5 }],
  ['idle', { bob: -1, lean: 6, arm: -4 }],
  ['idle', { bob: 0, arm: -7 }],
  ['failed', { bob: 0, mode: 'failed' }],
  ['failed', { bob: 2, lean: -3, mode: 'failed' }],
  ['idle', { bob: -5 }],
  ['idle', { bob: -8, lean: 5 }],
  ['idle', { bob: -5 }],
  ['failed', { bob: 1, mode: 'failed' }],
  ['failed', { bob: -1, lean: 3, mode: 'failed' }],
  ['idle', { bob: -8 }],
  ['idle', { bob: 0, lean: 7 }],
  ['idle', { bob: 0, arm: -9 }],
  ['failed', { bob: 2, mode: 'failed' }],
  ['idle', { bob: 0, lean: -7 }],
  ['working', { bob: -2, lean: -4, scan: -8, symbol: 'lens', mode: 'working' }],
  ['working', { bob: -1, lean: 0, scan: 2, symbol: 'lens', mode: 'working' }],
  ['working', { bob: -2, lean: 4, scan: 12, symbol: 'lens', mode: 'working' }],
  ['curious', { bob: -1, lean: 2, symbol: 'question' }],
  ['working', { bob: 0, scan: 14, arm: 3, symbol: 'lens', mode: 'working' }],
  ['idle', { bob: -1, scan: -10, arm: -3, symbol: 'question' }],
  ['idle', { bob: -3, arm: 8, lean: -4, symbol: 'ping' }],
  ['idle', { bob: 0, arm: -8, lean: 4, symbol: 'ping' }],
  ['delight', { bob: -4, arm: 9, mode: 'delight', symbol: 'spark' }],
  ['waiting', { bob: 1, blink: true, symbol: 'question' }],
  ['delight', { bob: -4, arm: 8, mode: 'delight', symbol: 'spark' }],
  ['delight', { bob: -7, lean: -3, arm: 10, mode: 'delight', symbol: 'spark' }],
  ['review', { bob: -2, lean: 3, mode: 'review', symbol: 'spark' }],
  ['idle', { bob: 0, mode: 'review', symbol: 'spark' }],
  ['failed', { bob: 0, mode: 'failed', symbol: 'warning' }],
  ['failed', { bob: 1, lean: -5, mode: 'failed', symbol: 'warning' }],
  ['failed', { bob: -1, lean: 5, mode: 'failed', symbol: 'warning' }],
  ['failed', { bob: 1, mode: 'failed', symbol: 'question' }],
  ['idle', { bob: -2, lean: -8, arm: 8, symbol: 'grip' }],
  ['idle', { bob: -1, lean: 8, arm: -8, symbol: 'grip' }],
  ['idle', { bob: -3, arm: 12, symbol: 'grip' }],
  ['idle', { bob: 0, lean: -10, arm: 6, symbol: 'grip' }],
  ['delight', { bob: -12, arm: 10, mode: 'delight', symbol: 'spark' }],
  ['working', { bob: -2, lean: 10, scan: 10, arm: 5, mode: 'working' }],
];

frameSpecs.forEach(([, opts], index) => drawCat(index, opts));

const states = {
  idle: {
    spriteFrames: [0, 1, 2, 3, 4, 5, 6, 13, 14, 15, 43, 45, 46, 49],
    frames: [{ sprite: 0, duration: 1500 }, { sprite: 1, duration: 520 }, { sprite: 2, duration: 520 }, { sprite: 3, duration: 640 }, { sprite: 4, duration: 640 }, { sprite: 5, duration: 1700 }],
    loop: true,
    variants: [
      { name: 'ear_ping', weight: 3, cooldownMinMs: 9000, cooldownMaxMs: 18000, frames: [{ sprite: 13, duration: 160 }, { sprite: 46, duration: 180 }, { sprite: 0, duration: 260 }] },
      { name: 'curious_tail', weight: 2, cooldownMinMs: 12000, cooldownMaxMs: 24000, frames: [{ sprite: 14, duration: 300 }, { sprite: 15, duration: 300 }, { sprite: 45, duration: 220 }, { sprite: 0, duration: 260 }] },
      { name: 'question_popup', weight: 1, cooldownMinMs: 16000, cooldownMaxMs: 30000, frames: [{ sprite: 43, duration: 360 }, { sprite: 49, duration: 300 }, { sprite: 0, duration: 260 }] },
    ],
  },
  walk: { spriteFrames: [7, 8, 9, 10], frames: [{ sprite: 7, duration: 150 }, { sprite: 8, duration: 150 }, { sprite: 9, duration: 150 }, { sprite: 10, duration: 150 }], loop: true, autoIdleTimeout: 3000 },
  sleep: { spriteFrames: [11, 12], frames: [{ sprite: 11, duration: 900 }, { sprite: 12, duration: 900 }], loop: true },
  talk: { spriteFrames: [21, 22, 23, 24], frames: [{ sprite: 21, duration: 180 }, { sprite: 22, duration: 180 }, { sprite: 23, duration: 180 }, { sprite: 24, duration: 260 }], repeat: 3, fallback: 'idle' },
  happy: { spriteFrames: [16, 17, 18, 50, 51, 52, 53], frames: [{ sprite: 16, duration: 240 }, { sprite: 50, duration: 170 }, { sprite: 51, duration: 150 }, { sprite: 52, duration: 170 }, { sprite: 53, duration: 280 }], repeat: 3, fallback: 'idle' },
  confused: { spriteFrames: [19, 20, 43, 49], frames: [{ sprite: 19, duration: 320 }, { sprite: 43, duration: 420 }, { sprite: 49, duration: 360 }, { sprite: 20, duration: 300 }], repeat: 2, fallback: 'idle' },
  focused: { spriteFrames: [21, 22, 23, 24, 40, 41, 42, 44], frames: [{ sprite: 21, duration: 140 }, { sprite: 40, duration: 150 }, { sprite: 41, duration: 150 }, { sprite: 42, duration: 150 }, { sprite: 44, duration: 220 }, { sprite: 24, duration: 180 }], loop: true },
  preparing: { spriteFrames: [25, 26, 27, 28, 40, 41, 42, 44], frames: [{ sprite: 25, duration: 130 }, { sprite: 26, duration: 130 }, { sprite: 40, duration: 130 }, { sprite: 41, duration: 130 }, { sprite: 42, duration: 130 }, { sprite: 28, duration: 220 }], loop: true },
  gameplay: { spriteFrames: [29, 30, 58, 59, 60, 61], frames: [{ sprite: 29, duration: 220 }, { sprite: 58, duration: 160 }, { sprite: 59, duration: 160 }, { sprite: 60, duration: 160 }, { sprite: 61, duration: 220 }, { sprite: 30, duration: 220 }], loop: true },
  gamewin: { spriteFrames: [31, 32, 33, 50, 51, 52, 53, 62], frames: [{ sprite: 31, duration: 220 }, { sprite: 50, duration: 150 }, { sprite: 62, duration: 150 }, { sprite: 51, duration: 150 }, { sprite: 52, duration: 150 }, { sprite: 53, duration: 260 }], repeat: 5, fallback: 'idle' },
  gamelose: { spriteFrames: [34, 35, 54, 55, 56, 57], frames: [{ sprite: 54, duration: 220 }, { sprite: 55, duration: 180 }, { sprite: 56, duration: 180 }, { sprite: 57, duration: 300 }, { sprite: 35, duration: 360 }], repeat: 4, fallback: 'idle' },
  working: { spriteFrames: [21, 22, 23, 24, 25, 26, 27, 28, 40, 41, 42, 44], frames: [{ sprite: 21, duration: 150 }, { sprite: 22, duration: 150 }, { sprite: 40, duration: 140 }, { sprite: 41, duration: 140 }, { sprite: 42, duration: 140 }, { sprite: 44, duration: 220 }], loop: true },
  waiting: { spriteFrames: [19, 20], frames: [{ sprite: 19, duration: 620 }, { sprite: 20, duration: 620 }], loop: true },
  review: { spriteFrames: [16, 17, 18, 50, 51, 52, 53], frames: [{ sprite: 16, duration: 320 }, { sprite: 50, duration: 220 }, { sprite: 51, duration: 220 }, { sprite: 52, duration: 220 }, { sprite: 53, duration: 420 }], loop: true },
  failed: { spriteFrames: [34, 35, 54, 55, 56, 57], frames: [{ sprite: 54, duration: 170 }, { sprite: 55, duration: 170 }, { sprite: 56, duration: 170 }, { sprite: 57, duration: 260 }, { sprite: 34, duration: 400 }], repeat: 2, fallback: 'idle' },
};

const manifest = {
  schemaVersion: 2,
  id: 'cat',
  displayName: 'Cat',
  description: 'A high-resolution v2 cat companion with expressive status animations.',
  render: {
    mode: 'sheet',
    displayWidth: 74,
    displayHeight: 80,
    scale: 80 / frameHeight,
    pixelated: false,
  },
  hotspots: {
    observe: { x: 0.2, y: 0.12, w: 0.6, h: 0.38 },
    input: { x: 0.24, y: 0.42, w: 0.52, h: 0.32 },
  },
  sprite: {
    image: 'spritesheet.webp',
    frameWidth,
    frameHeight,
    columns,
    rows,
    frameCount,
  },
  states,
  actions: {
    jump: { spriteFrames: [36, 62, 31], frames: [{ sprite: 36, duration: 120 }, { sprite: 62, duration: 160 }, { sprite: 31, duration: 140 }], repeat: 1, fallback: 'idle' },
    spin: { spriteFrames: [37, 63, 40, 42], frames: [{ sprite: 37, duration: 90 }, { sprite: 63, duration: 90 }, { sprite: 40, duration: 90 }, { sprite: 42, duration: 90 }], repeat: 2, fallback: 'idle' },
    wave: { spriteFrames: [38, 48, 50, 53], frames: [{ sprite: 38, duration: 140 }, { sprite: 48, duration: 140 }, { sprite: 50, duration: 140 }, { sprite: 53, duration: 220 }], repeat: 1, fallback: 'idle' },
    shake: { spriteFrames: [39, 54, 55, 56, 57], frames: [{ sprite: 39, duration: 100 }, { sprite: 54, duration: 100 }, { sprite: 55, duration: 100 }, { sprite: 56, duration: 100 }, { sprite: 57, duration: 180 }], repeat: 1, fallback: 'idle' },
    observe: { spriteFrames: [40, 41, 42, 43, 44, 45], frames: [{ sprite: 40, duration: 120 }, { sprite: 41, duration: 120 }, { sprite: 42, duration: 120 }, { sprite: 44, duration: 180 }, { sprite: 43, duration: 240 }, { sprite: 45, duration: 220 }], repeat: 1, fallback: 'idle' },
    nudge: { spriteFrames: [46, 47, 48, 49], frames: [{ sprite: 46, duration: 140 }, { sprite: 47, duration: 140 }, { sprite: 48, duration: 160 }, { sprite: 49, duration: 240 }], repeat: 1, fallback: 'idle' },
    acknowledge: { spriteFrames: [50, 51, 52, 53], frames: [{ sprite: 50, duration: 160 }, { sprite: 51, duration: 160 }, { sprite: 52, duration: 160 }, { sprite: 53, duration: 260 }], repeat: 1, fallback: 'idle' },
    blocked: { spriteFrames: [54, 55, 56, 57], frames: [{ sprite: 54, duration: 150 }, { sprite: 55, duration: 150 }, { sprite: 56, duration: 150 }, { sprite: 57, duration: 280 }], repeat: 1, fallback: 'idle' },
    dragging: { spriteFrames: [58, 59, 60, 61], frames: [{ sprite: 58, duration: 120 }, { sprite: 59, duration: 120 }, { sprite: 60, duration: 120 }, { sprite: 61, duration: 160 }], repeat: 2, fallback: 'idle' },
  },
  mini: {
    state: 'idle',
    frame: 0,
    headRows: 140,
  },
  metadata: {
    generatedFrom: ['app/frontend/tools/generate-cat-pack.cjs'],
    assetClass: 'default-companion',
    qualityTier: 'polished',
    style: 'smooth-scaled expressive orange cat companion',
    recommendedUse: 'default pet, game reactions, and status feedback',
    releaseTier: 'builtin',
    optimizedFor: 'smooth-scaled desktop pet window',
    frameLabels: frameSpecs.slice(0, frameCount).map(([mode], index) => `${mode}:${index}`),
  },
};

function crc32(buf) {
  let c = ~0;
  for (let i = 0; i < buf.length; i += 1) {
    c ^= buf[i];
    for (let k = 0; k < 8; k += 1) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return ~c >>> 0;
}

function chunk(type, data) {
  const typeBuf = Buffer.from(type, 'ascii');
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  typeBuf.copy(out, 4);
  data.copy(out, 8);
  out.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 8 + data.length);
  return out;
}

function pngBuffer() {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(sheetWidth, 0);
  ihdr.writeUInt32BE(sheetHeight, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const stride = sheetWidth * 4;
  const raw = Buffer.alloc((stride + 1) * sheetHeight);
  for (let y = 0; y < sheetHeight; y += 1) {
    raw[y * (stride + 1)] = 0;
    Buffer.from(pixels.buffer, y * stride, stride).copy(raw, y * (stride + 1) + 1);
  }
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

function convertPngToWebp(pngPath, outPath) {
  execFileSync('ffmpeg', [
    '-y',
    '-loglevel',
    'error',
    '-i',
    pngPath,
    '-c:v',
    'libwebp',
    '-quality',
    '90',
    '-compression_level',
    '6',
    outPath,
  ], { stdio: 'inherit' });
}

writeFileSync(tempPngPath, pngBuffer());
convertPngToWebp(tempPngPath, webpPath);
if (existsSync(tempPngPath)) rmSync(tempPngPath);
writeFileSync(path.join(outDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
