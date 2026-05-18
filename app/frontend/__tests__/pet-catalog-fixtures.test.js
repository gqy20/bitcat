import { readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { loadPetAssetPack } from '../js/sprite-loader.js';

const packs = [
  { id: 'piggy', dir: 'piggy', image: 'spritesheet.webp', minSize: 50_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat', dir: 'cat', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'core', dir: 'core', image: 'spritesheet.webp', qualityTier: 'polished', assetClass: 'terminal-status' },
  { id: 'status', dir: 'status', image: 'spritesheet.webp', qualityTier: 'polished', assetClass: 'terminal-status' },
  { id: 'dewey', dir: 'dewey', image: 'spritesheet.webp', qualityTier: 'standard', assetClass: 'character' },
  { id: 'fireball', dir: 'fireball', image: 'spritesheet.webp', qualityTier: 'standard', assetClass: 'character' },
  { id: 'rocky', dir: 'rocky', image: 'spritesheet.webp', qualityTier: 'standard', assetClass: 'character' },
  { id: 'seedy', dir: 'seedy', image: 'spritesheet.webp', qualityTier: 'standard', assetClass: 'character' },
  { id: 'byte-bun', dir: 'byte-bun', image: 'spritesheet.webp', qualityTier: 'generated', assetClass: 'character', displayWidth: 74, displayHeight: 80 },
  { id: 'mossbot', dir: 'mossbot', image: 'spritesheet.webp', qualityTier: 'generated', assetClass: 'character', displayWidth: 74, displayHeight: 80 },
  { id: 'moonbit', dir: 'moonbit', image: 'spritesheet.webp', qualityTier: 'generated', assetClass: 'character', displayWidth: 74, displayHeight: 80 },
  { id: 'sparkle', dir: 'sparkle', image: 'spritesheet.webp', qualityTier: 'generated', assetClass: 'character', displayWidth: 74, displayHeight: 80 },
  { id: 'stacky', dir: 'stacky', image: 'spritesheet.webp', qualityTier: 'polished', assetClass: 'terminal-status' },
  { id: 'bsod', dir: 'bsod', image: 'spritesheet.webp', qualityTier: 'polished', assetClass: 'terminal-status' },
  { id: 'null-signal', dir: 'null-signal', image: 'spritesheet.webp', qualityTier: 'polished', assetClass: 'terminal-status' },
];

const fixturesRoot = path.join(process.cwd(), '__fixtures__', 'pets');

function loadManifest(pack) {
  return JSON.parse(readFileSync(path.join(fixturesRoot, pack.dir, 'manifest.json'), 'utf8'));
}

function zeroImageData(manifest) {
  return {
    width: manifest.sprite.columns * manifest.sprite.frameWidth,
    height: manifest.sprite.rows * manifest.sprite.frameHeight,
    data: new Uint8ClampedArray(
      manifest.sprite.columns * manifest.sprite.frameWidth * manifest.sprite.rows * manifest.sprite.frameHeight * 4,
    ),
  };
}

describe('pet catalog fixtures', () => {
  it('ships the catalog as manifest-backed fixture packs', () => {
    for (const pack of packs) {
      const manifest = loadManifest(pack);
      const imagePath = path.join(fixturesRoot, pack.dir, pack.image);
      expect(manifest.id).toBe(pack.id);
      expect(manifest.sprite.image).toBe(pack.image);
      expect(statSync(imagePath).size).toBeGreaterThan(pack.minSize ?? 100_000);
      expect(manifest.metadata).toMatchObject({
        qualityTier: pack.qualityTier,
        assetClass: pack.assetClass,
      });
      expect(typeof manifest.metadata.style).toBe('string');
      expect(typeof manifest.metadata.recommendedUse).toBe('string');
      expect(typeof manifest.metadata.releaseTier).toBe('string');
      expect(typeof manifest.metadata.optimizedFor).toBe('string');
    }
  });

  it('loads every catalog pack through the manifest loader', async () => {
    for (const pack of packs) {
      const manifest = loadManifest(pack);
      const renderer = await loadPetAssetPack(`/fixtures/${pack.dir}`, {
        fetch: async (url) => ({
          ok: url.endsWith('/manifest.json'),
          status: 200,
          json: async () => manifest,
        }),
        imageData: async () => zeroImageData(manifest),
      });

      expect(renderer.assetSource.id).toBe(pack.id);
      expect(renderer.displayWidth).toBe(pack.displayWidth ?? 69);
      expect(renderer.displayHeight).toBe(pack.displayHeight ?? 75);
    }
  });
});
