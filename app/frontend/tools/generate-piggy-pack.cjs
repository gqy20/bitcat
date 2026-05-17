const { writeFileSync } = require('node:fs');
const path = require('node:path');
const { deflateSync } = require('node:zlib');

const outDir = path.join(__dirname, '..', '__fixtures__', 'pets', 'piggy');
const frameWidth = 192;
const frameHeight = 208;
const columns = 8;
const rows = 5;
const frameCount = 40;
const sheetWidth = frameWidth * columns;
const sheetHeight = frameHeight * rows;

const pixels = new Uint8ClampedArray(sheetWidth * sheetHeight * 4);

const rgba = (hex, alpha = 255) => {
  const n = Number.parseInt(hex.replace('#', ''), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, alpha];
};

const C = {
  shadow: rgba('#070812', 72),
  rim: rgba('#43203f', 238),
  shellDark: rgba('#5d2b58', 255),
  shell: rgba('#b54d82', 255),
  shellLight: rgba('#f091b6', 255),
  blush: rgba('#ffb4c9', 240),
  screenDark: rgba('#11172e', 255),
  screenMid: rgba('#202b54', 255),
  cyan: rgba('#8df7f0', 255),
  cyanDim: rgba('#45bfc6', 230),
  green: rgba('#89f0a6', 255),
  magenta: rgba('#f058ff', 255),
  red: rgba('#ff5b72', 255),
  white: rgba('#fff2fb', 255),
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
    for (let xx = Math.floor(x); xx < Math.ceil(x + w); xx += 1) {
      blendPixel(ox + xx, oy + yy, color);
    }
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

function roundRect(fx, fy, x, y, w, h, r, color) {
  rect(fx, fy, x + r, y, w - r * 2, h, color);
  rect(fx, fy, x, y + r, r, h - r * 2, color);
  rect(fx, fy, x + w - r, y + r, r, h - r * 2, color);
  ellipse(fx, fy, x + r, y + r, r, r, color);
  ellipse(fx, fy, x + w - r, y + r, r, r, color);
  ellipse(fx, fy, x + r, y + h - r, r, r, color);
  ellipse(fx, fy, x + w - r, y + h - r, r, r, color);
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
        const color = [
          Math.round(inner[0] * (1 - t) + outer[0] * t),
          Math.round(inner[1] * (1 - t) + outer[1] * t),
          Math.round(inner[2] * (1 - t) + outer[2] * t),
          Math.round(inner[3] * (1 - t) + outer[3] * t),
        ];
        blendPixel(ox + x, oy + y, color);
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

function clearFrame(fx, fy) {
  rect(fx, fy, 0, 0, frameWidth, frameHeight, rgba('#000000', 0));
}

function drawChevron(fx, fy, x, y, size, color, flip = false) {
  const dir = flip ? -1 : 1;
  line(fx, fy, x - dir * size * 0.55, y - size * 0.6, x + dir * size * 0.25, y, 6, color);
  line(fx, fy, x + dir * size * 0.25, y, x - dir * size * 0.55, y + size * 0.6, 6, color);
}

function drawFace(fx, fy, cfg) {
  const y = cfg.y;
  roundRect(fx, fy, 48, y, 96, 60, 18, C.screenDark);
  roundRect(fx, fy, 53, y + 5, 86, 50, 14, C.screenMid);
  rect(fx, fy, 58, y + 10, 76, 2, rgba('#5368aa', 90));

  if (cfg.mode === 'failed') {
    line(fx, fy, 76, y + 25, 88, y + 37, 5, C.red);
    line(fx, fy, 88, y + 25, 76, y + 37, 5, C.red);
    line(fx, fy, 104, y + 25, 116, y + 37, 5, C.red);
    line(fx, fy, 116, y + 25, 104, y + 37, 5, C.red);
    rect(fx, fy, 76, y + 45, 40, 4, C.red);
    return;
  }

  if (cfg.mode === 'review') {
    ellipse(fx, fy, 81, y + 32, 8, 8, C.green);
    ellipse(fx, fy, 111, y + 32, 8, 8, C.magenta);
    rect(fx, fy, 82, y + 46, 28, 4, C.cyanDim);
    return;
  }

  if (cfg.mode === 'working') {
    drawChevron(fx, fy, 82 + cfg.scan, y + 33, 14, C.cyan);
    rect(fx, fy, 105, y + 37, 25, 5, C.cyan);
    glow(fx, fy, 67 + cfg.scan, y + 15, 5, 2, C.cyanDim);
    return;
  }

  if (cfg.mode === 'waiting') {
    rect(fx, fy, 75, y + 34, 42, 5, cfg.blink ? C.cyan : C.cyanDim);
    rect(fx, fy, 121, y + 34, 8, 5, cfg.blink ? C.magenta : C.cyanDim);
    return;
  }

  line(fx, fy, 74, y + 31, 80, y + 37, 5, C.cyan);
  line(fx, fy, 80, y + 37, 88, y + 29, 5, C.cyan);
  line(fx, fy, 104, y + 31, 110, y + 37, 5, C.cyan);
  line(fx, fy, 110, y + 37, 118, y + 29, 5, C.cyan);
}

function drawPiggy(index, mode = 'idle', opts = {}) {
  const fx = index % columns;
  const fy = Math.floor(index / columns);
  clearFrame(fx, fy);

  const bob = opts.bob || 0;
  const lean = opts.lean || 0;
  const faceY = 62 + bob;
  const bodyY = 119 + bob;
  const color = opts.alert === 'failed' ? C.red : opts.alert === 'review' ? C.green : C.magenta;

  ellipse(fx, fy, 96 + lean * 0.4, 181, 48, 10, C.shadow);
  line(fx, fy, 73 + lean, 43 + bob, 67 + lean, 27 + bob, 4, C.rim);
  line(fx, fy, 119 + lean, 43 + bob, 126 + lean, 27 + bob, 4, C.rim);
  glow(fx, fy, 65 + lean, 24 + bob, 4, 4, C.cyanDim);
  glow(fx, fy, 128 + lean, 24 + bob, 4, 4, color);

  softEllipse(fx, fy, 96 + lean, 82 + bob, 67, 60, C.shellLight, C.shellDark);
  ellipse(fx, fy, 44 + lean, 79 + bob, 22, 34, C.shell);
  ellipse(fx, fy, 148 + lean, 79 + bob, 22, 34, C.shell);
  ellipse(fx, fy, 54 + lean, 73 + bob, 12, 19, C.blush);
  ellipse(fx, fy, 138 + lean, 73 + bob, 12, 19, C.blush);

  drawFace(fx, fy, {
    y: faceY,
    mode,
    scan: opts.scan || 0,
    blink: opts.blink,
  });

  roundRect(fx, fy, 62 + lean, bodyY, 68, 47, 14, C.shellDark);
  roundRect(fx, fy, 68 + lean, bodyY + 5, 56, 33, 10, C.shell);
  ellipse(fx, fy, 79 + lean, bodyY + 42, 12, 17, C.shellDark);
  ellipse(fx, fy, 113 + lean, bodyY + 42, 12, 17, C.shellDark);
  ellipse(fx, fy, 45 + lean + (opts.arm || 0), bodyY + 21, 11, 28, C.shellDark);
  ellipse(fx, fy, 147 + lean - (opts.arm || 0), bodyY + 21, 11, 28, C.shellDark);

  if (mode === 'working') {
    drawChevron(fx, fy, 88 + lean, bodyY + 24, 10, C.white);
    rect(fx, fy, 100 + lean, bodyY + 24, 18 + (opts.scan || 0), 4, C.cyan);
  } else if (mode === 'failed') {
    rect(fx, fy, 81 + lean, bodyY + 25, 30, 5, C.red);
  } else if (mode === 'review') {
    glow(fx, fy, 86 + lean, bodyY + 25, 6, 4, C.green);
    glow(fx, fy, 108 + lean, bodyY + 25, 6, 4, C.magenta);
  } else {
    drawChevron(fx, fy, 86 + lean, bodyY + 25, 9, C.white);
    rect(fx, fy, 100 + lean, bodyY + 25, 18, 4, C.cyan);
  }

  rect(fx, fy, 63, 190, 66, 4, rgba('#2e1830', 170));
}

const frameSpecs = [
  ...Array.from({ length: 7 }, (_, i) => ['idle', { bob: [0, -1, -2, -1, 0, 1, 0][i] }]),
  ...Array.from({ length: 4 }, (_, i) => ['idle', { bob: [0, -2, 0, 1][i], arm: [-2, 1, 2, -1][i], lean: [-2, 1, 2, -1][i] }]),
  ['waiting', { bob: 1, blink: true }],
  ['waiting', { bob: 0, blink: false }],
  ['idle', { bob: 0 }],
  ['idle', { bob: 0, scan: 4 }],
  ['idle', { bob: 0, scan: -4 }],
  ['review', { bob: -1, alert: 'review' }],
  ['review', { bob: -2, alert: 'review', scan: 4 }],
  ['review', { bob: -1, alert: 'review', scan: -4 }],
  ['waiting', { bob: 0, blink: false }],
  ['waiting', { bob: 0, blink: true }],
  ...Array.from({ length: 4 }, (_, i) => ['working', { bob: [0, -1, 0, 1][i], scan: [0, 6, 12, 2][i] }]),
  ...Array.from({ length: 4 }, (_, i) => ['working', { bob: [-1, 0, 1, 0][i], scan: [0, 5, 10, 15][i], arm: [0, 2, 0, -2][i] }]),
  ['idle', { bob: -6, arm: 5 }],
  ['idle', { bob: -1, lean: 6, arm: -4 }],
  ['idle', { bob: 0, arm: -7 }],
  ['failed', { bob: 0, alert: 'failed' }],
  ['failed', { bob: 2, lean: -3, alert: 'failed' }],
  ['idle', { bob: -5 }],
  ['idle', { bob: -8, lean: 5 }],
  ['idle', { bob: -5 }],
  ['failed', { bob: 1, alert: 'failed' }],
  ['failed', { bob: -1, lean: 3, alert: 'failed' }],
  ['idle', { bob: -8 }],
  ['idle', { bob: 0, lean: 7 }],
  ['idle', { bob: 0, arm: -9 }],
  ['failed', { bob: 2, alert: 'failed' }],
  ['idle', { bob: 0, lean: -7 }],
];

frameSpecs.forEach(([mode, opts], index) => drawPiggy(index, mode, opts));

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
      { name: 'antenna_ping', weight: 3, cooldownMinMs: 9000, cooldownMaxMs: 18000, frames: [{ sprite: 13, duration: 160 }, { sprite: 0, duration: 260 }] },
      { name: 'terminal_glance', weight: 2, cooldownMinMs: 12000, cooldownMaxMs: 24000, frames: [{ sprite: 14, duration: 300 }, { sprite: 15, duration: 300 }, { sprite: 0, duration: 260 }] },
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

const manifest = {
  schemaVersion: 2,
  id: 'piggy',
  displayName: 'Piggy',
  description: 'A high-resolution terminal pig companion for status-driven desktop feedback.',
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
    image: 'sprites.png',
    frameWidth,
    frameHeight,
    columns,
    rows,
    frameCount,
  },
  states,
  actions: {
    jump: { sprite: 36 },
    spin: { sprite: 37 },
    wave: { sprite: 38 },
    shake: { sprite: 39 },
    observe: {
      spriteFrames: [21, 22, 23, 24],
      frames: [{ sprite: 21, duration: 120 }, { sprite: 22, duration: 120 }, { sprite: 23, duration: 120 }, { sprite: 24, duration: 220 }],
      repeat: 1,
      fallback: 'idle',
    },
    nudge: {
      spriteFrames: [13, 14, 15],
      frames: [{ sprite: 13, duration: 150 }, { sprite: 14, duration: 180 }, { sprite: 15, duration: 180 }],
      repeat: 1,
      fallback: 'idle',
    },
    acknowledge: {
      spriteFrames: [16, 17, 18],
      frames: [{ sprite: 16, duration: 220 }, { sprite: 17, duration: 160 }, { sprite: 18, duration: 260 }],
      repeat: 1,
      fallback: 'idle',
    },
    blocked: {
      spriteFrames: [34, 35],
      frames: [{ sprite: 34, duration: 170 }, { sprite: 35, duration: 170 }, { sprite: 34, duration: 260 }],
      repeat: 1,
      fallback: 'idle',
    },
    dragging: {
      spriteFrames: [29, 30],
      frames: [{ sprite: 29, duration: 180 }, { sprite: 30, duration: 180 }],
      repeat: 2,
      fallback: 'idle',
    },
  },
  mini: {
    state: 'idle',
    frame: 0,
    headRows: 140,
  },
  metadata: {
    generatedFrom: ['app/frontend/tools/generate-piggy-pack.cjs'],
    assetClass: 'default-companion',
    qualityTier: 'polished',
    style: 'terminal-status pig companion',
    recommendedUse: 'default pet and status feedback',
    releaseTier: 'builtin',
    optimizedFor: 'smooth-scaled desktop pet window',
    frameLabels: frameSpecs.slice(0, frameCount).map(([mode], index) => `${mode}:${index}`),
  },
};

writeFileSync(path.join(outDir, 'sprites.png'), pngBuffer());
writeFileSync(path.join(outDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
