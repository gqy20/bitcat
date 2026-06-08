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
const statusFixtureDir = path.join(process.cwd(), '__fixtures__', 'pets', 'status');
const statusManifest = JSON.parse(readFileSync(path.join(statusFixtureDir, 'manifest.json'), 'utf8'));

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
    expect(validateManifest(statusManifest)).toBe(statusManifest);
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

  it('builds a status-sized v2 sheet runtime with aliases from the status fixture', () => {
    const imageData = {
      width: statusManifest.sprite.columns * statusManifest.sprite.frameWidth,
      height: statusManifest.sprite.rows * statusManifest.sprite.frameHeight,
      data: new Uint8ClampedArray(statusManifest.sprite.columns * statusManifest.sprite.frameWidth * statusManifest.sprite.rows * statusManifest.sprite.frameHeight * 4),
    };
    const runtime = buildRuntimeFromManifest(statusManifest, imageData);

    expect(runtime.frameWidth).toBe(192);
    expect(runtime.frameHeight).toBe(208);
    expect(runtime.renderScale).toBeCloseTo(75 / 208);
    expect(runtime.displayWidth).toBe(69);
    expect(runtime.displayHeight).toBe(75);
    expect(runtime.hotspots.observe).toEqual(statusManifest.hotspots.observe);
    expect(runtime.hotspots.input).toEqual(statusManifest.hotspots.input);
    expect(runtime.sprites.working).toHaveLength(6);
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
              pet_asset_url: '/__fixtures__/pets/status',
            },
          };
        },
      },
    };

    await expect(configuredPetAssetUrlAsync()).resolves.toBe('/__fixtures__/pets/status');
    expect(window.sessionStorage.getItem('bitcat.petAssetUrl')).toBe('/__fixtures__/pets/status');

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

  it('loads the status v2 pack through injected fetch and imageData hooks', async () => {
    const imageData = {
      width: statusManifest.sprite.columns * statusManifest.sprite.frameWidth,
      height: statusManifest.sprite.rows * statusManifest.sprite.frameHeight,
      data: new Uint8ClampedArray(statusManifest.sprite.columns * statusManifest.sprite.frameWidth * statusManifest.sprite.rows * statusManifest.sprite.frameHeight * 4),
    };
    const renderer = await loadPetAssetPack('/fixtures/status', {
      fetch: async (url) => ({
        ok: url.endsWith('/manifest.json'),
        status: 200,
        json: async () => statusManifest,
      }),
      imageData: async () => imageData,
    });

    expect(renderer.assetSource.kind).toBe('manifest');
    expect(renderer.assetSource.id).toBe('status');
    expect(renderer.SPRITE_W).toBe(192);
    expect(renderer.SPRITE_H).toBe(208);
    expect(renderer.renderScale).toBeCloseTo(75 / 208);
    expect(renderer.displayWidth).toBe(69);
    expect(renderer.displayHeight).toBe(75);
    expect(renderer.hotspots.observe).toEqual(statusManifest.hotspots.observe);
    expect(renderer.hotspots.input).toEqual(statusManifest.hotspots.input);
    expect(renderer.getSprite('preparing', 0)).toBe(renderer.SPRITES.working[0]);
  });

  it('rejects configured packs when external loading fails', async () => {
    await expect(loadPetAssetPack('/broken', {
      fetch: async () => ({ ok: false, status: 404 }),
    })).rejects.toThrow(/manifest request failed: 404/);
  });
});
