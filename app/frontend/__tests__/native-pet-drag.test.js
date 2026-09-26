import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { NativePetDrag } from '../js/native-pet-drag.js';

function fixture() {
  const pet = { beginDrag: vi.fn(), endDrag: vi.fn(), cancelDrag: vi.fn() };
  const isPressed = vi.fn(async () => true);
  const onDrop = vi.fn(async () => {}), onError = vi.fn();
  const win = { startDragging: vi.fn(async () => {}) };
  const drag = new NativePetDrag({ pet, isPressed, onDrop, onError });
  return { pet, isPressed, onDrop, onError, win, drag };
}

describe('native pet drag lifecycle', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('does not mistake resolved startDragging or a long stationary hold for release', async () => {
    const f = fixture();
    await f.drag.start(f.win);
    await vi.advanceTimersByTimeAsync(12_000);
    expect(f.isPressed.mock.calls.length).toBeGreaterThan(100);
    expect(f.pet.endDrag).not.toHaveBeenCalled();
    expect(f.onDrop).not.toHaveBeenCalled();
    expect(f.drag.session).not.toBeNull();
    f.drag.cancel();
  });

  it('finishes once on button-up, stops polling, and ignores duplicate pointerup', async () => {
    const f = fixture();await f.drag.start(f.win);
    f.isPressed.mockResolvedValue(false);
    await vi.advanceTimersByTimeAsync(100);
    f.drag.release();
    const queries = f.isPressed.mock.calls.length;
    await vi.advanceTimersByTimeAsync(5000);
    expect(f.pet.endDrag).toHaveBeenCalledTimes(1);
    expect(f.onDrop).toHaveBeenCalledTimes(1);
    expect(f.isPressed).toHaveBeenCalledTimes(queries);
  });

  it('handles release before native dragging starts', async () => {
    const f = fixture();f.isPressed.mockResolvedValue(false);
    await f.drag.start(f.win);await Promise.resolve();
    expect(f.win.startDragging).not.toHaveBeenCalled();
    expect(f.pet.endDrag).toHaveBeenCalledTimes(1);
  });

  it('discards an old asynchronous button sample after re-grabbing', async () => {
    const f = fixture();let resolve;
    f.isPressed.mockImplementationOnce(() => new Promise(r => { resolve = r; }));
    const old = f.drag.start(f.win);
    await f.drag.start(f.win);
    resolve(false);await old;
    expect(f.pet.endDrag).not.toHaveBeenCalled();
    expect(f.drag.session).not.toBeNull();f.drag.cancel();
  });

  it('does not snap a cancelled drop after a new drag starts', async () => {
    const f = fixture();await f.drag.start(f.win);
    f.drag.release();const newer = f.drag.start(f.win);await newer;
    expect(f.onDrop).not.toHaveBeenCalled();f.drag.cancel();
  });

  it('cancels on native drag failure without pretending to land', async () => {
    const f = fixture();f.win.startDragging.mockRejectedValue(new Error('native drag rejected'));
    await f.drag.start(f.win);await vi.advanceTimersByTimeAsync(1000);
    expect(f.drag.session).toBeNull();expect(f.onError).toHaveBeenCalled();
    expect(f.pet.endDrag).not.toHaveBeenCalled();expect(f.onDrop).not.toHaveBeenCalled();
  });

  it('waits for explicit pointerup when native button state is unsupported', async () => {
    const f = fixture();f.isPressed.mockResolvedValue(null);
    await f.drag.start(f.win);await vi.advanceTimersByTimeAsync(12_000);
    expect(f.isPressed).toHaveBeenCalledTimes(1);expect(f.pet.endDrag).not.toHaveBeenCalled();
    f.drag.release();await Promise.resolve();expect(f.pet.endDrag).toHaveBeenCalledTimes(1);
  });
});
