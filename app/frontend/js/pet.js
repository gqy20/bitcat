// pet.js — 前端宠物状态机（镜像 Rust core::pet::Pet 逻辑）

const STATE_CONFIG = {
  idle:     { frameCount: 4, frameDuration: 500, autoIdleTimeout: null },
  walk:     { frameCount: 4, frameDuration: 150, autoIdleTimeout: 3000 },
  sleep:    { frameCount: 2, frameDuration: 800, autoIdleTimeout: null },
  talk:     { frameCount: 3, frameDuration: 300, autoIdleTimeout: 5000 },
  happy:    { frameCount: 3, frameDuration: 200, autoIdleTimeout: 2000 },
  confused: { frameCount: 2, frameDuration: 400, autoIdleTimeout: 3000 },
};

class PetStateMachine {
  constructor() {
    this.state = 'idle';
    this.frame = 0;
    this.frameTimeMs = 0;
    this.stateTimeMs = 0;
    this.x = 64;
    this.y = 64;
    this.facingRight = true;
    this.speed = 60; // px/s
    this.targetX = null;
    this.bubble = null;
  }

  setState(newState) {
    if (this.state === newState) return;
    this.state = newState;
    this.frame = 0;
    this.frameTimeMs = 0;
    this.stateTimeMs = 0;
    if (newState !== 'walk') {
      this.targetX = null;
    }
  }

  walkTo(x) {
    this.setState('walk');
    this.targetX = x;
  }

  update(dtMs) {
    this.stateTimeMs += dtMs;
    this.frameTimeMs += dtMs;

    // 帧推进
    const config = STATE_CONFIG[this.state] || STATE_CONFIG.idle;
    while (this.frameTimeMs >= config.frameDuration) {
      this.frameTimeMs -= config.frameDuration;
      this.frame = (this.frame + 1) % config.frameCount;
    }

    // Walk 移动
    if (this.state === 'walk' && this.targetX !== null) {
      const dx = this.targetX - this.x;
      const move = this.speed * dtMs / 1000;
      if (Math.abs(dx) < move) {
        this.x = this.targetX;
      } else {
        this.x += Math.sign(dx) * move;
      }
      this.facingRight = dx > 0;
    }

    // 自动回 idle
    if (config.autoIdleTimeout !== null && this.stateTimeMs >= config.autoIdleTimeout) {
      this.setState('idle');
    }
  }

  applyEvent(event) {
    if (event.state) {
      if (event.state === 'exit') return;
      this.setState(event.state);
    }
    if (event.walk_to != null) {
      this.walkTo(event.walk_to);
    }
    if (event.bubble) {
      this.bubble = event.bubble;
    }
  }
}

// 测试函数
function runPetTests() {
  const results = [];

  function assert(name, condition) {
    results.push({ name, pass: !!condition });
  }

  // 默认状态
  const pet = new PetStateMachine();
  assert('default_state_idle', pet.state === 'idle');
  assert('default_frame_0', pet.frame === 0);
  assert('default_facing_right', pet.facingRight === true);

  // setState 重置帧
  pet.frameTimeMs = 999;
  pet.stateTimeMs = 9999;
  pet.frame = 3;
  pet.setState('talk');
  assert('set_state_resets_frame', pet.frame === 0);
  assert('set_state_resets_timers', pet.frameTimeMs === 0 && pet.stateTimeMs === 0);

  // 同状态不重置
  const pet2 = new PetStateMachine();
  pet2.frameTimeMs = 500;
  pet2.setState('idle');
  assert('same_state_no_reset', pet2.frameTimeMs === 500);

  // 帧推进
  const pet3 = new PetStateMachine();
  pet3.update(499);
  assert('frame_no_advance_499', pet3.frame === 0);
  pet3.update(1);
  assert('frame_advance_at_500', pet3.frame === 1);

  // Walk 移动
  const pet4 = new PetStateMachine();
  pet4.x = 0;
  pet4.speed = 100;
  pet4.walkTo(50);
  pet4.update(500);
  assert('walk_reaches_target', Math.abs(pet4.x - 50) < 1);
  assert('walk_facing_right', pet4.facingRight === true);

  // 自动 idle 超时
  const pet5 = new PetStateMachine();
  pet5.setState('walk');
  pet5.update(2999);
  assert('walk_still_walk_at_2999', pet5.state === 'walk');
  pet5.update(1);
  assert('walk_auto_idle_at_3000', pet5.state === 'idle');

  // Sleep 不超时
  const pet6 = new PetStateMachine();
  pet6.setState('sleep');
  pet6.update(100000);
  assert('sleep_no_timeout', pet6.state === 'sleep');

  // applyEvent
  const pet7 = new PetStateMachine();
  pet7.applyEvent({ state: 'talk' });
  assert('apply_event_state', pet7.state === 'talk');
  pet7.applyEvent({ bubble: 'hello' });
  assert('apply_event_bubble', pet7.bubble === 'hello');
  pet7.applyEvent({ walk_to: 200 });
  assert('apply_event_walk', pet7.state === 'walk' && pet7.targetX === 200);

  return results;
}

if (typeof window !== 'undefined') {
  window.PetState = { PetStateMachine, STATE_CONFIG, runPetTests };
}
