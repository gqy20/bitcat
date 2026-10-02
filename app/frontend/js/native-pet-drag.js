// A native drag can swallow WebView pointerup and can outlive startDragging().
// Only explicit pointer release or a native button-up sample ends the hold.
// Generation checks discard late samples and cancel stale snap/drop callbacks.
export class NativePetDrag {
  constructor({ pet, isPressed, onDrop, onError = console.warn,
    schedule = callback => setTimeout(callback, 100), unschedule = clearTimeout }) {
    Object.assign(this, { pet, isPressed, onDrop, onError, schedule, unschedule });
    this.generation = 0;
    this.session = null;
    this.timer = null;
  }

  cancel() {
    this.generation += 1;
    this.session = null;
    if (this.timer != null) this.unschedule(this.timer);
    this.timer = null;
    this.pet.cancelDrag();
  }

  async start(win) {
    this.cancel();
    const token = this.generation;
    this.session = { win, token };
    this.pet.beginDrag();
    try {
      const pressed = await this.isPressed();
      if (!this.session || token !== this.generation) return;
      if (pressed === false) { this.release(); return; }
      this.session.nativePolling = pressed === true;
      if (pressed === true) this.queuePoll(token);
      // Resolving the IPC promise is NOT evidence that the user released.
      await win.startDragging();
    } catch (error) {
      if (token !== this.generation) return;
      this.cancel();
      this.onError('[pet] native drag failed:', error);
    }
  }

  queuePoll(token) {
    this.timer = this.schedule(async () => {
      this.timer = null;
      try {
        const pressed = await this.isPressed();
        if (!this.session || token !== this.generation) return;
        if (pressed === false) this.release();
        else if (pressed === true) this.queuePoll(token);
        else this.session.nativePolling = false;
        // null means unsupported; wait for a DOM release instead of guessing.
      } catch (error) {
        if (token !== this.generation) return;
        this.cancel();
        this.onError('[pet] drag button query failed:', error);
      }
    });
  }

  release() {
    const session = this.session;
    if (!session) return;
    this.session = null;
    if (this.timer != null) this.unschedule(this.timer);
    this.timer = null;
    this.pet.endDrag();
    const current = () => this.generation === session.token;
    Promise.resolve().then(() => current() && this.onDrop(session.win, current))
      .catch(error => this.onError('[pet] drop positioning failed:', error));
  }
}
