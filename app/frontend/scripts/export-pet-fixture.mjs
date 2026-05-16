import { deflateSync } from 'node:zlib';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const frontendDir = path.resolve(__dirname, '..');
const outputDir = path.join(frontendDir, '__fixtures__', 'pets', 'default-cat');

const stateOrder = [
  'idle',
  'walk',
  'sleep',
  'talk',
  'happy',
  'confused',
  'focused',
  'preparing',
  'gameplay',
  'gamewin',
  'gamelose',
];

const actionOrder = ['jump', 'spin', 'wave', 'shake'];

async function importFrontendModule(relativePath) {
  const sourcePath = path.join(frontendDir, relativePath);
  const source = await readFile(sourcePath, 'utf8');
  const url = `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`;
  return import(url);
}

function assertFrame(sprite, frame, label) {
  if (!Array.isArray(frame) || frame.length !== sprite.SPRITE_W * sprite.SPRITE_H) {
    throw new Error(`${label} is not a ${sprite.SPRITE_W}x${sprite.SPRITE_H} frame`);
  }
}

function rgbaForIndex(palette, index) {
  const color = palette[index];
  if (color == null) {
    return [0, 0, 0, 0];
  }
  if (!Array.isArray(color) || color.length !== 4) {
    throw new Error(`palette index ${index} is not RGBA`);
  }
  return color;
}

function writeU32(buffer, offset, value) {
  buffer.writeUInt32BE(value >>> 0, offset);
}

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let i = 0; i < 8; i += 1) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBuffer = Buffer.from(type, 'ascii');
  const chunk = Buffer.alloc(12 + data.length);
  writeU32(chunk, 0, data.length);
  typeBuffer.copy(chunk, 4);
  data.copy(chunk, 8);
  writeU32(chunk, 8 + data.length, crc32(Buffer.concat([typeBuffer, data])));
  return chunk;
}

function encodePng({ width, height, pixels }) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  writeU32(ihdr, 0, width);
  writeU32(ihdr, 4, height);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const stride = width * 4;
  const scanlines = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y += 1) {
    const rowOffset = y * (stride + 1);
    scanlines[rowOffset] = 0; // no filter
    pixels.copy(scanlines, rowOffset + 1, y * stride, (y + 1) * stride);
  }

  return Buffer.concat([
    signature,
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', deflateSync(scanlines)),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function cloneTimelineConfig(config, resolveFrame) {
  const out = {};
  for (const [key, value] of Object.entries(config)) {
    if (key === 'frames') {
      out.frames = value.map((frame) => ({
        ...frame,
        sprite: resolveFrame(frame.sprite),
      }));
    } else if (key === 'variants') {
      out.variants = value.map((variant) => ({
        ...variant,
        frames: variant.frames.map((frame) => ({
          ...frame,
          sprite: resolveFrame(frame.sprite),
        })),
      }));
    } else {
      out[key] = value;
    }
  }
  return out;
}

function placeFrame(pixels, sheet, frameIndex, frame) {
  const { frameWidth, frameHeight, columns, palette } = sheet;
  const dstX = (frameIndex % columns) * frameWidth;
  const dstY = Math.floor(frameIndex / columns) * frameHeight;
  for (let y = 0; y < frameHeight; y += 1) {
    for (let x = 0; x < frameWidth; x += 1) {
      const [r, g, b, a] = rgbaForIndex(palette, frame[y * frameWidth + x]);
      const offset = ((dstY + y) * sheet.width + dstX + x) * 4;
      pixels[offset] = r;
      pixels[offset + 1] = g;
      pixels[offset + 2] = b;
      pixels[offset + 3] = a;
    }
  }
}

async function main() {
  const sprite = await importFrontendModule('js/sprite.js');
  const pet = await importFrontendModule('js/pet.js');
  const frames = [];
  const frameByState = new Map();

  for (const state of stateOrder) {
    const stateFrames = sprite.SPRITES[state];
    if (!stateFrames) {
      throw new Error(`missing SPRITES.${state}`);
    }
    const globalFrames = [];
    stateFrames.forEach((frame, index) => {
      assertFrame(sprite, frame, `${state}[${index}]`);
      globalFrames.push(frames.length);
      frames.push({ label: `${state}:${index}`, pixels: frame });
    });
    frameByState.set(state, globalFrames);
  }

  const actions = {};
  for (const action of actionOrder) {
    const actionFrames = sprite.SPRITES[action];
    if (!actionFrames || actionFrames.length < 1) {
      throw new Error(`missing SPRITES.${action}`);
    }
    assertFrame(sprite, actionFrames[0], `${action}[0]`);
    actions[action] = { sprite: frames.length };
    frames.push({ label: `${action}:0`, pixels: actionFrames[0] });
  }

  const states = {};
  for (const state of stateOrder) {
    const config = pet.STATE_CONFIG[state];
    if (!config) {
      throw new Error(`missing STATE_CONFIG.${state}`);
    }
    const stateFrames = frameByState.get(state);
    states[state] = {
      spriteFrames: frameByState.get(state).slice(),
      ...cloneTimelineConfig(config, (localIndex) => {
      const globalIndex = stateFrames[localIndex];
      if (globalIndex == null) {
        throw new Error(`${state} references missing local sprite ${localIndex}`);
      }
      return globalIndex;
      }),
    };
  }

  const columns = 8;
  const rows = Math.ceil(frames.length / columns);
  const width = columns * sprite.SPRITE_W;
  const height = rows * sprite.SPRITE_H;
  const pixels = Buffer.alloc(width * height * 4);
  const sheet = {
    frameWidth: sprite.SPRITE_W,
    frameHeight: sprite.SPRITE_H,
    columns,
    width,
    palette: sprite.PALETTE,
  };

  frames.forEach((frame, index) => placeFrame(pixels, sheet, index, frame.pixels));

  const manifest = {
    schemaVersion: 1,
    id: 'default-cat',
    displayName: 'Default Cat',
    description: 'The built-in 8-bit cat exported as a manifest-compatible fixture pack.',
    sprite: {
      image: 'sprites.png',
      frameWidth: sprite.SPRITE_W,
      frameHeight: sprite.SPRITE_H,
      columns,
      rows,
      frameCount: frames.length,
    },
    palette: Object.fromEntries(
      Object.entries(sprite.PALETTE).map(([key, value]) => [key, value])
    ),
    states,
    actions,
    mini: {
      state: 'idle',
      frame: frameByState.get('idle')[0],
      headRows: 10,
    },
    metadata: {
      generatedFrom: [
        'app/frontend/js/sprite.js',
        'app/frontend/js/pet.js',
      ],
      frameLabels: frames.map((frame) => frame.label),
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(path.join(outputDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  await writeFile(path.join(outputDir, 'sprites.png'), encodePng({ width, height, pixels }));

  console.log(`Exported ${frames.length} frames to ${pathToFileURL(outputDir).href}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
