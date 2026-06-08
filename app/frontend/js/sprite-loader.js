const DEFAULT_PET_ASSET_URL = '/__fixtures__/pets/cat-tabby';

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

function isVitestRuntime() {
  return typeof process !== 'undefined' && process.env && process.env.VITEST;
}

function configuredPetAssetUrl(options = {}) {
  const includeDefault = options.includeDefault !== false;
  if (typeof window === 'undefined') return includeDefault ? DEFAULT_PET_ASSET_URL : null;
  if (window.__PET_ASSET_URL__) return normalizeBaseUrl(window.__PET_ASSET_URL__);
  const params = new URLSearchParams(window.location ? window.location.search : '');
  const fromQuery = params.get('petAsset');
  if (fromQuery) return normalizeBaseUrl(fromQuery);
  try {
    const fromSession = window.sessionStorage && window.sessionStorage.getItem('bitcat.petAssetUrl');
    if (fromSession) return normalizeBaseUrl(fromSession);
    const fromLocal = window.localStorage && window.localStorage.getItem('bitcat.petAssetUrl');
    return normalizeBaseUrl(fromLocal) || (includeDefault ? DEFAULT_PET_ASSET_URL : null);
  } catch (_) {
    return includeDefault ? DEFAULT_PET_ASSET_URL : null;
  }
}

async function configuredPetAssetUrlAsync() {
  const configured = configuredPetAssetUrl({ includeDefault: false });
  if (configured) return configured;
  if (typeof window === 'undefined' || !window.__TAURI__ || !window.__TAURI__.core) return DEFAULT_PET_ASSET_URL;
  try {
    const snapshot = await window.__TAURI__.core.invoke('cmd_settings_load');
    const url = snapshot && snapshot.appearance && snapshot.appearance.pet_asset_url;
    if (!url) return DEFAULT_PET_ASSET_URL;
    const normalized = normalizeBaseUrl(url);
    window.__PET_ASSET_URL__ = normalized;
    try { window.sessionStorage.setItem('bitcat.petAssetUrl', normalized); } catch (_) {}
    return normalized;
  } catch (error) {
    console.warn('[sprite-loader] pet asset setting unavailable:', error);
    return DEFAULT_PET_ASSET_URL;
  }
}

function clonePalette(palette) {
  return Object.fromEntries(
    Object.entries(palette).map(([key, value]) => [key, value == null ? null : value.slice()])
  );
}

function cloneHotspots(hotspots) {
  if (!hotspots) return null;
  return Object.fromEntries(
    Object.entries(hotspots).map(([name, spec]) => [name, spec ? { ...spec } : spec])
  );
}

function rendererFromRuntime(runtime, source) {
  const sprites = runtime.sprites;
  const palette = runtime.palette;
  const frameWidth = runtime.frameWidth;
  const frameHeight = runtime.frameHeight;
  const renderScale = runtime.renderScale || 8;
  const pixelated = runtime.pixelated !== false;
  const displayWidth = runtime.displayWidth || Math.round(frameWidth * renderScale);
  const displayHeight = runtime.displayHeight || Math.round(frameHeight * renderScale);
  const sheetImage = runtime.sheetImage || null;
  const sheetColumns = runtime.sheetColumns || 1;

  function getSprite(state, frame) {
    const frames = sprites[state] || sprites.idle;
    const f = frame == null ? 0 : ((frame % frames.length) + frames.length) % frames.length;
    return frames[f];
  }

  function renderSprite(ctx, state, frame, facingRight, scale, opts) {
    scale = scale || renderScale;
    opts = opts || {};
    const ox = opts.offsetX || 0;
    const oy = opts.offsetY || 0;
    const bx = opts.baseX || 0;
    const by = opts.baseY || 0;
    const data = getSprite(state, frame);
    const totalW = opts.width || Math.round(frameWidth * scale);
    const totalH = opts.height || Math.round(frameHeight * scale);
    const canvasW = ctx.canvas ? ctx.canvas.width : totalW;
    const canvasH = ctx.canvas ? ctx.canvas.height : totalH;

    ctx.clearRect(0, 0, canvasW, canvasH);
    ctx.save();
    ctx.imageSmoothingEnabled = !pixelated;
    if (facingRight === false) {
      ctx.translate(bx + totalW, by);
      ctx.scale(-1, 1);
    } else {
      ctx.translate(bx, by);
    }

    if (sheetImage) {
      const sx = (data % sheetColumns) * frameWidth;
      const sy = Math.floor(data / sheetColumns) * frameHeight;
      const dx = ox + Math.floor((canvasW - totalW) / 2);
      const dy = oy + Math.floor((canvasH - totalH) / 2);
      ctx.drawImage(sheetImage, sx, sy, frameWidth, frameHeight, dx, dy, totalW, totalH);
      ctx.restore();
      return;
    }

    for (let row = 0; row < frameHeight; row += 1) {
      for (let col = 0; col < frameWidth; col += 1) {
        const idx = row * frameWidth + col;
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
    const miniScale = Math.max(1, Math.floor(48 / Math.max(frameWidth, frameHeight)));
    const headRows = runtime.mini && runtime.mini.headRows ? runtime.mini.headRows : Math.min(frameHeight, 20);
    const offsetX = Math.floor((48 - frameWidth * miniScale) / 2);

    ctx.clearRect(0, 0, 48, 48);
    if (sheetImage) {
      const sx = (data % sheetColumns) * frameWidth;
      const sy = Math.floor(data / sheetColumns) * frameHeight;
      const scale = Math.min(48 / frameWidth, 48 / frameHeight);
      const dw = Math.floor(frameWidth * scale);
      const dh = Math.floor(frameHeight * scale);
      ctx.imageSmoothingEnabled = !pixelated;
      ctx.drawImage(sheetImage, sx, sy, frameWidth, frameHeight, Math.floor((48 - dw) / 2), Math.floor((48 - dh) / 2), dw, dh);
      return;
    }

    const offsetY = Math.floor((48 - headRows * miniScale) / 2);
    for (let row = 0; row < headRows; row += 1) {
      for (let col = 0; col < frameWidth; col += 1) {
        const idx = row * frameWidth + col;
        const color = palette[data[idx]];
        if (color) {
          ctx.fillStyle = `rgba(${color[0]},${color[1]},${color[2]},${color[3] / 255})`;
          ctx.fillRect(offsetX + col * miniScale, offsetY + row * miniScale, miniScale, miniScale);
        }
      }
    }
  }

  return {
    SPRITES: sprites,
    PALETTE: palette,
    SPRITE_W: frameWidth,
    SPRITE_H: frameHeight,
    frameWidth,
    frameHeight,
    renderScale,
    displayWidth,
    displayHeight,
    pixelated,
    getSprite,
    renderSprite,
    renderMini,
    stateConfig: runtime.stateConfig,
    actionConfig: runtime.actionConfig || {},
    hotspots: cloneHotspots(runtime.hotspots),
    assetSource: source,
  };
}

function validateManifest(manifest) {
  if (!manifest || typeof manifest !== 'object') {
    throw new Error('manifest must be an object');
  }
  if (manifest.schemaVersion !== 2) {
    throw new Error('unsupported manifest schemaVersion');
  }
  const sprite = manifest.sprite;
  if (!sprite || typeof sprite !== 'object') {
    throw new Error('manifest.sprite is required');
  }
  if (!Number.isInteger(sprite.frameWidth) || sprite.frameWidth <= 0) {
    throw new Error('sprite.frameWidth must be positive');
  }
  if (!Number.isInteger(sprite.frameHeight) || sprite.frameHeight <= 0) {
    throw new Error('sprite.frameHeight must be positive');
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
    const hasState = manifest.states && manifest.states[state];
    const aliasTarget = manifest.aliases && manifest.aliases[state];
    if (!hasState && !aliasTarget) {
      throw new Error(`missing required state: ${state}`);
    }
  }
  if (manifest.hotspots != null) {
    if (!manifest.hotspots || typeof manifest.hotspots !== 'object' || Array.isArray(manifest.hotspots)) {
      throw new Error('manifest.hotspots must be an object');
    }
    for (const [name, spec] of Object.entries(manifest.hotspots)) {
      if (!spec || typeof spec !== 'object' || Array.isArray(spec)) {
        throw new Error(`hotspots.${name} must be an object`);
      }
      for (const key of ['x', 'y', 'w', 'h']) {
        if (!Number.isFinite(spec[key])) {
          throw new Error(`hotspots.${name}.${key} must be numeric`);
        }
      }
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

function normalizeActionTimeline(config, frameCount, label) {
  if (!config || typeof config !== 'object' || Array.isArray(config)) {
    throw new Error(`${label} must be an object`);
  }
  if (Number.isInteger(config.sprite)) {
    validateFrameRef(config.sprite, frameCount, label);
    return {
      spriteFrames: [config.sprite],
      frames: [{ sprite: config.sprite, duration: Number.isFinite(config.duration) ? config.duration : 220 }],
      repeat: config.repeat == null ? 1 : config.repeat,
      fallback: config.fallback || 'idle',
    };
  }
  const frames = Array.isArray(config.frames) ? config.frames : [];
  const timeline = {
    ...config,
    spriteFrames: Array.isArray(config.spriteFrames)
      ? config.spriteFrames
      : Array.from(new Set(frames.map((frame) => frame.sprite))),
    repeat: config.repeat == null ? 1 : config.repeat,
    fallback: config.fallback || 'idle',
  };
  validateTimeline({ ...timeline, __states: { idle: true, action: true } }, frameCount, label);
  return timeline;
}

function normalizePalette(palette) {
  const source = palette;
  if (!source || typeof source !== 'object') {
    throw new Error('manifest.palette is required');
  }
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
  const { frameWidth, frameHeight } = manifest.sprite;
  const render = manifest.render || {};
  const sheetMode = render.mode === 'sheet';
  if (
    !imageData ||
    imageData.width < manifest.sprite.columns * frameWidth ||
    imageData.height < manifest.sprite.rows * frameHeight
  ) {
    throw new Error('sprite image dimensions do not match manifest');
  }

  const palette = sheetMode ? clonePalette(manifest.palette || {}) : normalizePalette(manifest.palette);
  const lookup = sheetMode ? null : buildPaletteLookup(palette);
  const sheetFrames = [];
  for (let i = 0; i < manifest.sprite.frameCount; i += 1) {
    sheetFrames.push(sheetMode ? i : frameFromImageData(imageData, manifest, i, lookup));
  }

  const sprites = {};
  for (const [state, timeline] of Object.entries(manifest.states)) {
    if (!Array.isArray(timeline.spriteFrames) || timeline.spriteFrames.length === 0) {
      throw new Error(`states.${state}.spriteFrames is required`);
    }
    const refs = timeline.spriteFrames;
    sprites[state] = refs.map((spriteIndex) => sheetFrames[spriteIndex]);
  }
  const normalizedActions = {};
  for (const [action, config] of Object.entries(manifest.actions || {})) {
    const timeline = normalizeActionTimeline(config, manifest.sprite.frameCount, `actions.${action}`);
    normalizedActions[action] = timeline;
    sprites[action] = timeline.spriteFrames.map((spriteIndex) => sheetFrames[spriteIndex]);
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
  for (const [alias, target] of Object.entries(manifest.aliases || {})) {
    if (!sprites[target] || !stateConfig[target]) {
      throw new Error(`alias ${alias} references missing state ${target}`);
    }
    sprites[alias] = sprites[target];
    stateConfig[alias] = { ...stateConfig[target] };
  }
  const actionConfig = {};
  for (const [action, timeline] of Object.entries(normalizedActions)) {
    actionConfig[action] = remapTimeline(
      timeline,
      timeline.spriteFrames,
      manifest.sprite.frameCount,
      action,
      { idle: manifest.states.idle, action: timeline }
    );
  }
  for (const state of REQUIRED_STATES) {
    if (!sprites[state] || !stateConfig[state]) {
      throw new Error(`missing required state: ${state}`);
    }
  }
  const renderScale = Number.isFinite(render.scale)
    ? render.scale
    : Number.isFinite(render.logicalSize)
      ? render.logicalSize / Math.max(frameWidth, frameHeight)
      : 8;
  const displayWidth = Number.isFinite(render.displayWidth)
    ? render.displayWidth
    : Math.round(frameWidth * renderScale);
  const displayHeight = Number.isFinite(render.displayHeight)
    ? render.displayHeight
    : Math.round(frameHeight * renderScale);

  return {
    sprites,
    palette,
    frameWidth,
    frameHeight,
    renderScale,
    pixelated: render.pixelated !== false,
    displayWidth,
    displayHeight,
    sheetColumns: manifest.sprite.columns,
    stateConfig,
    actionConfig,
    mini: manifest.mini || { headRows: 10 },
    hotspots: cloneHotspots(manifest.hotspots),
    manifest,
  };
}

async function imageAssetFromUrl(url) {
  const image = new Image();
  image.decoding = 'sync';
  image.src = url;
  await image.decode();
  const canvas = document.createElement('canvas');
  canvas.width = image.naturalWidth || image.width;
  canvas.height = image.naturalHeight || image.height;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  ctx.drawImage(image, 0, 0);
  return {
    image,
    imageData: ctx.getImageData(0, 0, canvas.width, canvas.height),
  };
}

async function loadPetAssetPack(baseUrl, options) {
  const root = normalizeBaseUrl(baseUrl);
  if (!root) {
    throw new Error('pet asset baseUrl is required');
  }
  const opts = options || {};
  const manifestUrl = `${root}/manifest.json`;
  const response = await (opts.fetch || fetch)(manifestUrl);
  if (!response.ok) {
    throw new Error(`manifest request failed: ${response.status}`);
  }
  const manifest = await response.json();
  const imageUrl = `${root}/${manifest.sprite && manifest.sprite.image ? manifest.sprite.image : 'spritesheet.webp'}`;
  const imageAsset = opts.imageData
    ? { imageData: await opts.imageData(imageUrl, manifest), image: opts.image || null }
    : await imageAssetFromUrl(imageUrl);
  const runtime = buildRuntimeFromManifest(manifest, imageAsset.imageData);
  if (manifest.render && manifest.render.mode === 'sheet') {
    runtime.sheetImage = imageAsset.image;
  }
  return rendererFromRuntime(runtime, {
    kind: 'manifest',
    id: manifest.id || null,
    baseUrl: root,
  });
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
  const promise = configuredPetAssetUrlAsync()
    .then((baseUrl) => loadPetAssetPack(baseUrl))
    .then(installSpriteRenderer);
  if (typeof window !== 'undefined') {
    window.SpriteRendererReady = promise;
  }
  return promise;
}

if (typeof window !== 'undefined' && !isVitestRuntime()) {
  initSpriteRenderer();
}

export {
  DEFAULT_PET_ASSET_URL,
  REQUIRED_STATES,
  buildRuntimeFromManifest,
  configuredPetAssetUrl,
  configuredPetAssetUrlAsync,
  initSpriteRenderer,
  installSpriteRenderer,
  loadPetAssetPack,
  validateManifest,
};
