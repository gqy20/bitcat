import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { PetStateMachine } from '../js/pet.js';
import { buildRuntimeFromManifest } from '../js/sprite-loader.js';
import { previewFrame } from '../tools/pixel-preview-playback.js';

function pet() {
  const manifest = JSON.parse(readFileSync(path.join(process.cwd(), '__fixtures__/pets/cat-pixel-tuxedo/manifest.json'), 'utf8'));
  const runtime = buildRuntimeFromManifest(manifest, {
    width: manifest.sprite.columns * 96, height: manifest.sprite.rows * 96,
  });
  return new PetStateMachine({ stateConfig: runtime.stateConfig, actionConfig: runtime.actionConfig });
}
const duration = action => action.frames.reduce((sum, f) => sum + f.duration, 0);

describe('pixel preview reduced motion', () => {
  it.each([false, true])('advances gait while the requested walk moves, reduced=%s', reduced => {
    const p = pet(), start = p.x;
    p.walkTo(start + 100);
    p.update(duration(p.actionConfig.rise));
    const frames = new Set();
    for (let i = 0; i < 16; i++) {
      p.update(80);
      frames.add(previewFrame(p, reduced));
    }
    expect(p.x).toBeGreaterThan(start);
    expect(frames.size).toBe(16);
  });

  it('preserves rise and sit frames with reduced motion enabled', () => {
    const p = pet();
    p.walkTo(p.x + 7);
    p.update(200);
    expect(p.visualState()).toBe('rise');
    expect(previewFrame(p, true)).toBeGreaterThan(0);
    p.update(duration(p.actionConfig.rise) - 200 + 400);
    expect(p.visualState()).toBe('sit');
    p.update(200);
    expect(previewFrame(p, true)).toBeGreaterThan(0);
  });

  it('still suppresses automatic idle blinking under reduced motion', () => {
    const p = pet();
    p.update(3101);
    expect(p.frame).toBe(1);
    expect(previewFrame(p, true)).toBe(0);
    expect(previewFrame(p, false)).toBe(1);
  });
});
