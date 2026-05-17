import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';

const fixtureDir = path.join(process.cwd(), '__fixtures__', 'pets', 'piggy');
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
const actions = ['jump', 'spin', 'wave', 'shake'];

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

function readPngDimensions(bytes) {
  const signature = [137, 80, 78, 71, 13, 10, 26, 10];
  for (let i = 0; i < signature.length; i += 1) {
    expect(bytes[i]).toBe(signature[i]);
  }
  expect(bytes.toString('ascii', 12, 16)).toBe('IHDR');
  return {
    width: bytes.readUInt32BE(16),
    height: bytes.readUInt32BE(20),
  };
}

describe('piggy pet fixture pack', () => {
  it('declares the manifest v2 sprite sheet shape', () => {
    expect(manifest.schemaVersion).toBe(2);
    expect(manifest.id).toBe('piggy');
    expect(manifest.sprite).toEqual({
      image: 'sprites.png',
      frameWidth: 16,
      frameHeight: 16,
      columns: 8,
      rows: 5,
      frameCount: 40,
    });
    expect(manifest.render).toMatchObject({
      mode: 'sheet',
      displayWidth: 128,
      displayHeight: 128,
      scale: 8,
      pixelated: true,
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
      refs.push(manifest.actions[action].sprite);
    }

    for (const ref of refs) {
      expect(Number.isInteger(ref)).toBe(true);
      expect(ref).toBeGreaterThanOrEqual(0);
      expect(ref).toBeLessThan(manifest.sprite.frameCount);
    }
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

  it('writes a PNG matching manifest dimensions', () => {
    const pngPath = path.join(fixtureDir, manifest.sprite.image);
    const dimensions = readPngDimensions(readFileSync(pngPath));
    expect(dimensions).toEqual({
      width: manifest.sprite.columns * manifest.sprite.frameWidth,
      height: manifest.sprite.rows * manifest.sprite.frameHeight,
    });
  });
});
