import { describe, it, expect } from 'vitest';

function stepRepeat(step) {
  var repeat = Number(step && step.repeat);
  if (!Number.isFinite(repeat) || repeat < 1) return 1;
  return Math.max(1, Math.floor(repeat));
}

function advanceDanceStep(player) {
  var step = player.steps[player.index];
  var repeat = stepRepeat(step);

  if (player.repeatIndex + 1 < repeat) {
    player.repeatIndex++;
    return true;
  }

  player.repeatIndex = 0;
  player.index++;

  if (player.index < player.steps.length) {
    return true;
  }

  if (player.loop_) {
    player.index = 0;
    return true;
  }

  player.finished = true;
  return false;
}

function tickDance(player, dt) {
  player.time += dt;
  var step = player.steps[player.index];

  while (step && player.time >= step.duration_ms) {
    player.time -= step.duration_ms;
    if (!advanceDanceStep(player)) return;
    step = player.steps[player.index];
  }
}

describe('dance repeat playback', () => {
  it('repeats a step before advancing to the next step', () => {
    const player = {
      steps: [
        { action: 'shake', duration_ms: 100, repeat: 3 },
        { action: 'idle', duration_ms: 100 },
      ],
      index: 0,
      repeatIndex: 0,
      time: 0,
      loop_: false,
      finished: false,
    };

    tickDance(player, 100);
    expect(player.index).toBe(0);
    expect(player.repeatIndex).toBe(1);

    tickDance(player, 100);
    expect(player.index).toBe(0);
    expect(player.repeatIndex).toBe(2);

    tickDance(player, 100);
    expect(player.index).toBe(1);
    expect(player.repeatIndex).toBe(0);
  });

  it('defaults missing or invalid repeat to one', () => {
    expect(stepRepeat({})).toBe(1);
    expect(stepRepeat({ repeat: 0 })).toBe(1);
    expect(stepRepeat({ repeat: 2.8 })).toBe(2);
  });

  it('carries leftover time into the next step', () => {
    const player = {
      steps: [
        { action: 'jump', duration_ms: 100, repeat: 2 },
        { action: 'wave', duration_ms: 200 },
      ],
      index: 0,
      repeatIndex: 0,
      time: 0,
      loop_: false,
      finished: false,
    };

    tickDance(player, 250);

    expect(player.index).toBe(1);
    expect(player.repeatIndex).toBe(0);
    expect(player.time).toBe(50);
  });
});
