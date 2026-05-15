export function clamp01(value) {
  var n = Number(value);
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(1, n));
}

export function stepRepeat(step) {
  var repeat = Number(step && step.repeat);
  if (!Number.isFinite(repeat) || repeat < 1) return 1;
  return Math.max(1, Math.floor(repeat));
}

function emptyOffset() {
  return { x: 0, y: 0 };
}

function hasMetrics(metrics) {
  return metrics && Number.isFinite(metrics.screenW) && Number.isFinite(metrics.screenH);
}

export function computeTimelineDanceOffset(action, progress, time, metrics, intensity) {
  if (!hasMetrics(metrics)) return emptyOffset();
  var power = Math.max(0, Number.isFinite(Number(intensity)) ? Number(intensity) : 1);
  var x = 0, y = 0;

  switch (action) {
    case 'jump': {
      y = -Math.sin(progress * Math.PI) * metrics.screenH * 0.22 * power;
      x = Math.sin(progress * Math.PI) * metrics.screenW * 0.08 * power;
      break;
    }
    case 'spin': {
      x = Math.sin(time * 0.03) * metrics.screenW * 0.12 * power;
      y = Math.cos(time * 0.025) * metrics.screenH * 0.04 * power;
      break;
    }
    case 'wave': {
      y = -Math.abs(Math.sin(progress * Math.PI * 4)) * metrics.screenH * 0.08 * power;
      break;
    }
    case 'shake': {
      x = Math.sin(time * 0.05) * metrics.screenW * 0.20 * power;
      y = Math.sin(time * 0.07) * metrics.screenH * 0.025 * power;
      break;
    }
  }

  return { x: x, y: y };
}

export function computeMusicDanceOffset(action, progress, time, metrics, intensity) {
  if (!hasMetrics(metrics)) return emptyOffset();
  var power = clamp01(intensity);
  var x = 0, y = 0;

  switch (action) {
    case 'jump':
      y = -Math.sin(Math.min(progress, 1) * Math.PI) * (metrics.screenH * 0.012 + power * metrics.screenH * 0.025);
      x = Math.sin(time * 0.01) * (metrics.screenW * 0.006 + power * metrics.screenW * 0.012);
      break;
    case 'shake':
      x = Math.sin(time * 0.08) * (metrics.screenW * 0.006 + power * metrics.screenW * 0.018);
      y = Math.sin(time * 0.045) * (metrics.screenH * 0.004 + power * metrics.screenH * 0.008);
      break;
    case 'wave':
      y = -Math.abs(Math.sin(time * 0.012)) * (metrics.screenH * 0.008 + power * metrics.screenH * 0.014);
      x = Math.sin(time * 0.006) * metrics.screenW * 0.005;
      break;
  }

  return { x: x, y: y };
}

export function computeTimelineSpriteOptions(action, progress, time, intensity) {
  var power = Math.max(0, Number.isFinite(Number(intensity)) ? Number(intensity) : 1);
  var opts = {};
  var facingRight = null;

  switch (action) {
    case 'jump':
      opts.offsetY = -Math.sin(progress * Math.PI) * 18 * power;
      break;
    case 'spin':
      facingRight = Math.floor(time / 60) % 2 === 0;
      break;
    case 'wave':
      opts.offsetY = -Math.abs(Math.sin(progress * Math.PI * 4)) * 9 * power;
      break;
    case 'shake':
      opts.offsetX = Math.sin(time * 0.06) * 10 * power;
      break;
  }

  return { opts: opts, facingRight: facingRight };
}

export function computeMusicSpriteOptions(action, progress, time, intensity) {
  var power = Math.max(0.08, clamp01(intensity));
  var opts = {};

  switch (action) {
    case 'jump':
      opts.offsetY = -Math.sin(Math.min(progress, 1) * Math.PI) * (14 + power * 24);
      break;
    case 'shake':
      opts.offsetX = Math.sin(time * 0.08) * (6 + power * 18);
      break;
    case 'wave':
      opts.offsetY = -Math.abs(Math.sin(time * 0.015)) * (6 + power * 14);
      break;
  }

  return { opts: opts, facingRight: null };
}
