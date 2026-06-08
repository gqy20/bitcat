import { readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { loadPetAssetPack } from '../js/sprite-loader.js';

const packs = [
  { id: 'cat-tabby', dir: 'cat-tabby', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-calico', dir: 'cat-calico', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-siamese', dir: 'cat-siamese', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-tuxedo', dir: 'cat-tuxedo', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-black', dir: 'cat-black', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-white', dir: 'cat-white', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-ginger', dir: 'cat-ginger', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-gray', dir: 'cat-gray', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-cream', dir: 'cat-cream', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-blue-gray', dir: 'cat-blue-gray', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-cow', dir: 'cat-cow', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-tortie', dir: 'cat-tortie', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-ragdoll', dir: 'cat-ragdoll', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-snowshoe', dir: 'cat-snowshoe', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
  { id: 'cat-lilac', dir: 'cat-lilac', image: 'spritesheet.webp', minSize: 100_000, displayWidth: 74, displayHeight: 80, qualityTier: 'polished', assetClass: 'default-companion' },
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
