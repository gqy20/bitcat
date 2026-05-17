import {
  PALETTE as BUILTIN_PALETTE,
  SPRITES as BUILTIN_SPRITES,
  SPRITE_H,
  SPRITE_W,
  cloneSprite,
  runSpriteTests,
} from './sprite.js';

const REQUIRED_STATES = [
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

function normalizeBaseUrl(baseUrl) {
  if (!baseUrl) return null;
  return String(baseUrl).replace(/\/+$/, '');
}

function configuredPetAssetUrl() {
  if (typeof window === 'undefined') return null;
  if (window.__PET_ASSET_URL__) return normalizeBaseUrl(window.__PET_ASSET_URL__);
  const params = new URLSearchParams(window.location ? window.location.search : '');
  const fromQuery = params.get('petAsset');
  if (fromQuery) return normalizeBaseUrl(fromQuery);
  try {
    return normalizeBaseUrl(window.localStorage && window.localStorage.getItem('ai-pad.petAssetUrl'));
  } catch (_) {
    return null;
  }
}

function clonePalette(palette) {
  return Object.fromEntries(
    Object.entries(palette).map(([key, value]) => [key, value == null ? null : value.slice()])
  );
}

function cloneSprites(sprites) {
  return Object.fromEntries(
    Object.entries(sprites).map(([state, frames]) => [state, frames.map((frame) => frame.slice())])
  );
}

function rendererFromRuntime(runtime, source) {
  const sprites = runtime.sprites;
  const palette = runtime.palette;

  function getSprite(state, frame) {
    const frames = sprites[state] || sprites.idle;
    const f = frame == null ? 0 : ((frame % frames.length) + frames.length) % frames.length;
    return frames[f];
  }

  function renderSprite(ctx, state, frame, facingRight, scale, opts) {
    scale = scale || 8;
    opts = opts || {};
    const ox = opts.offsetX || 0;
    const oy = opts.offsetY || 0;
    const bx = opts.baseX || 0;
    const by = opts.baseY || 0;
    const data = getSprite(state, frame);
    const totalW = SPRITE_W * scale;
    const totalH = SPRITE_H * scale;
    const canvasW = ctx.canvas ? ctx.canvas.width : totalW;
    const canvasH = ctx.canvas ? ctx.canvas.height : totalH;

    ctx.clearRect(0, 0, canvasW, canvasH);
    ctx.save();
    if (facingRight === false) {
      ctx.translate(bx + totalW, by);
      ctx.scale(-1, 1);
    } else {
      ctx.translate(bx, by);
    }

    for (let row = 0; row < SPRITE_H; row += 1) {
      for (let col = 0; col < SPRITE_W; col += 1) {
        const idx = row * SPRITE_W + col;
        const color = palette[data[idx]];
        if (color) {
          ctx.fillStyle = `rgba(${color[0]},${color[1]},${color[2]},${color[3] / 255})`;
          ctx.fillRect(col * scale + ox, row * scale + oy, scale, scale);
        }
      }
    }
    ctx.restore();
  }

  function renderMini(ctx, state) {
    const data = getSprite(state, 0);
    const miniScale = 3;
    const headRows = runtime.mini && runtime.mini.headRows ? runtime.mini.headRows : 10;

    ctx.clearRect(0, 0, 48, 48);
    const offsetY = Math.floor((48 - headRows * miniScale) / 2);
    for (let row = 0; row < headRows; row += 1) {
      for (let col = 0; col < SPRITE_W; col += 1) {
        const idx = row * SPRITE_W + col;
        const color = palette[data[idx]];
        if (color) {
          ctx.fillStyle = `rgba(${color[0]},${color[1]},${color[2]},${color[3] / 255})`;
          ctx.fillRect(col * miniScale, offsetY + row * miniScale, miniScale, miniScale);
        }
      }
    }
  }

  return {
    SPRITES: sprites,
    PALETTE: palette,
    SPRITE_W,
    SPRITE_H,
    getSprite,
    renderSprite,
    renderMini,
    runSpriteTests,
    cloneSprite,
    stateConfig: runtime.stateConfig,
    assetSource: source,
  };
}

function builtinRuntime() {
  return {
    sprites: cloneSprites(BUILTIN_SPRITES),
    palette: clonePalette(BUILTIN_PALETTE),
    stateConfig: null,
    mini: { headRows: 10 },
  };
}

function builtinRenderer(reason) {
  return rendererFromRuntime(builtinRuntime(), {
    kind: 'builtin',
    reason: reason || null,
  });
}

function validateManifest(manifest) {
  if (!manifest || typeof manifest !== 'object') {
    throw new Error('manifest must be an object');
  }
  if (manifest.schemaVersion !== 1) {
    throw new Error('unsupported manifest schemaVersion');
  }
  const sprite = manifest.sprite;
  if (!sprite || typeof sprite !== 'object') {
    throw new Error('manifest.sprite is required');
  }
  if (sprite.frameWidth !== SPRITE_W || sprite.frameHeight !== SPRITE_H) {
    throw new Error(`sprite frames must be ${SPRITE_W}x${SPRITE_H}`);
  }
  if (!Number.isInteger(sprite.columns) || sprite.columns <= 0) {
    throw new Error('sprite.columns must be positive');
  }
  if (!Number.isInteger(sprite.rows) || sprite.rows <= 0) {
    throw new Error('sprite.rows must be positive');
  }
  if (!Number.isInteger(sprite.frameCount) || sprite.frameCount <= 0) {
    throw new Error('sprite.frameCount must be positive');
  }
  if (sprite.frameCount > sprite.columns * sprite.rows) {
    throw new Error('sprite.frameCount exceeds sheet capacity');
  }
  for (const state of REQUIRED_STATES) {
    if (!manifest.states || !manifest.states[state]) {
      throw new Error(`missing required state: ${state}`);
    }
  }
  return manifest;
}

function validateFrameRef(index, frameCount, label) {
  if (!Number.isInteger(index) || index < 0 || index >= frameCount) {
    throw new Error(`${label} references sprite ${index}, outside [0, ${frameCount})`);
  }
}

function validateTimeline(timeline, frameCount, label) {
  if (!timeline || !Array.isArray(timeline.frames) || timeline.frames.length === 0) {
    throw new Error(`${label}.frames must be non-empty`);
  }
  for (const frame of timeline.frames) {
    validateFrameRef(frame.sprite, frameCount, label);
    if (!Number.isFinite(frame.duration) || frame.duration <= 0) {
      throw new Error(`${label}.duration must be positive`);
    }
  }
  if (timeline.spriteFrames != null) {
    if (!Array.isArray(timeline.spriteFrames) || timeline.spriteFrames.length === 0) {
      throw new Error(`${label}.spriteFrames must be non-empty`);
    }
    for (const spriteIndex of timeline.spriteFrames) {
      validateFrameRef(spriteIndex, frameCount, `${label}.spriteFrames`);
    }
  }
  if (timeline.repeat != null && (!Number.isInteger(timeline.repeat) || timeline.repeat <= 0)) {
    throw new Error(`${label}.repeat must be a positive integer`);
  }
  if (timeline.fallback != null && !manifestHasState(timeline.fallback)) {
    throw new Error(`${label}.fallback references missing state`);
  }
  for (const variant of timeline.variants || []) {
    if (!Array.isArray(variant.frames) || variant.frames.length === 0) {
      throw new Error(`${label}.variant.frames must be non-empty`);
    }
    if (
      Number.isFinite(variant.cooldownMinMs) &&
      Number.isFinite(variant.cooldownMaxMs) &&
      variant.cooldownMaxMs < variant.cooldownMinMs
    ) {
      throw new Error(`${label}.variant cooldown max is below min`);
    }
    for (const frame of variant.frames) {
      validateFrameRef(frame.sprite, frameCount, `${label}.variant`);
      if (!Number.isFinite(frame.duration) || frame.duration <= 0) {
        throw new Error(`${label}.variant.duration must be positive`);
      }
    }
  }

  function manifestHasState(state) {
    return !!timeline.__states[state];
  }
}

function normalizePalette(palette) {
  const source = palette || BUILTIN_PALETTE;
  const normalized = {};
  for (const [key, value] of Object.entries(source)) {
    if (value == null) {
      normalized[key] = null;
    } else if (Array.isArray(value) && value.length === 4) {
      normalized[key] = value.map((item) => Number(item));
    } else {
      throw new Error(`palette.${key} must be null or RGBA`);
    }
  }
  return normalized;
}

function rgbaKey(color) {
  return color ? color.join(',') : '0,0,0,0';
}

function buildPaletteLookup(palette) {
  const lookup = new Map();
  for (const [index, color] of Object.entries(palette)) {
    lookup.set(rgbaKey(color), Number(index));
  }
  return lookup;
}

function frameFromImageData(imageData, manifest, frameIndex, lookup) {
  const { frameWidth, frameHeight, columns } = manifest.sprite;
  const originX = (frameIndex % columns) * frameWidth;
  const originY = Math.floor(frameIndex / columns) * frameHeight;
  const frame = [];
  for (let row = 0; row < frameHeight; row += 1) {
    for (let col = 0; col < frameWidth; col += 1) {
      const offset = ((originY + row) * imageData.width + originX + col) * 4;
      const color = [
        imageData.data[offset],
        imageData.data[offset + 1],
        imageData.data[offset + 2],
        imageData.data[offset + 3],
      ];
      const key = color[3] === 0 ? '0,0,0,0' : rgbaKey(color);
      const paletteIndex = lookup.get(key);
      if (paletteIndex == null) {
        throw new Error(`sprite color ${key} is not present in palette`);
      }
      frame.push(paletteIndex);
    }
  }
  return frame;
}

function remapTimeline(timeline, localFrames, frameCount, stateName, allStates) {
  const withStates = { ...timeline, __states: allStates };
  validateTimeline(withStates, frameCount, `states.${stateName}`);
  const localIndexByGlobal = new Map(localFrames.map((spriteIndex, index) => [spriteIndex, index]));

  function toLocalFrame(frame, label) {
    const sprite = localIndexByGlobal.get(frame.sprite);
    if (sprite == null) {
      throw new Error(`${label} references sprite ${frame.sprite}, outside ${stateName}.spriteFrames`);
    }
    return { ...frame, sprite };
  }

  const frames = timeline.frames.map((frame) => toLocalFrame(frame, `states.${stateName}.frames`));
  const out = { ...timeline, frames };
  if (timeline.variants) {
    out.variants = timeline.variants.map((variant) => ({
      ...variant,
      frames: variant.frames.map((frame) => toLocalFrame(frame, `states.${stateName}.variants`)),
    }));
  }
  delete out.__states;
  return out;
}

function buildRuntimeFromManifest(manifest, imageData) {
  validateManifest(manifest);
  if (
    !imageData ||
    imageData.width < manifest.sprite.columns * SPRITE_W ||
    imageData.height < manifest.sprite.rows * SPRITE_H
  ) {
    throw new Error('sprite image dimensions do not match manifest');
  }

  const palette = normalizePalette(manifest.palette);
  const lookup = buildPaletteLookup(palette);
  const sheetFrames = [];
  for (let i = 0; i < manifest.sprite.frameCount; i += 1) {
    sheetFrames.push(frameFromImageData(imageData, manifest, i, lookup));
  }

  const sprites = {};
  for (const [state, timeline] of Object.entries(manifest.states)) {
    if (!Array.isArray(timeline.spriteFrames) || timeline.spriteFrames.length === 0) {
      throw new Error(`states.${state}.spriteFrames is required`);
    }
    const refs = timeline.spriteFrames;
    sprites[state] = refs.map((spriteIndex) => sheetFrames[spriteIndex]);
  }
  for (const [action, config] of Object.entries(manifest.actions || {})) {
    validateFrameRef(config.sprite, manifest.sprite.frameCount, `actions.${action}`);
    sprites[action] = [sheetFrames[config.sprite]];
  }

  const stateConfig = {};
  for (const [state, timeline] of Object.entries(manifest.states)) {
    stateConfig[state] = remapTimeline(
      timeline,
      timeline.spriteFrames,
      manifest.sprite.frameCount,
      state,
      manifest.states
    );
  }

  return {
    sprites,
    palette,
    stateConfig,
    mini: manifest.mini || { headRows: 10 },
    manifest,
  };
}

async function imageDataFromUrl(url) {
  const image = new Image();
  image.decoding = 'sync';
  image.src = url;
  await image.decode();
  const canvas = document.createElement('canvas');
  canvas.width = image.naturalWidth || image.width;
  canvas.height = image.naturalHeight || image.height;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  ctx.drawImage(image, 0, 0);
  return ctx.getImageData(0, 0, canvas.width, canvas.height);
}

async function loadPetAssetPack(baseUrl, options) {
  const root = normalizeBaseUrl(baseUrl);
  if (!root) {
    return builtinRenderer('no external pet configured');
  }
  const opts = options || {};
  try {
    const manifestUrl = `${root}/manifest.json`;
    const response = await (opts.fetch || fetch)(manifestUrl);
    if (!response.ok) {
      throw new Error(`manifest request failed: ${response.status}`);
    }
    const manifest = await response.json();
    const imageUrl = `${root}/${manifest.sprite && manifest.sprite.image ? manifest.sprite.image : 'sprites.png'}`;
    const imageData = opts.imageData
      ? await opts.imageData(imageUrl, manifest)
      : await imageDataFromUrl(imageUrl);
    const runtime = buildRuntimeFromManifest(manifest, imageData);
    return rendererFromRuntime(runtime, {
      kind: 'manifest',
      id: manifest.id || null,
      baseUrl: root,
    });
  } catch (error) {
    console.warn('[sprite-loader] external pet failed, using builtin sprite:', error);
    return builtinRenderer(error && error.message ? error.message : String(error));
  }
}

function installSpriteRenderer(renderer) {
  if (typeof window !== 'undefined') {
    if (window.SpriteRenderer && window.SpriteRenderer !== renderer) {
      Object.assign(window.SpriteRenderer, renderer);
    } else {
      window.SpriteRenderer = renderer;
    }
  }
  return typeof window !== 'undefined' ? window.SpriteRenderer : renderer;
}

function initSpriteRenderer() {
  installSpriteRenderer(builtinRenderer('external pet not loaded yet'));
  const baseUrl = configuredPetAssetUrl();
  const promise = loadPetAssetPack(baseUrl).then(installSpriteRenderer);
  if (typeof window !== 'undefined') {
    window.SpriteRendererReady = promise;
  }
  return promise;
}

if (typeof window !== 'undefined') {
  initSpriteRenderer();
}

export {
  REQUIRED_STATES,
  buildRuntimeFromManifest,
  builtinRenderer,
  configuredPetAssetUrl,
  initSpriteRenderer,
  installSpriteRenderer,
  loadPetAssetPack,
  validateManifest,
};
