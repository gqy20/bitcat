// sprite.js — 像素精灵数据 + Canvas 渲染（多帧 + 左右翻转）

const SPRITE_W = 16;
const SPRITE_H = 16;

// 调色板：索引 → [r, g, b, a]
const PALETTE = {
  0: null,                                    // 透明
  1: [30, 30, 40, 255],                       // 轮廓
  2: [255, 180, 140, 255],                    // 肤色
  3: [255, 220, 190, 255],                    // 高光
  4: [40, 35, 50, 255],                       // 眼睛
  5: [255, 120, 140, 255],                    // 嘴巴/腮红
};

// 基底帧（站立 + 睁眼 + 抿嘴）
const IDLE_BASE = [
  0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
  0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
  0,0,1,2,2,2,2,1,2,2,2,2,1,0,0,0,
  0,1,1,2,2,2,2,2,2,2,2,2,1,1,0,0,
  0,1,2,2,3,2,2,2,2,2,3,2,2,1,0,0,
  0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
  0,1,2,4,4,2,2,2,2,2,4,4,2,1,0,0,
  0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
  0,1,2,2,2,5,2,2,2,5,2,2,2,1,0,0,
  0,1,2,2,2,2,2,1,2,2,2,2,2,1,0,0,
  0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
  0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,0,
  0,0,0,1,1,2,2,2,2,1,1,0,0,0,0,0,
  0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,0,
  0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,0,
  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

// 修改基底帧的指定像素，返回新数组
function cloneSprite(base, mods) {
  const out = base.slice();
  for (const [row, col, val] of mods) {
    out[row * SPRITE_W + col] = val;
  }
  return out;
}

// idle: 4 帧。睁 → 半眯 → 闭 → 睁
const IDLE_BLINK_HALF = cloneSprite(IDLE_BASE, [
  // 眼睛上半留肤色，下半保留眼黑
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
  [7, 3, 4], [7, 4, 4], [7, 10, 4], [7, 11, 4],
]);
const IDLE_BLINK_CLOSED = cloneSprite(IDLE_BASE, [
  // 眼睛全闭（用肤色覆盖 + 上方留一道暗线）
  [5, 3, 4], [5, 4, 4], [5, 10, 4], [5, 11, 4],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
]);

// walk: 4 帧。底部脚部位置交替
const WALK_FRAME_A = IDLE_BASE;
const WALK_FRAME_B = cloneSprite(IDLE_BASE, [
  // 抬左脚（左下角脚位上移）
  [13, 4, 0], [12, 4, 1],
  [14, 5, 0], [13, 5, 1],
]);
const WALK_FRAME_C = IDLE_BASE;
const WALK_FRAME_D = cloneSprite(IDLE_BASE, [
  // 抬右脚（右下角脚位上移）
  [13, 9, 0], [12, 9, 1],
  [14, 8, 0], [13, 8, 1],
]);

// sleep: 2 帧。常态 → 呼吸内缩
const SLEEP_BASE = cloneSprite(IDLE_BASE, [
  // 闭眼
  [5, 3, 4], [5, 4, 4], [5, 10, 4], [5, 11, 4],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
  // 嘴抿小
  [8, 5, 2], [8, 11, 2],
]);
const SLEEP_BREATHE = cloneSprite(SLEEP_BASE, [
  // 顶部内缩 1 行
  [0, 3, 0], [0, 4, 0], [0, 5, 0], [0, 6, 0], [0, 7, 0], [0, 8, 0], [0, 9, 0], [0, 10, 0],
  [1, 3, 1], [1, 4, 1], [1, 5, 2], [1, 6, 1], [1, 7, 1], [1, 8, 1], [1, 9, 2], [1, 10, 1],
]);

// talk: 3 帧。嘴小开 → 大开 → 关
const TALK_SMALL = cloneSprite(IDLE_BASE, [
  [8, 7, 0], [8, 8, 0], [8, 9, 0],
]);
const TALK_LARGE = cloneSprite(IDLE_BASE, [
  [8, 6, 1], [8, 7, 0], [8, 8, 0], [8, 9, 0], [8, 10, 1],
  [9, 7, 0], [9, 8, 0], [9, 9, 0],
]);
const TALK_CLOSED = cloneSprite(IDLE_BASE, [
  [8, 7, 5], [8, 8, 5], [8, 9, 5],
]);

// happy: 3 帧。笑眼（^_^）+ 大嘴笑，配合渲染层垂直偏移做跳跃
const HAPPY_BASE = cloneSprite(IDLE_BASE, [
  // 弯月眼（上方有一行轮廓 + 下方填肤色）
  [5, 3, 1], [5, 4, 1], [5, 10, 1], [5, 11, 1],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
  // 大嘴
  [8, 6, 1], [8, 7, 5], [8, 8, 5], [8, 9, 5], [8, 10, 1],
  [9, 7, 5], [9, 8, 5], [9, 9, 5],
]);
const HAPPY_BLINK = cloneSprite(HAPPY_BASE, [
  // 闭眼瞬间
  [5, 3, 1], [5, 4, 1], [5, 10, 1], [5, 11, 1],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
]);

// confused: 2 帧。X 眼 + 头歪交替
const CONFUSED_LEFT = cloneSprite(IDLE_BASE, [
  // 左眼 X
  [5, 3, 4], [5, 4, 0], [6, 3, 0], [6, 4, 4],
  // 右眼 X
  [5, 10, 4], [5, 11, 0], [6, 10, 0], [6, 11, 4],
  // 嘴歪
  [8, 5, 5], [8, 6, 5],
]);
const CONFUSED_RIGHT = cloneSprite(IDLE_BASE, [
  [5, 3, 0], [5, 4, 4], [6, 3, 4], [6, 4, 0],
  [5, 10, 0], [5, 11, 4], [6, 10, 4], [6, 11, 0],
  [8, 9, 5], [8, 10, 5],
]);

// ---- 舞蹈动作帧（基于 IDLE_BASE 的变体）----

// jump: 腾空 — 脚部全清 + 笑眼 + 大嘴（渲染层再加 y 上移）
const JUMP_SPRITE = cloneSprite(IDLE_BASE, [
  // 清空脚部 rows 12-14
  [12, 3, 0], [12, 4, 0], [12, 5, 0], [12, 6, 0], [12, 7, 0], [12, 8, 0], [12, 9, 0], [12, 10, 0], [12, 11, 0], [12, 12, 0],
  [13, 4, 0], [13, 5, 0], [13, 6, 0], [13, 7, 0], [13, 8, 0], [13, 9, 0],
  [14, 5, 0], [14, 6, 0], [14, 7, 0], [14, 8, 0], [14, 9, 0],
  // 笑眼弯月
  [5, 3, 1], [5, 4, 1], [5, 10, 1], [5, 11, 1],
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
  // 大嘴笑
  [8, 6, 1], [8, 7, 5], [8, 8, 5], [8, 9, 5], [8, 10, 1],
  [9, 7, 5], [9, 8, 5], [9, 9, 5],
]);

// spin: 高速旋转 — 激动大眼 + 大张嘴 + 角落速度线（渲染层快速翻转 facingRight）
const SPIN_SPRITE = cloneSprite(IDLE_BASE, [
  // 角落速度线像素
  [1, 1, 1], [1, 14, 1],
  [2, 0, 1], [2, 15, 1],
  // 眼睛放大一圈（激动感）
  [4, 2, 1], [4, 3, 2], [4, 4, 3], [4, 5, 2], [4, 6, 1],
  [4, 10, 1], [4, 11, 2], [4, 12, 3], [4, 13, 2], [4, 14, 1],
  // 大张嘴
  [8, 6, 1], [8, 7, 0], [8, 8, 0], [8, 9, 0], [8, 10, 1],
  [9, 7, 0], [9, 8, 5], [9, 9, 0],
]);

// wave: 挥手 — 左侧大面积清空模拟抬爪 + 眯眼微笑
const WAVE_SPRITE = cloneSprite(IDLE_BASE, [
  // 左侧抬爪：清空左耳+左肩 (cols 0-3, rows 0-5)
  [0, 2, 0], [0, 3, 0],
  [1, 1, 0], [1, 2, 0], [1, 3, 0], [1, 4, 0],
  [2, 1, 0], [2, 2, 0], [2, 3, 0],
  [3, 0, 0], [3, 1, 0], [3, 2, 0], [3, 3, 0],
  [4, 0, 0], [4, 1, 0], [4, 2, 0],
  [5, 0, 0], [5, 1, 0],
  // 左眼眯成笑眼
  [5, 3, 1], [5, 4, 1],
  [6, 3, 2], [6, 4, 2],
  // 微笑嘴
  [8, 7, 5], [8, 8, 5], [8, 9, 5],
]);

// shake: 晃动 — X 眼 + 波浪嘴 + 加深腮红
const SHAKE_SPRITE = cloneSprite(IDLE_BASE, [
  // X 眼
  [5, 3, 4], [5, 4, 0], [6, 3, 0], [6, 4, 4],
  [5, 10, 4], [5, 11, 0], [6, 10, 0], [6, 11, 4],
  // 波浪嘴 ~ 形
  [8, 5, 5], [8, 6, 0], [8, 7, 5], [8, 8, 5], [8, 9, 0], [8, 10, 5],
  [9, 6, 5], [9, 7, 0], [9, 8, 5], [9, 9, 5],
]);

// 多帧精灵：每个状态对应一个帧数组（每帧仍是 256 像素）
const SPRITES = {
  idle:     [IDLE_BASE, IDLE_BLINK_HALF, IDLE_BLINK_CLOSED, IDLE_BASE],
  walk:     [WALK_FRAME_A, WALK_FRAME_B, WALK_FRAME_C, WALK_FRAME_D],
  sleep:    [SLEEP_BASE, SLEEP_BREATHE],
  talk:     [TALK_SMALL, TALK_LARGE, TALK_CLOSED],
  happy:    [HAPPY_BASE, HAPPY_BLINK, HAPPY_BASE],
  confused: [CONFUSED_LEFT, CONFUSED_RIGHT],
  // 舞蹈动作（单帧，由舞蹈播放器控制时长）
  jump:     [JUMP_SPRITE],
  spin:     [SPIN_SPRITE],
  wave:     [WAVE_SPRITE],
  shake:    [SHAKE_SPRITE],
};

// 取出指定状态的指定帧（越界自动取模）
function getSprite(state, frame) {
  const frames = SPRITES[state] || SPRITES.idle;
  const f = frame == null ? 0 : ((frame % frames.length) + frames.length) % frames.length;
  return frames[f];
}

// 渲染指定帧到 Canvas，可左右翻转 + 像素偏移
// opts: { offsetX, offsetY } — 用于舞蹈动画（jump 上移、shake 抖动）
function renderSprite(ctx, state, frame, facingRight, scale, opts) {
  scale = scale || 8;
  opts = opts || {};
  var ox = opts.offsetX || 0;
  var oy = opts.offsetY || 0;
  const data = getSprite(state, frame);
  const totalW = SPRITE_W * scale;
  const totalH = SPRITE_H * scale;

  ctx.clearRect(0, 0, totalW, totalH);

  ctx.save();
  if (facingRight === false) {
    ctx.translate(totalW, 0);
    ctx.scale(-1, 1);
  }

  for (let row = 0; row < SPRITE_H; row++) {
    for (let col = 0; col < SPRITE_W; col++) {
      const idx = row * SPRITE_W + col;
      const color = PALETTE[data[idx]];
      if (color) {
        ctx.fillStyle = `rgba(${color[0]},${color[1]},${color[2]},${color[3] / 255})`;
        ctx.fillRect(col * scale + ox, row * scale + oy, scale, scale);
      }
    }
  }
  ctx.restore();
}

// 折叠态：只画猫头（取精灵上半部分，缩放到 48×48）
function renderMini(ctx, state) {
  const data = getSprite(state, 0); // 始终用第 0 帧
  const miniScale = 3; // 16px × 3 = 48px
  const headRows = 10; // 只画前 10 行（猫头区域）

  ctx.clearRect(0, 0, 48, 48);

  // 居中偏移：让猫头在 48×48 中居中
  const offsetY = Math.floor((48 - headRows * miniScale) / 2);

  for (let row = 0; row < headRows; row++) {
    for (let col = 0; col < SPRITE_W; col++) {
      const idx = row * SPRITE_W + col;
      const color = PALETTE[data[idx]];
      if (color) {
        ctx.fillStyle = `rgba(${color[0]},${color[1]},${color[2]},${color[3] / 255})`;
        ctx.fillRect(
          col * miniScale,
          offsetY + row * miniScale,
          miniScale,
          miniScale
        );
      }
    }
  }
}

// 测试函数（test.html 调用）
function runSpriteTests() {
  const results = [];

  function assert(name, condition) {
    results.push({ name, pass: !!condition });
  }

  // 每个状态都是多帧数组，且帧像素长度正确
  for (const [name, frames] of Object.entries(SPRITES)) {
    assert(`sprite_${name}_is_array`, Array.isArray(frames));
    assert(`sprite_${name}_non_empty`, frames.length > 0);
    for (let i = 0; i < frames.length; i++) {
      assert(`sprite_${name}_frame_${i}_length`, frames[i].length === SPRITE_W * SPRITE_H);
      assert(`sprite_${name}_frame_${i}_valid_range`, frames[i].every(v => v >= 0 && v <= 5));
    }
  }

  // 调色板
  assert('palette_0_transparent', PALETTE[0] === null);
  assert('palette_1_has_color', PALETTE[1] !== null && PALETTE[1].length === 4);
  assert('palette_2_has_color', PALETTE[2] !== null && PALETTE[2].length === 4);

  // getSprite
  assert('get_sprite_idle_frame_0', getSprite('idle', 0) === SPRITES.idle[0]);
  assert('get_sprite_frame_wrap', getSprite('idle', SPRITES.idle.length) === SPRITES.idle[0]);
  assert('get_sprite_negative_wrap', getSprite('idle', -1) === SPRITES.idle[SPRITES.idle.length - 1]);
  assert('get_sprite_unknown_fallback', getSprite('unknown', 0) === SPRITES.idle[0]);

  // 多帧动画特征
  // idle 帧 2（闭眼）：眼睛位置 (6, 3) 是肤色 2，不再是 4
  assert('idle_blink_eye_closed', SPRITES.idle[2][6 * SPRITE_W + 3] === 2);
  // talk 帧 1（大开）：(9, 8) 是透明 0
  assert('talk_large_mouth', SPRITES.talk[1][9 * SPRITE_W + 8] === 0);
  // sleep 第 2 帧顶部内缩
  assert('sleep_breathe_top_clear', SPRITES.sleep[1][0 * SPRITE_W + 5] === 0);
  // happy 第 1 帧大嘴
  assert('happy_big_mouth', SPRITES.happy[0][9 * SPRITE_W + 8] === 5);
  // walk B 帧抬脚
  assert('walk_lift_foot', SPRITES.walk[1][12 * SPRITE_W + 4] === 1);

  // 状态帧数
  assert('idle_4_frames', SPRITES.idle.length === 4);
  assert('walk_4_frames', SPRITES.walk.length === 4);
  assert('sleep_2_frames', SPRITES.sleep.length === 2);
  assert('talk_3_frames', SPRITES.talk.length === 3);
  assert('happy_3_frames', SPRITES.happy.length === 3);
  assert('confused_2_frames', SPRITES.confused.length === 2);

  return results;
}

// 导出（浏览器环境）
if (typeof window !== 'undefined') {
  window.SpriteRenderer = {
    SPRITES, PALETTE, SPRITE_W, SPRITE_H,
    getSprite, renderSprite, renderMini, runSpriteTests, cloneSprite,
  };
}
