const { existsSync, mkdirSync, writeFileSync, rmSync } = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { deflateSync } = require('node:zlib');

const petsRoot = path.join(__dirname, '..', '__fixtures__', 'pets');
const frameWidth = 192;
const frameHeight = 208;
const columns = 8;
const rows = 5;
const frameCount = 40;
const sheetWidth = frameWidth * columns;
const sheetHeight = frameHeight * rows;

const packs = [
  {
    id: 'byte-bun',
    displayName: 'Byte Bun',
    description: 'A bright terminal bunny for playful coding sessions.',
    style: 'soft-eared terminal bunny companion',
    recommendedUse: 'friendly idle feedback and cheerful confirmations',
    colors: {
      shadow: '#060812',
      rim: '#33254b',
      dark: '#51406f',
      mid: '#8d7fd8',
      light: '#d6c7ff',
      accent: '#69f2d4',
      alt: '#ff7fb7',
      warn: '#ff5f7a',
      ok: '#92f7a8',
      screen: '#11182e',
      screen2: '#26305a',
      white: '#fff6ff',
    },
    shape: 'ears',
  },
  {
    id: 'mossbot',
    displayName: 'Mossbot',
    description: 'A compact green desk bot with calm status animations.',
    style: 'moss-green compact terminal robot',
    recommendedUse: 'quiet focus mode and low-distraction work feedback',
    colors: {
      shadow: '#04100b',
      rim: '#18352b',
      dark: '#245642',
      mid: '#48a56d',
      light: '#a4f0b8',
      accent: '#8af7df',
      alt: '#f1dd6a',
      warn: '#ff6f62',
      ok: '#b8ff7e',
      screen: '#0f1d24',
      screen2: '#1d3b3f',
      white: '#f3fff5',
    },
    shape: 'antenna',
  },
  {
    id: 'moonbit',
    displayName: 'Moonbit',
    description: 'A crescent-shaped night companion for slower sessions.',
    style: 'lunar-blue crescent desktop companion',
    recommendedUse: 'sleep, waiting, and late-night coding moods',
    colors: {
      shadow: '#050714',
      rim: '#1d2d5f',
      dark: '#263f87',
      mid: '#5e85dc',
      light: '#bfd6ff',
      accent: '#8df7ff',
      alt: '#d9b6ff',
      warn: '#ff708d',
      ok: '#a8f5c1',
      screen: '#0d1732',
      screen2: '#1d315d',
      white: '#f6fbff',
    },
    shape: 'crescent',
  },
  {
    id: 'sparkle',
    displayName: 'Sparkle',
    description: 'A small electric star buddy for active tool feedback.',
    style: 'electric star terminal companion',
    recommendedUse: 'tool activity, rapid iteration, and success reactions',
    colors: {
      shadow: '#100813',
      rim: '#59305a',
      dark: '#8b3c7d',
      mid: '#ed68b4',
      light: '#ffd0ef',
      accent: '#8dfff0',
      alt: '#ffe66b',
      warn: '#ff4e69',
      ok: '#92ff9a',
      screen: '#1a112a',
      screen2: '#372450',
      white: '#fff8fb',
    },
    shape: 'star',
  },
];

const states = {
  idle: {
    spriteFrames: [0, 1, 2, 3, 4, 5, 6, 13, 14, 15],
    frames: [
      { sprite: 0, duration: 1500 },
      { sprite: 1, duration: 520 },
      { sprite: 2, duration: 520 },
      { sprite: 3, duration: 640 },
      { sprite: 4, duration: 640 },
      { sprite: 5, duration: 1700 },
    ],
    loop: true,
    variants: [
      { name: 'glance', weight: 3, cooldownMinMs: 9000, cooldownMaxMs: 18000, frames: [{ sprite: 13, duration: 160 }, { sprite: 0, duration: 260 }] },
      { name: 'pulse', weight: 2, cooldownMinMs: 12000, cooldownMaxMs: 24000, frames: [{ sprite: 14, duration: 300 }, { sprite: 15, duration: 300 }, { sprite: 0, duration: 260 }] },
    ],
  },
  walk: { spriteFrames: [7, 8, 9, 10], frames: [{ sprite: 7, duration: 150 }, { sprite: 8, duration: 150 }, { sprite: 9, duration: 150 }, { sprite: 10, duration: 150 }], loop: true, autoIdleTimeout: 3000 },
  sleep: { spriteFrames: [11, 12], frames: [{ sprite: 11, duration: 900 }, { sprite: 12, duration: 900 }], loop: true },
  talk: { spriteFrames: [21, 22, 23, 24], frames: [{ sprite: 21, duration: 180 }, { sprite: 22, duration: 180 }, { sprite: 23, duration: 180 }, { sprite: 24, duration: 260 }], repeat: 3, fallback: 'idle' },
  happy: { spriteFrames: [16, 17, 18], frames: [{ sprite: 16, duration: 260 }, { sprite: 17, duration: 160 }, { sprite: 18, duration: 320 }], repeat: 3, fallback: 'idle' },
  confused: { spriteFrames: [19, 20], frames: [{ sprite: 19, duration: 420 }, { sprite: 20, duration: 420 }], repeat: 2, fallback: 'idle' },
  focused: { spriteFrames: [21, 22, 23, 24], frames: [{ sprite: 21, duration: 170 }, { sprite: 22, duration: 170 }, { sprite: 23, duration: 170 }, { sprite: 24, duration: 240 }], loop: true },
  preparing: { spriteFrames: [25, 26, 27, 28], frames: [{ sprite: 25, duration: 150 }, { sprite: 26, duration: 150 }, { sprite: 27, duration: 150 }, { sprite: 28, duration: 240 }], loop: true },
  gameplay: { spriteFrames: [29, 30], frames: [{ sprite: 29, duration: 300 }, { sprite: 30, duration: 300 }], loop: true },
  gamewin: { spriteFrames: [31, 32, 33], frames: [{ sprite: 31, duration: 250 }, { sprite: 32, duration: 150 }, { sprite: 33, duration: 260 }], repeat: 5, fallback: 'idle' },
  gamelose: { spriteFrames: [34, 35], frames: [{ sprite: 34, duration: 360 }, { sprite: 35, duration: 360 }], repeat: 4, fallback: 'idle' },
  working: { spriteFrames: [21, 22, 23, 24], frames: [{ sprite: 21, duration: 170 }, { sprite: 22, duration: 170 }, { sprite: 23, duration: 170 }, { sprite: 24, duration: 240 }], loop: true },
  waiting: { spriteFrames: [19, 20], frames: [{ sprite: 19, duration: 620 }, { sprite: 20, duration: 620 }], loop: true },
  review: { spriteFrames: [16, 17, 18], frames: [{ sprite: 16, duration: 420 }, { sprite: 17, duration: 420 }, { sprite: 18, duration: 700 }], loop: true },
  failed: { spriteFrames: [34, 35], frames: [{ sprite: 34, duration: 220 }, { sprite: 35, duration: 220 }, { sprite: 34, duration: 520 }], repeat: 2, fallback: 'idle' },
};

const frameSpecs = [
  ...Array.from({ length: 7 }, (_, i) => ['idle', { bob: [0, -1, -2, -1, 0, 1, 0][i] }]),
  ...Array.from({ length: 4 }, (_, i) => ['idle', { bob: [0, -2, 0, 1][i], arm: [-2, 1, 2, -1][i], lean: [-2, 1, 2, -1][i] }]),
  ['waiting', { bob: 1, blink: true }],
  ['waiting', { bob: 0, blink: false }],
  ['idle', { bob: 0, scan: 4 }],
  ['idle', { bob: 0, scan: -4 }],
  ['idle', { bob: -1, pulse: true }],
  ['review', { bob: -1, mood: 'review' }],
  ['review', { bob: -2, mood: 'review', scan: 4 }],
  ['review', { bob: -1, mood: 'review', scan: -4 }],
  ['waiting', { bob: 0, blink: false }],
  ['waiting', { bob: 0, blink: true }],
  ...Array.from({ length: 4 }, (_, i) => ['working', { bob: [0, -1, 0, 1][i], scan: [0, 6, 12, 2][i] }]),
  ...Array.from({ length: 4 }, (_, i) => ['working', { bob: [-1, 0, 1, 0][i], scan: [0, 5, 10, 15][i], arm: [0, 2, 0, -2][i] }]),
  ['idle', { bob: -6, arm: 5 }],
  ['idle', { bob: -1, lean: 6, arm: -4 }],
  ['idle', { bob: 0, arm: -7 }],
  ['failed', { bob: 0, mood: 'failed' }],
  ['failed', { bob: 2, lean: -3, mood: 'failed' }],
  ['idle', { bob: -5 }],
  ['idle', { bob: -8, lean: 5 }],
  ['idle', { bob: -5 }],
  ['failed', { bob: 1, mood: 'failed' }],
  ['failed', { bob: -1, lean: 3, mood: 'failed' }],
  ['idle', { bob: -8 }],
  ['idle', { bob: 0, lean: 7 }],
  ['idle', { bob: 0, arm: -9 }],
  ['failed', { bob: 2, mood: 'failed' }],
  ['idle', { bob: 0, lean: -7 }],
];

function rgba(hex, alpha = 255) {
  const n = Number.parseInt(hex.replace('#', ''), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, alpha];
}

function colorPack(pack) {
  return Object.fromEntries(Object.entries(pack.colors).map(([key, value]) => [key, rgba(value, key === 'shadow' ? 76 : 255)]));
}

function createCanvas() {
  return new Uint8ClampedArray(sheetWidth * sheetHeight * 4);
}

function blendPixel(pixels, x, y, color) {
  if (x < 0 || y < 0 || x >= sheetWidth || y >= sheetHeight) return;
  const i = (Math.floor(y) * sheetWidth + Math.floor(x)) * 4;
  const a = color[3] / 255;
  const ia = 1 - a;
  pixels[i] = Math.round(color[0] * a + pixels[i] * ia);
  pixels[i + 1] = Math.round(color[1] * a + pixels[i + 1] * ia);
  pixels[i + 2] = Math.round(color[2] * a + pixels[i + 2] * ia);
  pixels[i + 3] = Math.round(255 * (a + (pixels[i + 3] / 255) * ia));
}

function frameOrigin(index) {
  return [(index % columns) * frameWidth, Math.floor(index / columns) * frameHeight];
}

function rect(pixels, index, x, y, w, h, color) {
  const [ox, oy] = frameOrigin(index);
  for (let yy = Math.floor(y); yy < Math.ceil(y + h); yy += 1) {
    for (let xx = Math.floor(x); xx < Math.ceil(x + w); xx += 1) blendPixel(pixels, ox + xx, oy + yy, color);
  }
}

function ellipse(pixels, index, cx, cy, rx, ry, color) {
  const [ox, oy] = frameOrigin(index);
  for (let y = Math.floor(cy - ry); y <= Math.ceil(cy + ry); y += 1) {
    for (let x = Math.floor(cx - rx); x <= Math.ceil(cx + rx); x += 1) {
      const dx = (x + 0.5 - cx) / rx;
      const dy = (y + 0.5 - cy) / ry;
      if (dx * dx + dy * dy <= 1) blendPixel(pixels, ox + x, oy + y, color);
    }
  }
}

function line(pixels, index, x0, y0, x1, y1, width, color) {
  const steps = Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0)) * 2;
  for (let i = 0; i <= steps; i += 1) {
    const t = steps === 0 ? 0 : i / steps;
    ellipse(pixels, index, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, width / 2, width / 2, color);
  }
}

function roundRect(pixels, index, x, y, w, h, r, color) {
  rect(pixels, index, x + r, y, w - r * 2, h, color);
  rect(pixels, index, x, y + r, r, h - r * 2, color);
  rect(pixels, index, x + w - r, y + r, r, h - r * 2, color);
  ellipse(pixels, index, x + r, y + r, r, r, color);
  ellipse(pixels, index, x + w - r, y + r, r, r, color);
  ellipse(pixels, index, x + r, y + h - r, r, r, color);
  ellipse(pixels, index, x + w - r, y + h - r, r, r, color);
}

function softEllipse(pixels, index, cx, cy, rx, ry, inner, outer) {
  const [ox, oy] = frameOrigin(index);
  for (let y = Math.floor(cy - ry); y <= Math.ceil(cy + ry); y += 1) {
    for (let x = Math.floor(cx - rx); x <= Math.ceil(cx + rx); x += 1) {
      const dx = (x + 0.5 - cx) / rx;
      const dy = (y + 0.5 - cy) / ry;
      const d = dx * dx + dy * dy;
      if (d > 1) continue;
      const t = Math.max(0, Math.min(1, Math.sqrt(d)));
      blendPixel(pixels, ox + x, oy + y, [
        Math.round(inner[0] * (1 - t) + outer[0] * t),
        Math.round(inner[1] * (1 - t) + outer[1] * t),
        Math.round(inner[2] * (1 - t) + outer[2] * t),
        Math.round(inner[3] * (1 - t) + outer[3] * t),
      ]);
    }
  }
}

function glow(pixels, index, cx, cy, rx, ry, color) {
  for (let i = 0; i < 5; i += 1) {
    ellipse(pixels, index, cx, cy, rx + i * 4, ry + i * 3, [color[0], color[1], color[2], Math.max(10, 48 - i * 8)]);
  }
  ellipse(pixels, index, cx, cy, rx, ry, color);
}

function star(pixels, index, cx, cy, r, color) {
  const points = [
    [cx, cy - r],
    [cx + r * 0.25, cy - r * 0.25],
    [cx + r, cy],
    [cx + r * 0.25, cy + r * 0.25],
    [cx, cy + r],
    [cx - r * 0.25, cy + r * 0.25],
    [cx - r, cy],
    [cx - r * 0.25, cy - r * 0.25],
  ];
  for (let i = 0; i < points.length; i += 1) {
    const [x0, y0] = points[i];
    const [x1, y1] = points[(i + 1) % points.length];
    line(pixels, index, x0, y0, x1, y1, 20, color);
  }
  ellipse(pixels, index, cx, cy, r * 0.45, r * 0.45, color);
}

function drawFace(pixels, index, C, mode, y, scan, blink) {
  roundRect(pixels, index, 50, y, 92, 57, 17, C.screen);
  roundRect(pixels, index, 55, y + 5, 82, 47, 13, C.screen2);
  rect(pixels, index, 61, y + 10, 70, 2, [255, 255, 255, 42]);
  if (mode === 'failed') {
    line(pixels, index, 77, y + 25, 88, y + 36, 5, C.warn);
    line(pixels, index, 88, y + 25, 77, y + 36, 5, C.warn);
    line(pixels, index, 104, y + 25, 115, y + 36, 5, C.warn);
    line(pixels, index, 115, y + 25, 104, y + 36, 5, C.warn);
    rect(pixels, index, 77, y + 45, 38, 4, C.warn);
  } else if (mode === 'review') {
    ellipse(pixels, index, 82, y + 32, 7, 7, C.ok);
    ellipse(pixels, index, 110, y + 32, 7, 7, C.alt);
    rect(pixels, index, 83, y + 45, 27, 4, C.accent);
  } else if (mode === 'working') {
    line(pixels, index, 75 + scan, y + 33, 87 + scan, y + 27, 5, C.accent);
    line(pixels, index, 87 + scan, y + 27, 99 + scan, y + 36, 5, C.accent);
    rect(pixels, index, 106, y + 37, 23, 5, C.accent);
    glow(pixels, index, 67 + scan, y + 15, 5, 2, C.accent);
  } else if (mode === 'waiting') {
    rect(pixels, index, 75, y + 34, 42, 5, blink ? C.accent : C.mid);
    rect(pixels, index, 121, y + 34, 8, 5, blink ? C.alt : C.mid);
  } else {
    line(pixels, index, 74, y + 31, 80, y + 37, 5, C.accent);
    line(pixels, index, 80, y + 37, 88, y + 29, 5, C.accent);
    line(pixels, index, 104, y + 31, 110, y + 37, 5, C.accent);
    line(pixels, index, 110, y + 37, 118, y + 29, 5, C.accent);
  }
}

function drawPet(pixels, pack, index, mode, opts) {
  const C = colorPack(pack);
  const bob = opts.bob || 0;
  const lean = opts.lean || 0;
  const arm = opts.arm || 0;
  const alert = opts.mood === 'failed' ? C.warn : opts.mood === 'review' ? C.ok : C.alt;

  ellipse(pixels, index, 96 + lean * 0.2, 182, 48, 10, C.shadow);
  if (pack.shape === 'ears') {
    ellipse(pixels, index, 62 + lean, 35 + bob, 14, 35, C.dark);
    ellipse(pixels, index, 130 + lean, 35 + bob, 14, 35, C.dark);
    ellipse(pixels, index, 62 + lean, 35 + bob, 7, 22, C.light);
    ellipse(pixels, index, 130 + lean, 35 + bob, 7, 22, C.light);
  } else if (pack.shape === 'antenna') {
    line(pixels, index, 73 + lean, 45 + bob, 63 + lean, 25 + bob, 4, C.rim);
    line(pixels, index, 119 + lean, 45 + bob, 130 + lean, 25 + bob, 4, C.rim);
    glow(pixels, index, 63 + lean, 25 + bob, 5, 5, C.accent);
    glow(pixels, index, 130 + lean, 25 + bob, 5, 5, alert);
  } else if (pack.shape === 'crescent') {
    ellipse(pixels, index, 137 + lean, 53 + bob, 17, 27, C.light);
    ellipse(pixels, index, 145 + lean, 49 + bob, 16, 25, [0, 0, 0, 0]);
    glow(pixels, index, 56 + lean, 36 + bob, 4, 4, C.alt);
  } else {
    star(pixels, index, 95 + lean, 38 + bob, 26, C.alt);
    glow(pixels, index, 95 + lean, 38 + bob, 5, 5, C.accent);
  }

  if (pack.shape === 'star') {
    star(pixels, index, 96 + lean, 96 + bob, 62, C.dark);
    star(pixels, index, 96 + lean, 94 + bob, 52, C.mid);
    ellipse(pixels, index, 96 + lean, 100 + bob, 52, 44, C.light);
  } else if (pack.shape === 'crescent') {
    softEllipse(pixels, index, 96 + lean, 84 + bob, 63, 60, C.light, C.dark);
    ellipse(pixels, index, 123 + lean, 77 + bob, 42, 55, [255, 255, 255, 58]);
  } else {
    softEllipse(pixels, index, 96 + lean, 84 + bob, 66, 59, C.light, C.dark);
    ellipse(pixels, index, 44 + lean, 83 + bob, 21, 31, C.mid);
    ellipse(pixels, index, 148 + lean, 83 + bob, 21, 31, C.mid);
  }

  drawFace(pixels, index, C, mode, 62 + bob, opts.scan || 0, opts.blink);
  roundRect(pixels, index, 62 + lean, 120 + bob, 68, 46, 14, C.dark);
  roundRect(pixels, index, 68 + lean, 125 + bob, 56, 32, 10, C.mid);
  ellipse(pixels, index, 79 + lean, 163 + bob, 12, 16, C.dark);
  ellipse(pixels, index, 113 + lean, 163 + bob, 12, 16, C.dark);
  ellipse(pixels, index, 45 + lean + arm, 141 + bob, 10, 27, C.dark);
  ellipse(pixels, index, 147 + lean - arm, 141 + bob, 10, 27, C.dark);

  if (mode === 'failed') {
    rect(pixels, index, 81 + lean, 145 + bob, 30, 5, C.warn);
  } else if (mode === 'review') {
    glow(pixels, index, 86 + lean, 145 + bob, 6, 4, C.ok);
    glow(pixels, index, 108 + lean, 145 + bob, 6, 4, C.alt);
  } else {
    line(pixels, index, 80 + lean, 146 + bob, 88 + lean, 154 + bob, 5, C.white);
    line(pixels, index, 88 + lean, 154 + bob, 101 + lean, 138 + bob, 5, C.white);
    rect(pixels, index, 105 + lean, 146 + bob, 17 + (opts.scan || 0), 4, C.accent);
  }
  rect(pixels, index, 63, 190, 66, 4, [8, 12, 22, 150]);
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

function pngBuffer(pixels) {
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

function writeManifest(pack, outDir) {
  const manifest = {
    schemaVersion: 2,
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
      frameWidth,
      frameHeight,
      columns,
      rows,
      frameCount,
      image: 'spritesheet.webp',
    },
    states,
    actions: {
      jump: { sprite: 36 },
      spin: { sprite: 37 },
      wave: { sprite: 38 },
      shake: { sprite: 39 },
      observe: { spriteFrames: [21, 22, 23, 24], frames: [{ sprite: 21, duration: 120 }, { sprite: 22, duration: 120 }, { sprite: 23, duration: 120 }, { sprite: 24, duration: 220 }], repeat: 1, fallback: 'idle' },
      nudge: { spriteFrames: [13, 14, 15], frames: [{ sprite: 13, duration: 150 }, { sprite: 14, duration: 180 }, { sprite: 15, duration: 180 }], repeat: 1, fallback: 'idle' },
      acknowledge: { spriteFrames: [16, 17, 18], frames: [{ sprite: 16, duration: 220 }, { sprite: 17, duration: 160 }, { sprite: 18, duration: 260 }], repeat: 1, fallback: 'idle' },
      blocked: { spriteFrames: [34, 35], frames: [{ sprite: 34, duration: 170 }, { sprite: 35, duration: 170 }, { sprite: 34, duration: 260 }], repeat: 1, fallback: 'idle' },
      dragging: { spriteFrames: [29, 30], frames: [{ sprite: 29, duration: 180 }, { sprite: 30, duration: 180 }], repeat: 2, fallback: 'idle' },
    },
    mini: {
      state: 'idle',
      frame: 0,
      headRows: 140,
    },
    id: pack.id,
    displayName: pack.displayName,
    description: pack.description,
    metadata: {
      generatedFrom: ['app/frontend/tools/generate-extra-pet-packs.cjs'],
      assetClass: 'character',
      qualityTier: 'generated',
      style: pack.style,
      recommendedUse: pack.recommendedUse,
      releaseTier: 'optional',
      optimizedFor: 'smooth-scaled desktop pet window',
      frameLabels: frameSpecs.slice(0, frameCount).map(([mode], index) => `${mode}:${index}`),
    },
  };
  writeFileSync(path.join(outDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
}

function convertPngToWebp(pngPath, webpPath) {
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
    webpPath,
  ], { stdio: 'inherit' });
}

for (const pack of packs) {
  const outDir = path.join(petsRoot, pack.id);
  mkdirSync(outDir, { recursive: true });
  const pixels = createCanvas();
  frameSpecs.forEach(([mode, opts], index) => drawPet(pixels, pack, index, mode, opts));
  const tempPng = path.join(outDir, 'spritesheet.tmp.png');
  const webp = path.join(outDir, 'spritesheet.webp');
  writeFileSync(tempPng, pngBuffer(pixels));
  convertPngToWebp(tempPng, webp);
  if (existsSync(tempPng)) rmSync(tempPng);
  writeManifest(pack, outDir);
  console.log(`generated ${pack.id}`);
}
