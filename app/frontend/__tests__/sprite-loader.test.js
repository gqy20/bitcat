import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_PET_ASSET_URL,
  buildRuntimeFromManifest,
  configuredPetAssetUrlAsync,
  loadPetAssetPack,
  validateManifest,
} from '../js/sprite-loader.js';

const fixtureDir = path.join(process.cwd(), '__fixtures__', 'pets', 'cat-tabby');
const manifest = JSON.parse(readFileSync(path.join(fixtureDir, 'manifest.json'), 'utf8'));
const compactManifest = {
  ...manifest,
  id: 'cat-compact-test',
  render: {
    ...manifest.render,
    displayWidth: 69,
    displayHeight: 75,
    scale: 75 / 208,
  },
  states: {
    ...Object.fromEntries(
      Object.entries(manifest.states).filter(([state]) => state !== 'focused' && state !== 'confused'),
    ),
  },
  aliases: { focused: 'working', confused: 'failed' },
  actions: {
    ...manifest.actions,
    jump: { spriteFrames: [0], frames: [{ sprite: 0, duration: 220 }], repeat: 1, fallback: 'idle' },
  },
};

function zeroImageData(manifest) {
  return {
    width: manifest.sprite.columns * manifest.sprite.frameWidth,
    height: manifest.sprite.rows * manifest.sprite.frameHeight,
    data: new Uint8ClampedArray(
      manifest.sprite.columns * manifest.sprite.frameWidth * manifest.sprite.rows * manifest.sprite.frameHeight * 4,
    ),
  };
}

describe('sprite-loader', () => {
  it('validates manifest basics', () => {
    expect(validateManifest(manifest)).toBe(manifest);
    expect(validateManifest(compactManifest)).toBe(compactManifest);
    expect(() => validateManifest({ ...manifest, schemaVersion: 3 })).toThrow(/schemaVersion/);
    expect(() => validateManifest({
      ...manifest,
      states: { ...manifest.states, idle: undefined },
    })).toThrow(/missing required state/);
    expect(() => validateManifest({
      ...manifest,
      states: { ...manifest.states, focused: undefined },
    })).toThrow(/missing required state/);
  });

  it('builds a runtime renderer model from the default tabby cat fixture', () => {
    const imageData = zeroImageData(manifest);
    const runtime = buildRuntimeFromManifest(manifest, imageData);

    expect(runtime.sprites.idle).toHaveLength(14);
    expect(runtime.sprites.focused).toHaveLength(8);
    expect(runtime.sprites.preparing).toHaveLength(8);
    expect(runtime.sprites.jump).toHaveLength(3);
    expect(runtime.frameWidth).toBe(192);
    expect(runtime.frameHeight).toBe(208);
    expect(runtime.renderScale).toBeCloseTo(80 / 208);
    expect(runtime.displayWidth).toBe(74);
    expect(runtime.displayHeight).toBe(80);
    expect(runtime.pixelated).toBe(false);
    expect(runtime.sheetColumns).toBe(8);
    expect(runtime.palette).toEqual({});
    expect(runtime.hotspots.observe).toEqual(manifest.hotspots.observe);
    expect(runtime.stateConfig.focused.frames.map((frame) => frame.sprite)).toEqual([0, 4, 5, 6, 7, 3]);
    expect(runtime.stateConfig.idle.variants[0].frames.map((frame) => frame.sprite)).toEqual([7, 12, 0]);
    expect(runtime.actionConfig.observe.frames.map((frame) => frame.sprite)).toEqual([0, 1, 2, 4, 3, 5]);
  });

  it('builds a compact v2 sheet runtime with aliases', () => {
    const imageData = {
      width: compactManifest.sprite.columns * compactManifest.sprite.frameWidth,
      height: compactManifest.sprite.rows * compactManifest.sprite.frameHeight,
      data: new Uint8ClampedArray(compactManifest.sprite.columns * compactManifest.sprite.frameWidth * compactManifest.sprite.rows * compactManifest.sprite.frameHeight * 4),
    };
    const runtime = buildRuntimeFromManifest(compactManifest, imageData);

    expect(runtime.frameWidth).toBe(192);
    expect(runtime.frameHeight).toBe(208);
    expect(runtime.renderScale).toBeCloseTo(75 / 208);
    expect(runtime.displayWidth).toBe(69);
    expect(runtime.displayHeight).toBe(75);
    expect(runtime.hotspots.observe).toEqual(compactManifest.hotspots.observe);
    expect(runtime.hotspots.input).toEqual(compactManifest.hotspots.input);
    expect(runtime.sprites.working).toHaveLength(12);
    expect(runtime.sprites.focused).toBe(runtime.sprites.working);
    expect(runtime.sprites.confused).toBe(runtime.sprites.failed);
    expect(runtime.actionConfig.jump.frames).toEqual([{ sprite: 0, duration: 220 }]);
    expect(runtime.stateConfig.focused.frames.map((frame) => frame.sprite))
      .toEqual(runtime.stateConfig.working.frames.map((frame) => frame.sprite));
  });

  it('rejects timelines that reference frames outside their local spriteFrames', () => {
    const imageData = zeroImageData(manifest);
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
    const imageData = zeroImageData(manifest);
    const renderer = await loadPetAssetPack('/fixtures/cat-tabby', {
      fetch: async (url) => ({
        ok: url.endsWith('/manifest.json'),
        status: 200,
        json: async () => manifest,
      }),
      imageData: async () => imageData,
    });

    expect(renderer.assetSource.kind).toBe('manifest');
    expect(renderer.assetSource.id).toBe('cat-tabby');
    expect(renderer.getSprite('focused', 0)).toBe(renderer.SPRITES.focused[0]);
    expect(renderer.getSprite('unknown', 0)).toBe(renderer.SPRITES.idle[0]);
  });

  it('reads the persisted pet asset setting before using the default pack', async () => {
    const previousTauri = window.__TAURI__;
    const previousPetAssetUrl = window.__PET_ASSET_URL__;
    window.__PET_ASSET_URL__ = '';
    window.sessionStorage.removeItem('bitcat.petAssetUrl');
    window.localStorage.removeItem('bitcat.petAssetUrl');
    window.__TAURI__ = {
      core: {
        invoke: async (command) => {
          expect(command).toBe('cmd_settings_load');
          return {
            appearance: {
            pet_asset_url: '/__fixtures__/pets/cat-snowshoe',
            },
          };
        },
      },
    };

    await expect(configuredPetAssetUrlAsync()).resolves.toBe('/__fixtures__/pets/cat-snowshoe');
    expect(window.sessionStorage.getItem('bitcat.petAssetUrl')).toBe('/__fixtures__/pets/cat-snowshoe');

    window.__TAURI__ = previousTauri;
    window.__PET_ASSET_URL__ = previousPetAssetUrl;
    window.sessionStorage.removeItem('bitcat.petAssetUrl');
  });

  it('loads the tabby cat v2 pack when no asset url is configured', async () => {
    const imageData = {
      width: manifest.sprite.columns * manifest.sprite.frameWidth,
      height: manifest.sprite.rows * manifest.sprite.frameHeight,
      data: new Uint8ClampedArray(manifest.sprite.columns * manifest.sprite.frameWidth * manifest.sprite.rows * manifest.sprite.frameHeight * 4),
    };
    const renderer = await loadPetAssetPack(DEFAULT_PET_ASSET_URL, {
      fetch: async (url) => ({
        ok: url === `${DEFAULT_PET_ASSET_URL}/manifest.json`,
        status: 200,
        json: async () => manifest,
      }),
      imageData: async (url) => {
        expect(url).toBe(`${DEFAULT_PET_ASSET_URL}/spritesheet.webp`);
        return imageData;
      },
    });

    expect(renderer.assetSource.kind).toBe('manifest');
    expect(renderer.assetSource.id).toBe('cat-tabby');
    expect(renderer.assetSource.baseUrl).toBe(DEFAULT_PET_ASSET_URL);
    expect(renderer.SPRITE_W).toBe(192);
    expect(renderer.SPRITE_H).toBe(208);
    expect(renderer.displayWidth).toBe(74);
    expect(renderer.displayHeight).toBe(80);
    expect(renderer.pixelated).toBe(false);
    expect(renderer.actionConfig.observe.frames.map((frame) => frame.sprite)).toEqual([0, 1, 2, 4, 3, 5]);
    expect(renderer.getSprite('observe', 0)).toBe(renderer.SPRITES.observe[0]);
  });

  it('requires an explicit asset URL when loading a pack directly', async () => {
    await expect(loadPetAssetPack(null)).rejects.toThrow(/baseUrl is required/);
  });

  it('loads a compact v2 pack through injected fetch and imageData hooks', async () => {
    const imageData = {
      width: compactManifest.sprite.columns * compactManifest.sprite.frameWidth,
      height: compactManifest.sprite.rows * compactManifest.sprite.frameHeight,
      data: new Uint8ClampedArray(compactManifest.sprite.columns * compactManifest.sprite.frameWidth * compactManifest.sprite.rows * compactManifest.sprite.frameHeight * 4),
    };
    const renderer = await loadPetAssetPack('/fixtures/cat-compact-test', {
      fetch: async (url) => ({
        ok: url.endsWith('/manifest.json'),
        status: 200,
        json: async () => compactManifest,
      }),
      imageData: async () => imageData,
    });

    expect(renderer.assetSource.kind).toBe('manifest');
    expect(renderer.assetSource.id).toBe('cat-compact-test');
    expect(renderer.SPRITE_W).toBe(192);
    expect(renderer.SPRITE_H).toBe(208);
    expect(renderer.renderScale).toBeCloseTo(75 / 208);
    expect(renderer.displayWidth).toBe(69);
    expect(renderer.displayHeight).toBe(75);
    expect(renderer.hotspots.observe).toEqual(compactManifest.hotspots.observe);
    expect(renderer.hotspots.input).toEqual(compactManifest.hotspots.input);
    expect(renderer.getSprite('focused', 0)).toBe(renderer.SPRITES.working[0]);
    expect(renderer.getSprite('confused', 0)).toBe(renderer.SPRITES.failed[0]);
  });

  it('rejects configured packs when external loading fails', async () => {
    await expect(loadPetAssetPack('/broken', {
      fetch: async () => ({ ok: false, status: 404 }),
    })).rejects.toThrow(/manifest request failed: 404/);
  });
});
