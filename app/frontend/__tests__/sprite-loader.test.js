import { inflateSync } from 'node:zlib';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  buildRuntimeFromManifest,
  loadPetAssetPack,
  validateManifest,
} from '../js/sprite-loader.js';

const fixtureDir = path.join(process.cwd(), '__fixtures__', 'pets', 'default-cat');
const manifest = JSON.parse(readFileSync(path.join(fixtureDir, 'manifest.json'), 'utf8'));

function readChunks(bytes) {
  const chunks = [];
  let offset = 8;
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.toString('ascii', offset + 4, offset + 8);
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    chunks.push({ type, data });
    offset += 12 + length;
    if (type === 'IEND') break;
  }
  return chunks;
}

function decodeFixturePng(filePath) {
  const bytes = readFileSync(filePath);
  const chunks = readChunks(bytes);
  const ihdr = chunks.find((chunk) => chunk.type === 'IHDR').data;
  const width = ihdr.readUInt32BE(0);
  const height = ihdr.readUInt32BE(4);
  const compressed = Buffer.concat(chunks.filter((chunk) => chunk.type === 'IDAT').map((chunk) => chunk.data));
  const scanlines = inflateSync(compressed);
  const stride = width * 4;
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    const rowOffset = y * (stride + 1);
    expect(scanlines[rowOffset]).toBe(0);
    data.set(scanlines.subarray(rowOffset + 1, rowOffset + 1 + stride), y * stride);
  }
  return { width, height, data };
}

describe('sprite-loader', () => {
  it('validates manifest basics', () => {
    expect(validateManifest(manifest)).toBe(manifest);
    expect(() => validateManifest({ ...manifest, schemaVersion: 2 })).toThrow(/schemaVersion/);
    expect(() => validateManifest({
      ...manifest,
      states: { ...manifest.states, idle: undefined },
    })).toThrow(/missing required state/);
    expect(() => validateManifest({
      ...manifest,
      states: { ...manifest.states, focused: undefined },
    })).toThrow(/missing required state/);
  });

  it('builds a runtime renderer model from the default-cat fixture', () => {
    const imageData = decodeFixturePng(path.join(fixtureDir, 'sprites.png'));
    const runtime = buildRuntimeFromManifest(manifest, imageData);

    expect(runtime.sprites.idle).toHaveLength(7);
    expect(runtime.sprites.focused).toHaveLength(4);
    expect(runtime.sprites.preparing).toHaveLength(4);
    expect(runtime.sprites.jump).toHaveLength(1);
    expect(runtime.palette[0]).toBeNull();
    expect(runtime.sprites.idle[0][6 * 16 + 3]).toBe(4);
    expect(runtime.stateConfig.focused.frames.map((frame) => frame.sprite)).toEqual([0, 1, 0, 2]);
    expect(runtime.stateConfig.idle.variants[0].frames.map((frame) => frame.sprite)).toEqual([4, 0]);
  });

  it('rejects timelines that reference frames outside their local spriteFrames', () => {
    const imageData = decodeFixturePng(path.join(fixtureDir, 'sprites.png'));
    const broken = {
      ...manifest,
      states: {
        ...manifest.states,
        focused: {
          ...manifest.states.focused,
          spriteFrames: [21, 22],
          frames: [{ sprite: 23, duration: 100 }],
        },
      },
    };

    expect(() => buildRuntimeFromManifest(broken, imageData)).toThrow(/outside focused\.spriteFrames/);
  });

  it('loads a configured pack through injected fetch and imageData hooks', async () => {
    const imageData = decodeFixturePng(path.join(fixtureDir, 'sprites.png'));
    const renderer = await loadPetAssetPack('/fixtures/default-cat', {
      fetch: async (url) => ({
        ok: url.endsWith('/manifest.json'),
        status: 200,
        json: async () => manifest,
      }),
      imageData: async () => imageData,
    });

    expect(renderer.assetSource.kind).toBe('manifest');
    expect(renderer.assetSource.id).toBe('default-cat');
    expect(renderer.getSprite('focused', 0)).toBe(renderer.SPRITES.focused[0]);
    expect(renderer.getSprite('unknown', 0)).toBe(renderer.SPRITES.idle[0]);
  });

  it('falls back to builtin sprites when external loading fails', async () => {
    const renderer = await loadPetAssetPack('/broken', {
      fetch: async () => ({ ok: false, status: 404 }),
    });

    expect(renderer.assetSource.kind).toBe('builtin');
    expect(renderer.SPRITES.idle.length).toBeGreaterThan(0);
  });
});
