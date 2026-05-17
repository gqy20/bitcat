import { describe, expect, it } from 'vitest';
import { readFileSync, statSync } from 'node:fs';
import path from 'node:path';

const fixtureDir = path.join(process.cwd(), '__fixtures__', 'pets', 'status');
const manifest = JSON.parse(readFileSync(path.join(fixtureDir, 'manifest.json'), 'utf8'));

describe('status pet fixture pack', () => {
  it('declares the 192x208 manifest v2 sprite sheet', () => {
    expect(manifest.schemaVersion).toBe(2);
    expect(manifest.id).toBe('status');
    expect(manifest.sprite).toMatchObject({
      image: 'spritesheet.webp',
      frameWidth: 192,
      frameHeight: 208,
      columns: 8,
      rows: 9,
      frameCount: 72,
    });
    expect(manifest.render).toMatchObject({
      mode: 'sheet',
      displayWidth: 69,
      displayHeight: 75,
      scale: 75 / 208,
      pixelated: false,
    });
    expect(manifest.hotspots.observe).toMatchObject({
      x: 0.18,
      y: 0.10,
      w: 0.64,
      h: 0.40,
    });
    expect(manifest.hotspots.input).toMatchObject({
      x: 0.22,
      y: 0.38,
      w: 0.56,
      h: 0.34,
    });
  });

  it('keeps legacy visual states mapped to status states', () => {
    expect(Object.keys(manifest.states)).toEqual([
      'idle',
      'working',
      'waiting',
      'review',
      'failed',
    ]);
    expect(manifest.aliases.focused).toBe('working');
    expect(manifest.aliases.preparing).toBe('working');
    expect(manifest.aliases.confused).toBe('failed');
    expect(manifest.aliases.gamewin).toBe('review');
  });

  it('includes the downloaded WebP spritesheet', () => {
    const bytes = readFileSync(path.join(fixtureDir, manifest.sprite.image));
    expect(bytes.toString('ascii', 0, 4)).toBe('RIFF');
    expect(bytes.toString('ascii', 8, 12)).toBe('WEBP');
    expect(statSync(path.join(fixtureDir, manifest.sprite.image)).size).toBeGreaterThan(100_000);
  });
});
