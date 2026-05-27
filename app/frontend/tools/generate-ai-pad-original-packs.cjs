const { existsSync, mkdirSync, rmSync, writeFileSync } = require('node:fs');
const { execFileSync } = require('node:child_process');
const path = require('node:path');
const { deflateSync } = require('node:zlib');

const petsRoot = path.join(__dirname, '..', '__fixtures__', 'pets');
const frameWidth = 192;
const frameHeight = 208;
const columns = 8;
const rows = 8;
const frameCount = 64;
const sheetWidth = frameWidth * columns;
const sheetHeight = frameHeight * rows;

const rgba = (hex, alpha = 255) => {
  const n = Number.parseInt(hex.replace('#', ''), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, alpha];
};

const C = {
  shadow: rgba('#050713', 80),
  ink: rgba('#14192b'),
  ink2: rgba('#242b46'),
  shell: rgba('#eee8d0'),
  shellShade: rgba('#c7d0c9'),
  coral: rgba('#ee706e'),
  coralDark: rgba('#b5485b'),
  screen: rgba('#111d37'),
  screen2: rgba('#24365c'),
  cyan: rgba('#70f6d6'),
  cyanDim: rgba('#49bbc3'),
  blue: rgba('#58a6ff'),
  green: rgba('#80ed99'),
  amber: rgba('#ffce5b'),
  rose: rgba('#ff76b5'),
  red: rgba('#ff5b6d'),
  white: rgba('#fffbec'),
  violet: rgba('#8a6cff'),
  darkViolet: rgba('#2a214b'),
};

const packs = [
  {
    id: 'padlet',
    displayName: 'Padlet',
    description: 'An original BitCat handheld terminal companion for agent status and desktop feedback.',
    assetClass: 'bitcat-original',
    style: 'handheld terminal companion with glass screen face',
    recommendedUse: 'default BitCat companion and status feedback',
    draw: drawPadlet,
  },
  {
    id: 'hackmark',
    displayName: 'Hackmark',
    description: 'A geek-styled animated terminal mark for compact BitCat status feedback.',
    assetClass: 'geek-logo',
    style: 'cyber terminal logo with command glyph and status core',
    recommendedUse: 'logo-like pet, tray-adjacent identity, and focused tool feedback',
    draw: drawHackmark,
  },
];

const frameStates = [
  'idle', 'idle', 'idle', 'idle', 'idle', 'idle', 'idle', 'walk',
  'walk', 'walk', 'walk', 'sleep', 'sleep', 'idle', 'idle', 'idle',
  'happy', 'happy', 'happy', 'confused', 'confused', 'talk', 'talk', 'talk',
  'talk', 'preparing', 'preparing', 'preparing', 'preparing', 'gameplay', 'gameplay', 'gamewin',
  'gamewin', 'gamelose', 'gamelose', 'gamelose', 'idle', 'idle', 'idle', 'idle',
  'focused', 'focused', 'focused', 'observe', 'focused', 'observe', 'idle', 'confused',
  'working', 'working', 'happy', 'happy', 'happy', 'waiting', 'failed', 'failed',
  'failed', 'failed', 'dragging', 'dragging', 'dragging', 'dragging', 'gamewin', 'idle',
];

let pixels = null;

function blendPixel(x, y, color) {
  if (x < 0 || y < 0 || x >= sheetWidth || y >= sheetHeight) return;
  const ix = (Math.floor(y) * sheetWidth + Math.floor(x)) * 4;
  const alpha = color[3] / 255;
  const inv = 1 - alpha;
  pixels[ix] = Math.round(color[0] * alpha + pixels[ix] * inv);
  pixels[ix + 1] = Math.round(color[1] * alpha + pixels[ix + 1] * inv);
  pixels[ix + 2] = Math.round(color[2] * alpha + pixels[ix + 2] * inv);
  pixels[ix + 3] = Math.round(255 * (alpha + (pixels[ix + 3] / 255) * inv));
}

function frameOffset(index) {
  return [(index % columns) * frameWidth, Math.floor(index / columns) * frameHeight];
}

function rect(index, x, y, w, h, color) {
  const [ox, oy] = frameOffset(index);
  for (let yy = Math.floor(y); yy < Math.ceil(y + h); yy += 1) {
    for (let xx = Math.floor(x); xx < Math.ceil(x + w); xx += 1) {
      blendPixel(ox + xx, oy + yy, color);
    }
  }
}

function ellipse(index, cx, cy, rx, ry, color) {
  const [ox, oy] = frameOffset(index);
  for (let y = Math.floor(cy - ry); y <= Math.ceil(cy + ry); y += 1) {
    for (let x = Math.floor(cx - rx); x <= Math.ceil(cx + rx); x += 1) {
      const dx = (x + 0.5 - cx) / rx;
      const dy = (y + 0.5 - cy) / ry;
      if (dx * dx + dy * dy <= 1) blendPixel(ox + x, oy + y, color);
    }
  }
}

function roundRect(index, x, y, w, h, r, color) {
  rect(index, x + r, y, w - r * 2, h, color);
  rect(index, x, y + r, r, h - r * 2, color);
  rect(index, x + w - r, y + r, r, h - r * 2, color);
  ellipse(index, x + r, y + r, r, r, color);
  ellipse(index, x + w - r, y + r, r, r, color);
  ellipse(index, x + r, y + h - r, r, r, color);
  ellipse(index, x + w - r, y + h - r, r, r, color);
}

function line(index, x0, y0, x1, y1, width, color) {
  const steps = Math.max(1, Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0)) * 2);
  for (let i = 0; i <= steps; i += 1) {
    const t = i / steps;
    ellipse(index, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, width / 2, width / 2, color);
  }
}

function polygon(index, points, color) {
  const [ox, oy] = frameOffset(index);
  const ys = points.map(([, y]) => y);
  for (let y = Math.floor(Math.min(...ys)); y <= Math.ceil(Math.max(...ys)); y += 1) {
    const xs = [];
    for (let i = 0; i < points.length; i += 1) {
      const [x1, y1] = points[i];
      const [x2, y2] = points[(i + 1) % points.length];
      if ((y1 <= y && y2 > y) || (y2 <= y && y1 > y)) {
        xs.push(x1 + ((y - y1) / (y2 - y1)) * (x2 - x1));
      }
    }
    xs.sort((a, b) => a - b);
    for (let i = 0; i < xs.length; i += 2) {
      for (let x = Math.floor(xs[i]); x <= Math.ceil(xs[i + 1]); x += 1) blendPixel(ox + x, oy + y, color);
    }
  }
}

function glow(index, cx, cy, rx, ry, color, strength = 52) {
  for (let i = 5; i >= 0; i -= 1) {
    ellipse(index, cx, cy, rx + i * 4, ry + i * 3, [color[0], color[1], color[2], Math.max(8, strength - i * 8)]);
  }
  ellipse(index, cx, cy, rx, ry, color);
}

function chevron(index, x, y, size, color, flip = false) {
  const dir = flip ? -1 : 1;
  line(index, x - dir * size * 0.55, y - size * 0.6, x + dir * size * 0.25, y, 5, color);
  line(index, x + dir * size * 0.25, y, x - dir * size * 0.55, y + size * 0.6, 5, color);
}

function plus(index, x, y, size, color) {
  line(index, x - size, y, x + size, y, 3, color);
  line(index, x, y - size, x, y + size, 3, color);
}

function drawFace(index, mood, phase, xShift = 0) {
  if (mood === 'sleep') {
    line(index, 70 + xShift, 91, 92 + xShift, 91, 4, C.cyan);
    line(index, 102 + xShift, 91, 124 + xShift, 91, 4, C.cyan);
    line(index, 83 + xShift, 113, 109 + xShift, 113, 3, C.blue);
    return;
  }
  if (mood === 'failed') {
    line(index, 70 + xShift, 86, 86 + xShift, 102, 4, C.red);
    line(index, 86 + xShift, 86, 70 + xShift, 102, 4, C.red);
    line(index, 106 + xShift, 86, 122 + xShift, 102, 4, C.red);
    line(index, 122 + xShift, 86, 106 + xShift, 102, 4, C.red);
    line(index, 82 + xShift, 119, 112 + xShift, 119, 4, C.red);
    return;
  }
  if (mood === 'confused') {
    ellipse(index, 76 + xShift, 91, 7, 7, C.green);
    ellipse(index, 115 + xShift, 91, 7, 7, C.rose);
    line(index, 82 + xShift, 115, 112 + xShift, 115, 3, C.blue);
    return;
  }
  if (mood === 'happy') {
    chevron(index, 79 + xShift, 91, 15, C.cyan);
    chevron(index, 115 + xShift, 91, 15, C.cyan);
    line(index, 80 + xShift, 113, 94 + xShift, 123, 4, C.amber);
    line(index, 94 + xShift, 123, 111 + xShift, 113, 4, C.amber);
    return;
  }
  if (mood === 'focused') {
    line(index, 65 + xShift, 91, 89 + xShift, 91, 4, C.cyan);
    line(index, 105 + xShift, 91, 129 + xShift, 91, 4, C.cyan);
    line(index, 81 + xShift, 116, 114 + xShift, 116, 4, C.blue);
    return;
  }
  if (mood === 'talk') {
    chevron(index, 79 + xShift, 91, 14, C.cyan);
    chevron(index, 115 + xShift, 91, 14, C.cyan);
    roundRect(index, 84 + xShift, 112, 25 + Math.abs(Math.sin(phase * Math.PI * 2)) * 10, 6, 3, C.blue);
    return;
  }
  if (phase > 0.72 && phase < 0.86) {
    line(index, 69 + xShift, 91, 88 + xShift, 91, 4, C.cyan);
    line(index, 105 + xShift, 91, 124 + xShift, 91, 4, C.cyan);
  } else {
    chevron(index, 79 + xShift, 91, 15, C.cyan);
    chevron(index, 115 + xShift, 91, 15, C.cyan);
  }
  line(index, 83 + xShift, 116, 108 + xShift, 116, 3, C.blue);
}

function stateMood(state) {
  if (state === 'sleep' || state === 'waiting') return 'sleep';
  if (state === 'failed' || state === 'gamelose') return 'failed';
  if (state === 'confused') return 'confused';
  if (state === 'happy' || state === 'gamewin' || state === 'review') return 'happy';
  if (state === 'talk') return 'talk';
  if (state === 'focused' || state === 'preparing' || state === 'working' || state === 'observe') return 'focused';
  return 'idle';
}

function drawPadlet(index, state) {
  const phase = (index % 8) / 8;
  const active = ['walk', 'dragging', 'gameplay'].includes(state);
  const bob = Math.sin(phase * Math.PI * 2 * (active ? 2 : 1)) * (active ? 5 : 3);
  const cy = 104 + bob;
  const xShift = Math.sin(phase * Math.PI * 2) * (active ? 3 : 1);

  roundRect(index, 51, 180, 90, 10, 8, C.shadow);
  glow(index, 61, cy - 61, 7, 7, C.cyan, 96);
  glow(index, 132, cy - 61, 7, 7, state === 'failed' ? C.red : C.rose, 90);

  roundRect(index, 31, cy - 27, 24, 73, 13, C.ink);
  roundRect(index, 137, cy - 27, 24, 73, 13, C.ink);
  roundRect(index, 38, cy - 21, 20, 61, 11, C.coral);
  roundRect(index, 134, cy - 21, 20, 61, 11, C.coral);
  roundRect(index, 58 + Math.sin(phase * Math.PI * 2) * 2, cy + 54, 18, 20, 7, C.ink);
  roundRect(index, 116, cy + 54, 18 + Math.cos(phase * Math.PI * 2) * 2, 20, 7, C.ink);

  line(index, 73, cy - 56, 65, cy - 72, 4, C.ink2);
  line(index, 120, cy - 56, 132, cy - 72, 4, C.ink2);
  ellipse(index, 64, cy - 76, 8, 8, C.screen);
  ellipse(index, 132, cy - 76, 8, 8, C.screen);
  ellipse(index, 64, cy - 76, 3, 3, state === 'gamewin' ? C.green : C.cyan);
  ellipse(index, 132, cy - 76, 3, 3, state === 'gamelose' ? C.red : C.rose);

  roundRect(index, 43 + xShift, cy - 61, 106, 119, 33, C.ink);
  roundRect(index, 49 + xShift, cy - 57, 94, 109, 29, C.shell);
  roundRect(index, 52 + xShift, cy - 54, 88, 41, 24, C.coral);
  roundRect(index, 55 + xShift, cy + 17, 82, 41, 16, C.shellShade);
  roundRect(index, 47 + xShift, cy - 38, 98, 54, 16, C.ink);
  roundRect(index, 54 + xShift, cy - 31, 84, 39, 10, C.screen);
  line(index, 62 + xShift, cy - 25, 84 + xShift, cy - 25, 2, [...C.cyan, 95]);

  drawFace(index, stateMood(state), phase, xShift);

  roundRect(index, 62, 139, 68, 31, 10, C.coralDark);
  if (['focused', 'preparing', 'working', 'talk', 'gameplay'].includes(state)) {
    chevron(index, 81, 157, 14, C.white);
    line(index, 98 + Math.sin(phase * Math.PI * 2) * 2, 159, 116, 159, 4, C.cyan);
  } else if (['failed', 'gamelose'].includes(state)) {
    roundRect(index, 78, 151, 36, 10, 3, C.red);
  } else if (['happy', 'gamewin', 'review'].includes(state)) {
    ellipse(index, 84, 157, 7, 7, C.amber);
    ellipse(index, 108, 157, 7, 7, C.green);
  } else {
    line(index, 77, 157, 91, 157, 4, C.white);
    line(index, 101, 157, 116, 157, 4, C.cyan);
  }

  if (state === 'observe') {
    roundRect(index, 124, cy - 48, 30, 29, 8, C.screen);
    ellipse(index, 139, cy - 33, 7, 7, C.blue);
  }
  if (['preparing', 'working'].includes(state)) {
    for (let i = 0; i < 3; i += 1) {
      const a = phase * Math.PI * 2 + i * 2.1;
      ellipse(index, 96 + Math.cos(a) * 43, cy - 4 + Math.sin(a) * 31, 3, 3, [...C.cyan, 180]);
    }
  }
  if (['happy', 'gamewin'].includes(state)) {
    plus(index, 50, 54, 5, C.amber);
    plus(index, 143, 59, 5, C.green);
    plus(index, 133, 37, 4, C.rose);
  }
  if (['failed', 'gamelose'].includes(state)) {
    polygon(index, [[134, 45], [154, 78], [116, 78]], [...C.red, 64]);
    line(index, 134, 56, 134, 68, 4, C.red);
    ellipse(index, 134, 75, 3, 3, C.red);
  }
  if (state === 'confused') {
    line(index, 139, 52, 149, 44, 5, C.amber);
    line(index, 149, 44, 157, 52, 5, C.amber);
    line(index, 157, 52, 149, 62, 5, C.amber);
    line(index, 149, 62, 149, 72, 5, C.amber);
    ellipse(index, 149, 83, 3, 3, C.amber);
  }
}

function drawHackmark(index, state) {
  const phase = (index % 8) / 8;
  const pulse = 1 + Math.sin(phase * Math.PI * 2) * 0.06;
  const active = ['focused', 'preparing', 'working', 'talk', 'gameplay', 'observe'].includes(state);
  const danger = ['failed', 'gamelose'].includes(state);
  const happy = ['happy', 'gamewin', 'review'].includes(state);
  const cx = 96;
  const cy = 103 + Math.sin(phase * Math.PI * 2) * (active ? 4 : 2);
  const r = 54 * pulse;

  roundRect(index, 43, 177, 106, 10, 8, C.shadow);
  glow(index, cx, cy, 38, 34, danger ? C.red : (happy ? C.green : C.cyan), 70);

  const hex = [
    [cx, cy - r],
    [cx + r * 0.88, cy - r * 0.5],
    [cx + r * 0.88, cy + r * 0.5],
    [cx, cy + r],
    [cx - r * 0.88, cy + r * 0.5],
    [cx - r * 0.88, cy - r * 0.5],
  ];
  polygon(index, hex, danger ? C.darkViolet : C.ink);
  polygon(index, hex.map(([x, y]) => [cx + (x - cx) * 0.86, cy + (y - cy) * 0.86]), C.screen);
  polygon(index, hex.map(([x, y]) => [cx + (x - cx) * 0.66, cy + (y - cy) * 0.66]), danger ? rgba('#351b2c') : rgba('#132b40'));

  for (let i = 0; i < 6; i += 1) {
    const [x1, y1] = hex[i];
    const [x2, y2] = hex[(i + 1) % 6];
    line(index, x1, y1, x2, y2, 4, danger ? C.red : (happy ? C.green : C.cyan));
  }

  const scanY = cy - 25 + ((phase * 52 + index * 2) % 52);
  line(index, cx - 42, scanY, cx + 42, scanY, 2, [...(danger ? C.red : C.cyan), 120]);

  if (danger) {
    line(index, 69, cy - 16, 84, cy - 1, 6, C.red);
    line(index, 84, cy - 16, 69, cy - 1, 6, C.red);
    line(index, 108, cy - 16, 123, cy - 1, 6, C.red);
    line(index, 123, cy - 16, 108, cy - 1, 6, C.red);
    line(index, 77, cy + 29, 115, cy + 29, 5, C.red);
  } else {
    chevron(index, 78, cy - 6, 23, happy ? C.amber : C.cyan);
    chevron(index, 116, cy - 6, 23, happy ? C.green : C.cyan, true);
    line(index, 84, cy + 26, 116, cy + 26, 5, happy ? C.amber : C.blue);
    if (active) line(index, 120 + Math.sin(phase * Math.PI * 2) * 4, cy + 26, 132, cy + 26, 5, C.cyan);
  }

  roundRect(index, 62, cy + 53, 68, 24, 8, C.ink2);
  if (active) {
    chevron(index, 79, cy + 65, 13, C.white);
    line(index, 95, cy + 66, 117, cy + 66, 4, C.cyan);
  } else if (happy) {
    plus(index, 78, cy + 65, 5, C.amber);
    plus(index, 113, cy + 65, 5, C.green);
  } else {
    line(index, 76, cy + 65, 91, cy + 65, 4, C.white);
    line(index, 102, cy + 65, 118, cy + 65, 4, C.cyan);
  }

  if (state === 'confused') {
    line(index, 139, 49, 151, 39, 5, C.amber);
    line(index, 151, 39, 160, 50, 5, C.amber);
    line(index, 160, 50, 151, 63, 5, C.amber);
    ellipse(index, 151, 75, 3, 3, C.amber);
  }
  if (state === 'observe') {
    roundRect(index, 129, 61, 32, 28, 7, C.screen2);
    ellipse(index, 145, 75, 8, 8, C.blue);
  }
  if (happy) {
    plus(index, 46, 70, 6, C.amber);
    plus(index, 146, 55, 6, C.green);
  }
}

function seq(indices, duration = 160) {
  return indices.map((sprite) => ({ sprite, duration }));
}

function manifestFor(pack) {
  const states = {
    idle: {
      spriteFrames: [0, 1, 2, 3, 4, 5, 6, 13, 14, 15, 46, 63],
      frames: [{ sprite: 0, duration: 1300 }, { sprite: 1, duration: 520 }, { sprite: 2, duration: 520 }, { sprite: 3, duration: 640 }, { sprite: 4, duration: 640 }, { sprite: 5, duration: 1500 }],
      loop: true,
      variants: [
        { name: 'status_ping', weight: 3, cooldownMinMs: 9000, cooldownMaxMs: 18000, frames: [{ sprite: 13, duration: 170 }, { sprite: 14, duration: 170 }, { sprite: 0, duration: 260 }] },
        { name: 'screen_glance', weight: 2, cooldownMinMs: 12000, cooldownMaxMs: 24000, frames: [{ sprite: 15, duration: 260 }, { sprite: 46, duration: 220 }, { sprite: 0, duration: 260 }] },
      ],
    },
    walk: { spriteFrames: [7, 8, 9, 10], frames: seq([7, 8, 9, 10], 145), loop: true, autoIdleTimeout: 3000 },
    sleep: { spriteFrames: [11, 12], frames: seq([11, 12], 900), loop: true },
    talk: { spriteFrames: [21, 22, 23, 24], frames: seq([21, 22, 23, 24], 170), repeat: 3, fallback: 'idle' },
    happy: { spriteFrames: [16, 17, 18, 50, 51, 52], frames: seq([16, 50, 51, 52, 18], 170), repeat: 3, fallback: 'idle' },
    confused: { spriteFrames: [19, 20, 47], frames: seq([19, 47, 20, 47], 300), repeat: 2, fallback: 'idle' },
    focused: { spriteFrames: [40, 41, 42, 44], frames: seq([40, 41, 42, 44], 145), loop: true },
    preparing: { spriteFrames: [25, 26, 27, 28, 40, 41, 42], frames: seq([25, 26, 40, 41, 42, 28], 130), loop: true },
    gameplay: { spriteFrames: [29, 30, 58, 59, 60, 61], frames: seq([29, 58, 59, 60, 61, 30], 160), loop: true },
    gamewin: { spriteFrames: [31, 32, 50, 51, 52, 62], frames: seq([31, 50, 62, 51, 52, 32], 160), repeat: 5, fallback: 'idle' },
    gamelose: { spriteFrames: [33, 34, 35, 54, 55, 56, 57], frames: seq([54, 55, 56, 57, 35], 190), repeat: 4, fallback: 'idle' },
    working: { spriteFrames: [40, 41, 42, 44, 48, 49], frames: seq([48, 40, 41, 42, 44, 49], 145), loop: true },
    waiting: { spriteFrames: [19, 20, 53], frames: seq([53, 19, 20], 620), loop: true },
    review: { spriteFrames: [16, 17, 18, 50, 51, 52], frames: seq([16, 50, 51, 52, 18], 220), loop: true },
    failed: { spriteFrames: [54, 55, 56, 57], frames: seq([54, 55, 56, 57], 170), repeat: 2, fallback: 'idle' },
  };
  return {
    schemaVersion: 2,
    id: pack.id,
    displayName: pack.displayName,
    description: pack.description,
    render: { mode: 'sheet', displayWidth: 74, displayHeight: 80, scale: 80 / frameHeight, pixelated: false },
    hotspots: { observe: { x: 0.24, y: 0.1, w: 0.52, h: 0.38 }, input: { x: 0.22, y: 0.42, w: 0.56, h: 0.34 } },
    sprite: { image: 'spritesheet.webp', frameWidth, frameHeight, columns, rows, frameCount },
    states,
    actions: {
      jump: { spriteFrames: [36, 62, 31], frames: seq([36, 62, 31], 140), repeat: 1, fallback: 'idle' },
      spin: { spriteFrames: [37, 58, 59, 60], frames: seq([37, 58, 59, 60], 90), repeat: 2, fallback: 'idle' },
      wave: { spriteFrames: [38, 50, 51, 52], frames: seq([38, 50, 51, 52], 150), repeat: 1, fallback: 'idle' },
      shake: { spriteFrames: [39, 54, 55, 56, 57], frames: seq([39, 54, 55, 56, 57], 100), repeat: 1, fallback: 'idle' },
      observe: { spriteFrames: [43, 45], frames: seq([43, 45, 43, 45], 150), repeat: 1, fallback: 'idle' },
      nudge: { spriteFrames: [46, 47], frames: seq([46, 47, 46], 160), repeat: 1, fallback: 'idle' },
      acknowledge: { spriteFrames: [50, 51, 52], frames: seq([50, 51, 52], 160), repeat: 1, fallback: 'idle' },
      blocked: { spriteFrames: [54, 55, 56, 57], frames: seq([54, 55, 56, 57], 150), repeat: 1, fallback: 'idle' },
      dragging: { spriteFrames: [58, 59, 60, 61], frames: seq([58, 59, 60, 61], 120), repeat: 2, fallback: 'idle' },
    },
    mini: { state: 'idle', frame: 0, headRows: 140 },
    metadata: {
      generatedFrom: ['app/frontend/tools/generate-ai-pad-original-packs.cjs'],
      assetClass: pack.assetClass,
      qualityTier: 'generated',
      style: pack.style,
      recommendedUse: pack.recommendedUse,
      releaseTier: 'builtin-candidate',
      optimizedFor: 'smooth-scaled desktop pet window',
      frameLabels: frameStates.map((state, index) => `${state}:${index}`),
    },
  };
}

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
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-i', pngPath, '-c:v', 'libwebp', '-quality', '90', '-compression_level', '6', outPath], { stdio: 'inherit' });
}

function generate(pack) {
  const outDir = path.join(petsRoot, pack.id);
  mkdirSync(outDir, { recursive: true });
  pixels = new Uint8ClampedArray(sheetWidth * sheetHeight * 4);
  frameStates.forEach((state, index) => pack.draw(index, state));
  const tempPng = path.join(outDir, 'spritesheet.tmp.png');
  const webp = path.join(outDir, 'spritesheet.webp');
  writeFileSync(tempPng, pngBuffer());
  convertPngToWebp(tempPng, webp);
  if (existsSync(tempPng)) rmSync(tempPng);
  writeFileSync(path.join(outDir, 'manifest.json'), `${JSON.stringify(manifestFor(pack), null, 2)}\n`);
}

packs.forEach(generate);
