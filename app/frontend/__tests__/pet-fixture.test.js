import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';

const fixtureDir = path.join(process.cwd(), '__fixtures__', 'pets', 'cat');
const manifest = JSON.parse(readFileSync(path.join(fixtureDir, 'manifest.json'), 'utf8'));

const requiredStates = [
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
const actions = [
  'jump',
  'spin',
  'wave',
  'shake',
  'observe',
  'nudge',
  'acknowledge',
  'blocked',
  'dragging',
];

function collectSpriteRefsFromTimeline(timeline) {
  const refs = [
    ...timeline.frames.map((frame) => frame.sprite),
    ...(timeline.spriteFrames || []),
  ];
  for (const variant of timeline.variants || []) {
    refs.push(...variant.frames.map((frame) => frame.sprite));
  }
  return refs;
}

function readWebpDimensions(bytes) {
  expect(bytes.toString('ascii', 0, 4)).toBe('RIFF');
  expect(bytes.toString('ascii', 8, 12)).toBe('WEBP');

  let offset = 12;
  while (offset + 8 <= bytes.length) {
    const chunk = bytes.toString('ascii', offset, offset + 4);
    const size = bytes.readUInt32LE(offset + 4);
    const dataOffset = offset + 8;
    if (chunk === 'VP8X') {
      return {
        width: 1 + bytes.readUIntLE(dataOffset + 4, 3),
        height: 1 + bytes.readUIntLE(dataOffset + 7, 3),
      };
    }
    if (chunk === 'VP8 ') {
      return {
        width: bytes.readUInt16LE(dataOffset + 6) & 0x3fff,
        height: bytes.readUInt16LE(dataOffset + 8) & 0x3fff,
      };
    }
    if (chunk === 'VP8L') {
      const b0 = bytes[dataOffset + 1];
      const b1 = bytes[dataOffset + 2];
      const b2 = bytes[dataOffset + 3];
      const b3 = bytes[dataOffset + 4];
      return {
        width: 1 + (((b1 & 0x3f) << 8) | b0),
        height: 1 + (((b3 & 0x0f) << 10) | (b2 << 2) | ((b1 & 0xc0) >> 6)),
      };
    }
    offset = dataOffset + size + (size % 2);
  }
  throw new Error('unable to read WebP dimensions');
}

describe('cat pet fixture pack', () => {
  it('declares the manifest v2 sprite sheet shape', () => {
    expect(manifest.schemaVersion).toBe(2);
    expect(manifest.id).toBe('cat');
    expect(manifest.sprite).toEqual({
      image: 'spritesheet.webp',
      frameWidth: 192,
      frameHeight: 208,
      columns: 8,
      rows: 8,
      frameCount: 64,
    });
    expect(manifest.render).toMatchObject({
      mode: 'sheet',
      displayWidth: 74,
      displayHeight: 80,
      pixelated: false,
    });
    expect(manifest.render.scale).toBeCloseTo(80 / 208);
    expect(manifest.hotspots.observe).toEqual({
      x: 0.2,
      y: 0.12,
      w: 0.6,
      h: 0.38,
    });
    expect(manifest.hotspots.input).toEqual({
      x: 0.24,
      y: 0.42,
      w: 0.52,
      h: 0.32,
    });
  });

  it('covers every visual state the pet window can enter', () => {
    for (const state of requiredStates) {
      expect(manifest.states[state]).toBeTruthy();
      expect(manifest.states[state].frames.length).toBeGreaterThan(0);
    }
  });

  it('keeps every sprite reference inside frameCount', () => {
    const refs = [];
    for (const timeline of Object.values(manifest.states)) {
      refs.push(...collectSpriteRefsFromTimeline(timeline));
    }
    for (const action of actions) {
      refs.push(...collectSpriteRefsFromTimeline(manifest.actions[action]));
    }

    for (const ref of refs) {
      expect(Number.isInteger(ref)).toBe(true);
      expect(ref).toBeGreaterThanOrEqual(0);
      expect(ref).toBeLessThan(manifest.sprite.frameCount);
    }
  });

  it('ships the full v2 status and interaction vocabulary', () => {
    expect(Object.keys(manifest.actions).sort()).toEqual([...actions].sort());
    expect(manifest.states.working.frames).toHaveLength(6);
    expect(manifest.states.waiting.frames).toHaveLength(2);
    expect(manifest.states.review.frames).toHaveLength(5);
    expect(manifest.states.failed.frames).toHaveLength(5);
    expect(manifest.actions.observe.frames.map((frame) => frame.sprite)).toEqual([40, 41, 42, 44, 43, 45]);
    expect(manifest.actions.dragging.frames.map((frame) => frame.sprite)).toEqual([58, 59, 60, 61]);
    expect(manifest.metadata).toMatchObject({
      qualityTier: 'polished',
      assetClass: 'default-companion',
      releaseTier: 'builtin',
    });
  });

  it('keeps timeline durations positive', () => {
    for (const timeline of Object.values(manifest.states)) {
      for (const frame of timeline.frames) {
        expect(frame.duration).toBeGreaterThan(0);
      }
      for (const variant of timeline.variants || []) {
        expect(variant.cooldownMaxMs).toBeGreaterThanOrEqual(variant.cooldownMinMs);
        for (const frame of variant.frames) {
          expect(frame.duration).toBeGreaterThan(0);
        }
      }
    }
  });

  it('writes a WebP matching manifest dimensions', () => {
    const imagePath = path.join(fixtureDir, manifest.sprite.image);
    const dimensions = readWebpDimensions(readFileSync(imagePath));
    expect(dimensions).toEqual({
      width: manifest.sprite.columns * manifest.sprite.frameWidth,
      height: manifest.sprite.rows * manifest.sprite.frameHeight,
    });
  });
});
