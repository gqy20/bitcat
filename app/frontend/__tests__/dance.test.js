import { describe, it, expect } from 'vitest';
import {
  clamp01,
  computeMusicDanceOffset,
  computeMusicSpriteOptions,
  computeTimelineDanceOffset,
  computeTimelineSpriteOptions,
  stepRepeat,
} from '../js/performance/motion.js';
import { TimelineDancePlayer } from '../js/performance/timeline-dance-player.js';
import { MusicReactivePlayer } from '../js/performance/music-reactive-player.js';

const metrics = {
  baseX: 10,
  baseY: 20,
  screenW: 1000,
  screenH: 800,
};

function timelinePayload(overrides = {}) {
  return {
    session_id: 7,
    kind: 'timeline-dance',
    dance: {
      loop_: false,
      max_duration_ms: null,
      steps: [{ action: 'shake', duration_ms: 100 }],
      ...overrides,
    },
  };
}

describe('performance motion helpers', () => {
  it('normalizes repeat and clamped scalar input', () => {
    expect(stepRepeat({})).toBe(1);
    expect(stepRepeat({ repeat: 0 })).toBe(1);
    expect(stepRepeat({ repeat: 2.8 })).toBe(2);

    expect(clamp01(-1)).toBe(0);
    expect(clamp01(0.4)).toBe(0.4);
    expect(clamp01(2)).toBe(1);
    expect(clamp01('nope')).toBe(0);
  });

  it('computes timeline offsets without touching the window', () => {
    const jump = computeTimelineDanceOffset('jump', 0.5, 100, metrics, 1);
    expect(jump.y).toBeCloseTo(-176);
    expect(jump.x).toBeCloseTo(80);

    const idle = computeTimelineDanceOffset('idle', 0.5, 100, metrics, 1);
    expect(idle).toEqual({ x: 0, y: 0 });
  });

  it('computes sprite options separately from window movement', () => {
    const jump = computeTimelineSpriteOptions('jump', 0.5, 100, 1);
    expect(jump.opts.offsetY).toBeCloseTo(-18);
    expect(jump.facingRight).toBeNull();

    const spin = computeTimelineSpriteOptions('spin', 0, 120, 1);
    expect(spin.opts).toEqual({});
    expect(spin.facingRight).toBe(true);
  });

  it('keeps music-reactive motion intentionally smaller than timeline shake', () => {
    const timeline = computeTimelineDanceOffset('shake', 0.25, 100, metrics, 1);
    const music = computeMusicDanceOffset('shake', 0.25, 100, metrics, 0.8);

    expect(Math.abs(music.x)).toBeLessThan(Math.abs(timeline.x));
    expect(Math.abs(music.y)).toBeLessThan(metrics.screenH * 0.03);

    const sprite = computeMusicSpriteOptions('shake', 0.25, 100, 0.8);
    expect(sprite.opts.offsetX).toBeTypeOf('number');
  });
});

describe('timeline performance player', () => {
  it('wraps a fixed dance as a timeline performer', () => {
    const player = new TimelineDancePlayer(timelinePayload({
      max_duration_ms: 500,
      steps: [{ action: 'shake', duration_ms: 100 }],
    }), metrics);

    expect(player.kind).toBe('timeline-dance');
    expect(player.sessionId).toBe(7);
    expect(player.metrics).toEqual(metrics);
    expect(player.maxDurationMs).toBe(500);
  });

  it('repeats a step before advancing to the next step', () => {
    const player = new TimelineDancePlayer(timelinePayload({
      steps: [
        { action: 'shake', duration_ms: 100, repeat: 3 },
        { action: 'idle', duration_ms: 100 },
      ],
    }), null);

    player.update(100);
    expect(player.index).toBe(0);
    expect(player.repeatIndex).toBe(1);

    player.update(100);
    expect(player.index).toBe(0);
    expect(player.repeatIndex).toBe(2);

    player.update(100);
    expect(player.index).toBe(1);
    expect(player.repeatIndex).toBe(0);
  });

  it('carries leftover time into the next step', () => {
    const player = new TimelineDancePlayer(timelinePayload({
      steps: [
        { action: 'jump', duration_ms: 100, repeat: 2 },
        { action: 'wave', duration_ms: 200 },
      ],
    }), null);

    const frame = player.update(250);
    expect(frame.done).toBe(false);
    expect(player.index).toBe(1);
    expect(player.repeatIndex).toBe(0);
    expect(player.time).toBe(50);
  });

  it('stops a timeline performer at max duration', () => {
    const player = new TimelineDancePlayer(timelinePayload({
      loop_: true,
      max_duration_ms: 250,
      steps: [{ action: 'shake', duration_ms: 100 }],
    }), null);

    expect(player.update(249).done).toBe(false);
    const frame = player.update(1);
    expect(frame.done).toBe(true);
    expect(frame.reason).toBe('max_duration');
  });
});

describe('music reactive performance player', () => {
  it('maps music onset frames to reactive actions', () => {
    const player = new MusicReactivePlayer({ session_id: 9, kind: 'music-reactive' }, null);

    player.handleFrame({ energy: 0.5, bass: 0.2, onset: true });
    expect(player.kind).toBe('music-reactive');
    expect(player.sessionId).toBe(9);
    expect(player.action).toBe('shake');
    expect(player.onsetMs).toBe(0);

    player.handleFrame({ energy: 0.9, bass: 0.8, onset: true });
    expect(player.action).toBe('jump');
  });

  it('decays stale music frames back toward idle', () => {
    const player = new MusicReactivePlayer({ session_id: 9, kind: 'music-reactive' }, null);

    player.handleFrame({ energy: 0.7, onset: false });
    player.update(100);
    expect(player.energy).toBeGreaterThan(0);

    player.update(1700);
    expect(player.targetEnergy).toBe(0);

    for (let i = 0; i < 20; i++) player.update(100);
    expect(player.action).toBe('idle');
  });
});
