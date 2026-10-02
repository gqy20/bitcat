// Moves the native pet window using the state machine's logical walk position.
// Generation checks prevent stale async monitor reads from restarting cancelled walks.
// Native writes are serialized; all coordinates crossing Tauri are physical pixels.
export class WindowWalk {
  constructor({ pet, getWindow, getApi, getScale }) {
    Object.assign(this, { pet, getWindow, getApi, getScale });
    this.generation = 0;
    this.active = null;
    this.busy = false;
    this.pendingWrite = null;
  }

  cancel() {
    this.generation += 1;
    this.active = null;
    if (this.pet.state === 'walk') this.pet.setState(this.pet.currentVisualState());
    if (this.pet.locomotionAction) this.pet.clearAction();
    return this.pendingWrite ? this.pendingWrite.catch(() => {}) : Promise.resolve();
  }

  async walkTo(target) {
    if (!Number.isFinite(target)) return;
    const win = this.getWindow();
    if (!win) { this.pet.walkTo(target); return; }
    const generation = ++this.generation;
    this.active = null;
    try {
      if (this.pendingWrite) await this.pendingWrite.catch(() => {});
      if (generation !== this.generation) return;
      const api = this.getApi();
      const [position, size, dpi, monitor] = await Promise.all([
        win.outerPosition(), win.outerSize(), win.scaleFactor(), api.currentMonitor(),
      ]);
      if (generation !== this.generation) return;
      const scale = dpi * this.getScale();
      if (!Number.isFinite(scale) || scale <= 0) throw new Error('invalid window scale');
      const area = monitor && (monitor.workArea || monitor);
      const minX = area ? area.position.x : position.x - 300 * scale;
      const maxX = area ? Math.max(minX, minX + area.size.width - size.width) : position.x + 300 * scale;
      const baseX = this.pet.x;
      const end = Math.max(minX, Math.min(maxX, position.x + (target - baseX) * scale));
      this.active = { win, api, position, baseX, scale, lastX: position.x, minX, maxX };
      this.pet.walkTo(baseX + (end - position.x) / scale);
    } catch (error) {
      if (generation === this.generation) this.cancel();
      console.warn('[pet] walk positioning unavailable:', error);
    }
  }

  async update() {
    const movement = this.active;
    if (!movement || this.busy) return;
    const x = Math.round(Math.max(movement.minX, Math.min(movement.maxX,
      movement.position.x + (this.pet.x - movement.baseX) * movement.scale)));
    if (x === movement.lastX) {
      if (this.pet.state !== 'walk') this.active = null;
      return;
    }
    this.busy = true;
    try {
      this.pendingWrite = movement.win.setPosition(new movement.api.PhysicalPosition(x, movement.position.y));
      await this.pendingWrite;
      if (this.active === movement) movement.lastX = x;
    } catch (error) {
      if (this.active === movement) this.cancel();
      console.warn('[pet] walk position update failed:', error);
    } finally { this.busy = false; this.pendingWrite = null; }
  }
}
