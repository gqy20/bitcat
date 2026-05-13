# 桌宠动画系统优化研究：借鉴 OpenAI Codex CLI 宠物架构

> **日期**: 2026-05-13
>
> ## 项目位置关系
>
> ```
> D:\C\Desktop\ai\
> ├── codex/                          ← 参考项目（本文档分析的源码来源）
> │   └── codex-rs/tui/src/pets/      ← Codex 宠物系统全部源码在此目录下
> │       ├── mod.rs                  #   图像渲染管线（Kitty/Sixel）
> │       ├── model.rs                #   ★ 核心数据模型：Pet/Animation/AnimationFrame
> │       ├── ambient.rs              #   ★ 运行时动画状态机 + 通知系统
> │       ├── catalog.rs              #   8 个内置宠物目录
> │       ├── frames.rs               #   精灵表切片（PNG 帧提取）
> │       ├── asset_pack.rs           #   CDN 资产下载与缓存
> │       ├── image_protocol.rs       #   终端图像协议检测
> │       ├── sixel.rs                #   Sixel 编码器
> │       ├── picker.rs               #   /pets 选择弹窗
> │       └── preview.rs              #   选择器预览
> │
> └── 8bit/                           ← 目标项目（本文档提出的优化对象）
>     ├── core/src/pet.rs             #   ★ Rust 状态机（与 pet.js 镜像）
>     ├── core/src/dance.rs           #   舞蹈定义与 YML 持久化
>     ├── core/src/bridge.rs          #   IPC 桥接：事件→命令转换
>     └── app/frontend/js/
>         ├── sprite.js               #   ★ 像素数据 + Canvas 渲染
>         ├── pet.js                  #   ★ JS 状态机（与 pet.rs 镜像）
>         └── app.js                  #   主循环 + 舞蹈播放器 + 交互
> ```
>
> **本文档中的路径约定：**
> - Codex 源码引用格式：`📄 pets/文件名:行号` （相对于 `codex/codex-rs/tui/src/`）
> - 8Bit 源码引用格式：`📄 文件名:行号` （相对于各自目录，见上方树形结构）
> - 正文中用 `<!-- CODEX:文件名:行号 -->` 和 `<!-- 8BIT:文件名:行号 -->` 标注精确位置

---

## 目录

1. [两项目现状对比](#1-两项目现状对比)
2. [优化方向一：非均匀帧时长（时间轴驱动）](#2-优化方向一非均匀帧时长时间轴驱动)
3. [优化方向二："三遍 + 回落"状态过渡模式](#3-优化方向二三遍--回落状态过渡模式)
4. [优化方向三：通知驱动的状态机 + 生命周期](#4-优化方向三通知驱动的状态机--生命周期)
5. [优化方向四：精灵表外化 + JSON Manifest](#5-优化方向四精灵表外化--json-manifest)
6. [优化方向五：Idle 环境变化（Ambient Variants）](#6-优化方向五idle-环境变化ambient-variants)
7. [优化方向六：Dance 纳入统一状态机](#7-优化方向六dance-纳入统一状态机)
8. [实施优先级与风险评估](#8-实施优先级与风险评估)

---

## 1. 两项目现状对比

### 1.1 技术栈差异

| 维度 | Codex CLI 宠物 | 8Bit Cat |
|------|---------------|----------|
| **运行环境** | 终端 TUI (Ratatui) | 桌面窗口 (Tauri 2 + WebView2) |
| **渲染方式** | Kitty/Sixel 终端图像协议 | HTML5 Canvas 2D (`fillRect`) |
| **语言** | Rust | Rust (core) + JavaScript (frontend) |
| **帧数据来源** | 外部 `.webp` 精灵表 + JSON manifest | JS 源码内硬编码 `int[256]` 像素数组 |
| **分辨率** | 192x208px / 帧, 72 帧 | 16x16px / 帧, ~20 帧 |

### 1.2 动画引擎核心差异

**Codex — 时间轴查表（elapsed-driven）:**
<!-- CODEX:ambient.rs:376-412 -->

```rust
// 📄 pets/ambient.rs:376-412  — 核心时间轴查表函数
fn current_animation_frame(animation: &Animation, elapsed: Duration) -> Option<AnimationFrameTick> {
    let mut remaining_elapsed = elapsed.as_nanos();
    for frame in &animation.frames {
        let frame_nanos = frame.duration.as_nanos().max(1);
        if remaining_elapsed < frame_nanos {
            // 还没到下一帧 → 当前就显示这一帧，返回距离切换的剩余时间
            return Some(AnimationFrameTick {
                sprite_index: frame.sprite_index,
                delay: Some(nanos_to_duration(frame_nanos - remaining_elapsed)),
            });
        }
        remaining_elapsed = remaining_elapsed.saturating_sub(frame_nanos);
    }
    // 所有帧放完 → 显示最后一帧
    Some(AnimationFrameTick { sprite_index: animation.last()?.sprite_index, delay: None })
}
```

关键特性：
- **每帧有独立 duration** — 不要求均匀
- **基于绝对时间定位** — 掉帧时自动跳到正确位置，不会"追赶"
- **返回 delay** — 调用方知道何时该请求下一帧

**8Bit — 帧计数器累加（counter-driven）:**
<!-- 8BIT:pet.js:42-51 -->

```javascript
// 📄 pet.js:42-51  — JS 侧帧计数器（均匀帧长）
update(dtMs) {
    this.stateTimeMs += dtMs;
    this.frameTimeMs += dtMs;

    const config = STATE_CONFIG[this.state] || STATE_CONFIG.idle;
    while (this.frameTimeMs >= config.frameDuration) {
        this.frameTimeMs -= config.frameDuration;
        this.frame = (this.frame + 1) % config.frameCount;
    }
    // ...
}
```

```rust
// 📄 core/src/pet.rs:109-118  — Rust 侧帧计数器（与 JS 逻辑镜像）
pub fn update(&mut self, dt_ms: u64) {
    self.state_time_ms += dt_ms;
    self.frame_time_ms += dt_ms;
    let duration = self.state.frame_duration_ms();
    while self.frame_time_ms >= duration {
        self.frame_time_ms -= duration;
        self.frame = (self.frame + 1) % self.state.frame_count();
    }
}
```

关键特性：
- **所有帧等长** — `frameDuration` 是每个状态一个固定值
- **水桶接水模式** — 累加到阈值就进帧，多出的时间带入下一轮
- **无掉帧保护** — 如果 dt 很大（如标签页切回来），会连续跳多帧

### 1.3 状态管理核心差异

**Codex — 通知驱动 + 生命周期:**
<!-- CODEX:ambient.rs:46-111 -->

```rust
// 📄 pets/ambient.rs:46-90  — 通知类型定义 + 生命周期
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PetNotificationKind {
    Running,   // Agent 执行中
    Waiting,   // 等待用户输入
    Review,    // 待审核
    Failed,    // 出错/阻塞
}

impl PetNotificationKind {
    fn lifetime(self) -> Duration {
        match self {
            Self::Running => Duration::from_secs(3 * 60),    // 3 分钟
            Self::Waiting => Duration::from_secs(24 * 60 * 60), // 24 小时
            Self::Review => Duration::from_secs(7 * 24 * 60 * 60), // 7 天
            Self::Failed => Duration::from_secs(60 * 60),     // 1 小时
        }
    }

    fn animation_name(self) -> &'static str { /* ... */ }
}

// 📄 pets/ambient.rs:277-281  — 通知过期检测
fn visible_notification(&self, now: Instant) -> Option<&PetNotification> {
    self.notification.as_ref()
        .filter(|notification| !notification.is_expired(now))
}
```

**8Bit — 固定超时回落:**
<!-- 8BIT:pet.js:4-68 -->

```javascript
// 📄 pet.js:4-10  — 状态配置（均匀帧长 + 固定超时）
const STATE_CONFIG = {
    idle:     { frameCount: 4, frameDuration: 500, autoIdleTimeout: null },
    walk:     { frameCount: 4, frameDuration: 150, autoIdleTimeout: 3000 },
    sleep:    { frameCount: 2, frameDuration: 800, autoIdleTimeout: null },
    talk:     { frameCount: 3, frameDuration: 300, autoIdleTimeout: 5000 },
    happy:    { frameCount: 3, frameDuration: 200, autoIdleTimeout: 2000 },
    confused: { frameCount: 2, frameDuration: 400, autoIdleTimeout: 3000 },
};

// 📄 pet.js:65-68  — 自动超时回落
if (config.autoIdleTimeout !== null && this.stateTimeMs >= config.autoIdleTimeout) {
    this.setState('idle');
}
```

### 1.4 数据模型差异

**Codex — 外部 manifest 驱动:**
<!-- CODEX:model.rs:61-72 -->

```rust
// 📄 pets/model.rs:61-72  — Pet 核心数据结构
pub struct Pet {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub spritesheet_path: PathBuf,      // 外部图片文件
    pub frame_width: u32,               // 192
    pub frame_height: u32,              // 208
    pub columns: u32,                   // 8
    pub rows: u32,                      // 9
    pub frame_count: usize,             // 72
    pub animations: HashMap<String, Animation>,  // 从 JSON 加载
}
```

```rust
// 📄 pets/model.rs:32-43  — Animation / AnimationFrame 结构体
pub struct AnimationFrame {
    pub sprite_index: usize,
    pub duration: Duration,              // ← 每帧独立时长
}

pub struct Animation {
    pub frames: Vec<AnimationFrame>,
    pub loop_start: Option<usize>,      // None=一次性, Some(n)=从第n帧循环
    pub fallback: String,               // 一次性结束后的回落动画
}
```

自定义宠物 JSON manifest 格式：

```json
// 📄 pets/model.rs (文档注释中的 JSON 示例)
{
    "id": "chefito",
    "displayName": "Chefito",
    "description": "A tiny recipe-loving chef",
    "spritesheetPath": "spritesheet.webp",
    "frame": { "width": 192, "height": 208, "columns": 8, "rows": 9 },
    "animations": {
        "idle": { "frames": [0, 1, 2], "fps": 8.0, "loop": true, "fallback": "idle" },
        "wave": { "frames": [3, 4], "fps": 4.0, "loop": false, "fallback": "idle" }
    }
}
```

**8Bit — 内联像素数据:**
<!-- 8BIT:sprite.js:17-195 -->

```javascript
// 📄 sprite.js:17-34  — 基底帧：256 个 palette index
const IDLE_BASE = [
  0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
  0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
  // ... 共 16×16 = 256 个 palette index
];

// 📄 sprite.js:37-43  — 帧派生工具函数
function cloneSprite(base, mods) {
  const out = base.slice();
  for (const [row, col, val] of mods) {
    out[row * SPRITE_W + col] = val;
  }
  return out;
}

// 📄 sprite.js:183-195  — 状态名 → 帧数组映射
const SPRITES = {
  idle:     [IDLE_BASE, IDLE_BLINK_HALF, IDLE_BLINK_CLOSED, IDLE_BASE],
  walk:     [WALK_FRAME_A, WALK_FRAME_B, WALK_FRAME_C, WALK_FRAME_D],
  sleep:    [SLEEP_BASE, SLEEP_BREATHE],
  talk:     [TALK_SMALL, TALK_LARGE, TALK_CLOSED],
  happy:    [HAPPY_BASE, HAPPY_BLINK, HAPPY_BASE],
  confused: [CONFUSED_LEFT, CONFUSED_RIGHT],
  jump:     [JUMP_SPRITE],       // 舞蹈动作全部单帧
  spin:     [SPIN_SPRITE],
  wave:     [WAVE_SPRITE],
  shake:    [SHAKE_SPRITE],
};
```

---

## 2. 优化方向一：非均匀帧时长（时间轴驱动）

### 2.1 问题诊断

当前 8Bit 的 idle 动画是 4 帧 × 500ms 均匀循环：

```
现状:  睁眼(500ms) → 半眯(500ms) → 闭眼(500ms) → 睁眼(500ms) → 循环
       ↑ 机械匀速，像节拍器而不像生物呼吸
```

真实猫的眨眼特征：**大部分时间睁着，眨眼是一瞬间的动作**。

Codex 的 idle 动画展示了非均匀帧时长的威力：
<!-- CODEX:model.rs:584-596 -->

```rust
// 📄 pets/model.rs:584-596  — Idle 呼吸动画（非均匀帧时长！）
fn idle_animation() -> Animation {
    Animation {
        frames: [(0, 1680), (1, 660), (2, 660), (3, 840), (4, 840), (5, 1920)]
            .into_iter()
            .map(|(sprite_index, duration_ms)| AnimationFrame {
                sprite_index,
                duration: Duration::from_millis(duration_ms),
            })
            .collect(),
        loop_start: Some(0),
        fallback: "idle".to_string(),
    }
}
```

```
Codex idle:  睁(1680ms) → 半闭(660ms) → 闭合(840ms) → 半睁(840ms) → 睁(1920ms) → 循环
           ↑ 悠闲停留    ↑ 快速过渡    ↑ 屏息感     ↑ 恢复中     ↑ 深吸气
           总周期 ≈ 6.6 秒，节奏像真实呼吸
```

### 2.2 改造方案

#### Step 1: STATE_CONFIG 支持 per-frame durations

**Before (当前):**

```javascript
// pet.js:4-10
const STATE_CONFIG = {
    idle: { frameCount: 4, frameDuration: 500, autoIdleTimeout: null },
    // ...
};
```

**After (目标):**

```javascript
const STATE_CONFIG = {
    idle: {
        frames: [
            { spriteIndex: 0, duration: 1500 },  // 睁眼 - 悠闲
            { spriteIndex: 1, duration: 120 },   // 半眯 - 快速
            { spriteIndex: 2, duration: 200 },   // 闭眼 - 短暂
            { spriteIndex: 1, duration: 120 },   // 半眯 - 快速恢复
            { spriteIndex: 0, duration: 1800 },  // 睁眼 - 深呼吸停顿
        ],
        loopStart: 0,          // 从头循环
        fallback: 'idle',      // （循环动画不使用）
        autoIdleTimeout: null,
    },
    happy: {
        frames: [
            { spriteIndex: 0, duration: 300 },   // 笑眼大嘴
            { spriteIndex: 1, duration: 150 },   // 眨眼
            { spriteIndex: 0, duration: 250 },   // 回笑
            { spriteIndex: 0, duration: 300 },   // 笑（重复引起注意）
            { spriteIndex: 0, duration: 300 },   // 笑（再重复）
            // ↓ 下面接入 idle 的帧序列作为回落
            { spriteIndex: 0, duration: 1500 },  // idle 睁眼
            { spriteIndex: 1, duration: 120 },
            { spriteIndex: 2, duration: 200 },
            { spriteIndex: 1, duration: 120 },
            { spriteIndex: 0, duration: 1800 },
        ],
        loopStart: 5,             // 前 5 帧播完后，从 index 5 开始循环（即 idle 部分）
        fallback: 'idle',
        autoIdleTimeout: null,    // 由 loopStart + fallback 接管，不再需要超时
    },
    // ... 其他状态类似改造
};
```

#### Step 2: update() 方法改为时间轴查表

**Before (📄 pet.js:42-51):**

```javascript
// 📄 pet.js:42-51  — 现有帧计数器逻辑
update(dtMs) {
    this.frameTimeMs += dtMs;
    const config = STATE_CONFIG[this.state];
    while (this.frameTimeMs >= config.frameDuration) {
        this.frameTimeMs -= config.frameDuration;
        this.frame = (this.frame + 1) % config.frameCount;
    }
}
```

**After:**

```javascript
update(dtMs) {
    this.stateTimeMs += dtMs;
    this.frameTimeMs += dtMs;  // 从进入当前状态（或当前循环圈）开始的累计时间

    const config = STATE_CONFIG[this.state] || STATE_CONFIG.idle;
    const frames = config.frames;

    // 时间轴查表：根据 elapsed 时间定位当前该显示哪一帧
    let elapsed = this.frameTimeMs;
    for (let i = 0; i < frames.length; i++) {
        if (elapsed < frames[i].duration) {
            // 还在这一帧内
            if (this.frame !== i) {
                this.frame = i;  // 帧切换
            }
            return;  // 不消耗时间，等下次 update 自然推进
        }
        elapsed -= frames[i].duration;
    }

    // 所有帧都放完了
    if (config.loopStart !== undefined && config.loopStart !== null) {
        // 循环动画：回退到 loopStart，重置 frameTimeMs 为循环部分的偏移
        const loopDuration = frames.slice(config.loopStart)
            .reduce((sum, f) => sum + f.duration, 0);
        if (loopDuration > 0) {
            this.frameTimeMs = config.loopStart > 0
                ? frames.slice(0, config.loopStart).reduce((s, f) => s + f.duration, 0) + (elapsed % loopDuration)
                : this.frameTimeMs % (frames.reduce((s, f) => s + f.duration, 0));
            // 下一帧会重新走查表逻辑，自然落到循环区域
        }
        this.frame = config.loopStart;
    } else {
        // 一次性动画：停在最后一帧，或切换到 fallback
        this.frame = frames.length - 1;
        if (config.fallback && config.fallback !== this.state) {
            this.setState(config.fallback);
        }
    }
}
```

#### Step 3: Rust core 同步修改

```rust
// 📄 core/src/pet.rs  — 新增：时间轴动画配置结构体

#[derive(Debug, Clone)]
pub struct AnimationFrameDef {
    pub sprite_index: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct StateAnimationConfig {
    pub frames: Vec<AnimationFrameDef>,
    pub loop_start: Option<usize>,   // None = one-shot
    pub fallback: Option<PetState>,  // one-shot 结束后切换到的状态
    pub auto_idle_timeout_ms: Option<u64>,  // 兼容旧逻辑的兜底
}

impl PetState {
    /// 返回完整的时间轴动画配置（替代旧的 frame_count + frame_duration_ms）
    pub fn animation_config(self) -> StateAnimationConfig {
        match self {
            PetState::Idle => StateAnimationConfig {
                frames: vec![
                    AnimationFrameDef { sprite_index: 0, duration_ms: 1500 },
                    AnimationFrameDef { sprite_index: 1, duration_ms: 120 },
                    AnimationFrameDef { sprite_index: 2, duration_ms: 200 },
                    AnimationFrameDef { sprite_index: 1, duration_ms: 120 },
                    AnimationFrameDef { sprite_index: 0, duration_ms: 1800 },
                ],
                loop_start: Some(0),
                fallback: None,
                auto_idle_timeout_ms: None,
            },
            // ... 其他状态
        }
    }
}
```

### 2.3 效果对比

| 维度 | Before | After |
|------|--------|-------|
| Idle 节奏 | 机械 500ms 匀速眨眼 | 自然呼吸（快慢交替，~5.7s/周期） |
| Happy 表现 | 3 帧 x 200ms = 0.6s 就没了 | 主动作 2 遍 + idle 回落 ≈ 3.3s 有存在感 |
| 掉帧恢复 | 连续追赶（可能闪过中间帧） | 时间轴直接定位正确帧 |
| 扩展性 | 加帧要改 JS 数组 | 加帧只需在 frames 数组里插入一项 |

---

## 3. 优化方向二："三遍 + 回落"状态过渡模式

### 3.1 问题诊断

当前状态切换行为：

```
用户夸奖 → Happy(200ms×3帧) → 2秒后突然切断 → Idle
         ↑ 太短了，用户可能都没看清猫笑了

AI 报错 → Confused(400ms×2帧) → 3秒后突然切断 → Idle
         ↑ 3 秒后猫就一脸没事了，但 AI 可能还在报错
```

### 3.2 Codex 的做法
<!-- CODEX:model.rs:598-627 -->

```rust
// 📄 pets/model.rs:598-627  — "三遍+回落"工厂函数（核心！）
fn app_state_animation(
    row_index: usize,
    frame_count: usize,
    frame_duration_ms: u64,
    final_frame_duration_ms: u64,
) -> Animation {
    let primary_frames = (0..frame_count)
        .map(|column_index| AnimationFrame { /* ... */ })
        .collect::<Vec<_>>();
    let primary_frame_count = primary_frames.len() * 3;  // ← 播 3 遍
    let frames = primary_frames
        .iter()
        .chain(primary_frames.iter())    // 第 2 遍
        .chain(primary_frames.iter())    // 第 3 遍
        .chain(idle_animation().frames)  // ← 回落到 idle
        .collect();
    Animation {
        frames,
        loop_start: Some(primary_frame_count),  // ← 从 idle 部分开始循环
        fallback: "idle".to_string(),
    }
}
```

生成的 running 动画帧序列（测试验证于 `model.rs:691-708`）：

```
[running 6帧] → [running 6帧] → [running 6帧] → [idle 6帧] → 循环 idle 部分
 ↑ 第1遍         ↑ 第2遍         ↑ 第3遍         ↑ 回落
 引起注意        强化印象        最后确认        安静下来
```

### 3.3 应用到 8Bit

将"三遍 + 回落"编码为通用的**状态动画构建函数**：

```javascript
/**
 * 构建状态动画：主序列重复 N 遍 + idle 回落
 * @param {Array} primaryFrames - 主动画帧 [{spriteIndex, duration}, ...]
 * @param {number} repeatCount - 重复次数（默认 3）
 * @param {Array} idleFrames - idle 帧序列
 * @returns {{frames: Array, loopStart: number, fallback: string}}
 */
function buildStateAnimation(primaryFrames, repeatCount = 3, idleFrames) {
    const frames = [];
    for (let r = 0; r < repeatCount; r++) {
        frames.push(...primaryFrames);
    }
    const loopStart = frames.length;  // idle 从这里开始
    frames.push(...idleFrames);

    return {
        frames,
        loopStart,       // 播完 repeatCount 遍后，从这里循环
        fallback: 'idle',
    };
}

// 应用示例
STATE_CONFIG.happy = buildStateAnimation(
    [
        { spriteIndex: 0, duration: 250 },  // 笑眼大嘴
        { spriteIndex: 1, duration: 120 },  // 眨眼
        { spriteIndex: 0, duration: 230 },  // 回笑
    ],
    3,  // 播 3 遍 ≈ 1.77s 的开心表现
    STATE_CONFIG.idle.frames  // 回落到 idle 呼吸
);
// 结果帧序列: [happy×3] + [idle×5], loopStart=9, 之后循环 idle 部分
```

**效果对比：**

```
Before:  Happy(0.6s) ──2s硬超时──→ Idle（突兀切断）
After:   Happy(1.8s) ──平滑过渡──→ Idle breathing（自然回落）
```

---

## 4. 优化方向三：通知驱动的状态机 + 生命周期

### 4.1 问题诊断

当前超时值是**拍脑袋的毫秒数**，没有语义：

```javascript
// 📄 pet.js:5-10  — 现有超时值（语义不明）
walk:     { autoIdleTimeout: 3000 },   // 为什么是 3 秒？
talk:     { autoIdleTimeout: 5000 },   // 为什么是 5 秒？
happy:    { autoIdleTimeout: 2000 },   // 为什么是 2 秒？
confused: { autoIdleTimeout: 3000 },   // 为什么是 3 秒？
```

Confused 3 秒后猫就一脸没事了——但 AI 可能还在报错状态。

### 4.2 Codex 的通知模型
<!-- CODEX:ambient.rs:46-111 (完整引用) -->

```rust
// 📄 pets/ambient.rs:46-111  — 通知类型 + PetNotification 结构 + 过期检测（完整版）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PetNotificationKind {
    Running,   // Agent 在执行代码
    Waiting,   // 等待用户输入
    Review,    // 完成待审核
    Failed,    // 出错/阻塞
}

impl PetNotificationKind {
    fn lifetime(self) -> Duration {  /* 语义化时长 */ }
    fn animation_name(self) -> &'static str { /* 映射到动画名 */ }
    fn label(self) -> &'static str { /* UI 显示文本 */ }
}

#[derive(Debug, Clone)]
struct PetNotification {
    kind: PetNotificationKind,
    body: String,
    updated_at: Instant,           // 通知创建/刷新时间
}

impl PetNotification {
    fn is_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.updated_at) >= self.kind.lifetime()
    }
}
```

**关键设计：通知可刷新。** 如果 Agent 持续在运行，Running 通知会不断被 refresh，所以动画不会回落。

### 4.3 应用到 8Bit

#### 定义通知类型

```javascript
// 通知定义
const NOTIFICATIONS = {
    ai_thinking: {
        animation: 'talk',        // 显示 talk 动画
        lifetime_ms: 30_000,       // 30 秒（AI 通常几秒内回复）
        label: '思考中...',
        refreshable: true,         // 每个 AI token 都刷新
    },
    ai_error: {
        animation: 'confused',
        lifetime_ms: 15_000,       // 15 秒足够用户看到错误表情
        label: '出错了...',
        refreshable: true,         // 连续错误则持续显示
    },
    user_praise: {
        animation: 'happy',
        lifetime_ms: 5_000,        // 5 秒开心
        label: '开心!',
        refreshable: false,        // 夸一次就一次
    },
    walking: {
        animation: 'walk',
        lifetime_ms: null,          // 到达目的地才过期
        label: '',
        refreshable: false,
    },
};
```

#### PetStateMachine 加入通知系统

```javascript
class PetStateMachine {
    constructor() {
        // ... 现有属性
        this.notification = null;   // { kind, body, createdAt }
    }

    /** 设置通知（外部事件调用） */
    setNotification(kind, body) {
        const def = NOTIFICATIONS[kind];
        if (!def) return;

        this.notification = {
            kind,
            body: body || def.label,
            createdAt: performance.now(),
        };

        // 切换到对应动画
        this.setState(def.animation);
        this.frameTimeMs = 0;  // 重置动画时间轴
    }

    /** 刷新通知（延长生命周期） */
    refreshNotification(kind) {
        if (this.notification && this.notification.kind === kind) {
            this.notification.createdAt = performance.now();
        }
    }

    /** 每帧检查通知是否过期 */
    update(dtMs) {
        this.stateTimeMs += dtMs;
        this.frameTimeMs += dtMs;

        // 检查通知过期
        if (this.notification) {
            const def = NOTIFICATIONS[this.notification.kind];
            const age = performance.now() - this.notification.createdAt;
            if (def.lifetime_ms !== null && age >= def.lifetime_ms) {
                this.notification = null;
                this.setState('idle');  // 过期 → idle
            }
        }

        // 时间轴帧推进（使用优化方向一的逻辑）
        this.advanceFrame();
    }
}
```

#### 事件源映射

```javascript
// 📄 app.js  — 事件监听改为通知驱动
window.__TAURI__.event.listen('pet-event', (event) => {
    const payload = event.payload;

    if (payload.notification) {
        // 新的通知驱动模式
        pet.setNotification(payload.notification.kind, payload.notification.body);
    } else if (payload.state) {
        // 兼容旧模式：直接设状态（用于 sleep 等持久状态）
        pet.setState(payload.state);
    }

    if (payload.bubble) pet.bubble = payload.bubble;
    if (payload.walk_to != null) pet.walkTo(payload.walk_to);
});
```

**Rust bridge 对应修改：**

```rust
// 📄 core/src/bridge.rs  — 新增通知类型（需改动）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PetNotification {
    AiThinking { body: String },
    AiError { body: String },
    UserPraise,
}

// gamepad.rs / AI 回复处理中发送通知而非直接 SetState
let event = PetEvent {
    notification: Some(PetNotification::AiThinking {
        body: "思考中...".into(),
    }),
    ..Default::default()
};
```

### 4.4 效果对比

| 场景 | Before | After |
|------|--------|-------|
| AI 连续报错 | Confused 3s → Idle（反复闪烁） | Confused 持续显示（每次错误 refresh），15s 无新错误才回落 |
| 用户连续夸奖 | Happy 2s → Idle → Happy 2s → Idle（抖动） | 每次 praise 更新通知，Happy 平滑持续 |
| Walk 到达前 | Walk 3s 强制 Idle（没走到） | Walk 直到到达才过期（lifetime=null + 到达检测） |

---

## 5. 优化方向四：精灵表外化 + JSON Manifest

### 5.1 问题诊断

当前添加/修改任何帧都需要编辑 `sprite.js` 中的像素数组：

```javascript
// 📄 sprite.js:98-105  — 改表情必须手敲像素坐标（痛点）
const HAPPY_BASE = cloneSprite(IDLE_BASE, [
  [5, 3, 1], [5, 4, 1], [5, 10, 1], [5, 11, 1],     // 弯月眼
  [6, 3, 2], [6, 4, 2], [6, 10, 2], [6, 11, 2],
  [8, 6, 1], [8, 7, 5], [8, 8, 5], [8, 9, 5], [8, 10, 1],  // 大嘴
  [9, 7, 5], [9, 8, 5], [9, 9, 5],
]);
```

无法换皮肤、无法用 Aseprite 编辑、无法分享宠物。

### 5.2 Codex 的资产管线

```
CDN 下载 → 本地缓存 → 切帧提取(PNG) → 运行时按需加载
                    ↓
            codex-rs/tui/src/pets/frames.rs
            codex-rs/tui/src/pets/asset_pack.rs
```

```rust
// 📄 pets/model.rs:102-111  — 缓存键 = SHA256 + 维度（精灵表变更自动失效）
pub(super) fn frame_cache_key(&self) -> Result<String> {
    let bytes = fs::read(&self.spritesheet_path)?;
    let digest = Sha256::digest(&bytes);
    Ok(format!(
        "sha256-{digest:x}-{}x{}-{}x{}",
        self.frame_width, self.frame_height, self.columns, self.rows
    ))
}
```

### 5.3 应用到 8Bit 的方案

考虑到 8Bit 是桌面应用（非终端），不需要 Sixel/Kitty 编码，但可以借鉴**外部数据驱动**的思想。

#### 方案 A：精灵条 PNG + JSON Manifest（推荐）

```
~/.ai-pad/pets/
└── default/
    ├── manifest.json          # 动画定义
    └── sprites.png            # 精灵条（所有帧水平排列）
```

**manifest.json 格式：**

```json
{
    "id": "default",
    "displayName": "8-Bit Cat",
    "description": "Original pixel cat companion",
    "sprite": {
        "image": "sprites.png",
        "frameWidth": 16,
        "frameHeight": 16,
        "columns": 20
    },
    "palette": {
        "0": null,
        "1": [30, 30, 40, 255],
        "2": [255, 180, 140, 255],
        "3": [255, 220, 190, 255],
        "4": [40, 35, 50, 255],
        "5": [255, 120, 140, 255]
    },
    "animations": {
        "idle": {
            "frames": [0, 1, 2, 1, 0],
            "durations": [1500, 120, 200, 120, 1800],
            "loop": true
        },
        "walk": {
            "frames": [3, 4, 3, 5],
            "durations": [150, 150, 150, 150],
            "loop": true
        },
        "sleep": {
            "frames": [6, 7],
            "durations": [800, 800],
            "loop": true
        },
        "talk": {
            "frames": [8, 9, 10],
            "durations": [300, 300, 400],
            "loop": false,
            "fallback": "idle",
            "repeatBeforeFallback": 3
        },
        "happy": {
            "frames": [11, 12, 11],
            "durations": [250, 120, 230],
            "repeatBeforeFallback": 3,
            "fallback": "idle"
        },
        "confused": {
            "frames": [13, 14],
            "durations": [400, 400],
            "loop": false,
            "fallback": "idle",
            "repeatBeforeFallback": 2
        },
        "jump": { "frames": [15], "durations": [1] },
        "spin":  { "frames": [16], "durations": [1] },
        "wave":  { "frames": [17], "durations": [1] },
        "shake": { "frames": [18], "durations": [1] }
    }
}
```

**sprites.png 布局：** 20 帧水平排列，每帧 16x16 px，总尺寸 320x16 px。

#### 加载器实现

```javascript
// 📄 (新文件) sprite-loader.js  — 精灵表加载器（需新建）

async function loadPetManifest(petId) {
    const petDir = `${PET_DATA_DIR}/pets/${petId}`;
    const resp = await fetch(`${petDir}/manifest.json`);
    const manifest = await resp.json();

    // 加载精灵图
    const img = new Image();
    img.src = `${petDir}/${manifest.sprite.image}`;
    await new Promise(resolve => { img.onload = resolve; });

    // 切帧：从精灵图中提取每帧的像素数据
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    const { frameWidth, frameHeight, columns } = manifest.sprite;
    canvas.width = frameWidth;
    canvas.height = frameHeight;

    const framePixels = {};  // { frameIndex: int[256] }

    for (let i = 0; i < columns; i++) {
        ctx.clearRect(0, 0, frameWidth, frameHeight);
        ctx.drawImage(
            img,
            i * frameWidth, 0, frameWidth, frameHeight,  // 源区域
            0, 0, frameWidth, frameHeight                 // 目标区域
        );
        const imageData = ctx.getImageData(0, 0, frameWidth, frameHeight);
        // 将 RGBA 量化为 palette index
        framePixels[i] = quantizeToPalette(imageData.data, manifest.palette);
    }

    return { manifest, framePixels };
}

function quantizeToPalette(rgbaData, palette) {
    const result = new Array(rgbaData.length / 4);
    for (let i = 0; i < rgbaData.length; i += 4) {
        const [r, g, b, a] = [rgbaData[i], rgbaData[i+1], rgbaData[i+2], rgbaData[i+3]];
        result[i / 4] = findNearestPaletteIndex(r, g, b, a, palette);
    }
    return result;
}
```

#### 向后兼容

保留现有 `SPRITES` 和 `cloneSprite` 作为**内置默认宠物**（当没有外部 manifest 时使用）。加载顺序：

```
1. 尝试加载 ~/.ai-pad/pets/<id>/manifest.json
2. 失败 → 使用内置默认宠物（现有 sprite.js 数据）
```

### 5.4 换肤示例

用户想换成一只狗？只需：

```
~/.ai-pad/pets/
├── default/     # 猫（内置）
└── dog/
    ├── manifest.json    # 同结构，不同帧图
    └── sprites.png      # 16x16 狗的精灵条
```

配置文件指定：`pet: "custom:dog"`

---

## 6. 优化方向五：Idle 环境变化（Ambient Variants）

### 6.1 问题诊断

Idle 只有单一 4 帧眨眼循环，永远不变。长时间观看非常单调。

### 6.2 设计思路

将 Idle 从单一循环扩展为**一组随机触发的 ambient 动画**：

```json
{
    "idle": {
        "baseLoop": {
            "frames": [0, 1, 2, 1, 0],
            "durations": [1500, 120, 200, 120, 1800]
        },
        "variants": [
            {
                "name": "ear_twitch",
                "frames": [19, 0],
                "durations": [200, 100],
                "weight": 3,
                "cooldownMinMs": 5000,
                "cooldownMaxMs": 15000
            },
            {
                "name": "look_around",
                "frames": [20, 21, 20],
                "durations": [400, 300, 400],
                "weight": 2,
                "cooldownMinMs": 8000,
                "cooldownMaxMs": 25000
            },
            {
                "name": "tail_wag",
                "frames": [22],
                "durations": [1500],
                "weight": 1,
                "cooldownMinMs": 12000,
                "cooldownMaxMs": 30000
            },
            {
                "name": "yawn",
                "frames": [23, 24, 24, 23],
                "durations": [300, 800, 600, 300],
                "weight": 1,
                "cooldownMinMs": 30000,
                "cooldownMaxMs": 60000
            }
        ]
    }
}
```

**运行时行为：**

```
idle base loop (blink) ──→ 随机触发 ear_twitch ──→ 回到 blink ──→ look_around ──→ blink ...
                       ↑ 每 5-15 秒                  ↑ 每 8-25 秒
```

**JS 实现：**

```javascript
class AmbientIdleController {
    constructor(baseLoop, variants) {
        this.baseLoop = baseLoop;
        this.variants = variants;
        this.currentVariant = null;
        this.variantEndTime = 0;
        this.nextVariantTime = this.scheduleNext();
    }

    scheduleNext() {
        // 从 variants 中按 weight 随机选一个，加上随机 cooldown
        const totalWeight = this.variants.reduce((s, v) => s + v.weight, 0);
        let rand = Math.random() * totalWeight;
        let selected = this.variants[0];
        for (const v of this.variants) {
            rand -= v.weight;
            if (rand <= 0) { selected = v; break; }
        }
        const cooldown = selected.cooldownMinMs +
            Math.random() * (selected.cooldownMaxMs - selected.cooldownMinMs);
        return { variant: selected, delay: cooldown };
    }

    getCurrentFrames(now) {
        if (this.currentVariant && now < this.variantEndTime) {
            // 正在播放 variant
            return this.currentVariant.frames;
        }
        if (this.nextVariantTime && now >= this.nextVariantTime.time) {
            // 触发新 variant
            this.currentVariant = this.nextVariantTime.variant;
            this.variantEndTime = now + this.currentVariant.frames
                .reduce((s, f) => s + f.durations, 0);
            this.nextVariantTime = this.scheduleNext();
            return this.currentVariant.frames;
        }
        // 默认 base loop
        return this.baseLoop.frames;
    }
}
```

### 6.3 效果

- 猫不再像个机械装置，而是会偶尔动耳朵、四处看看、打哈欠
- `weight` + `cooldown` 控制频率——不打扰用户工作
- 完全通过数据配置，无需改代码

---

## 7. 优化方向六：Dance 纳入统一状态机

### 7.1 问题诊断

Dance 系统**完全绕过** `PetStateMachine`：

```javascript
// 📄 app.js:414-416  — 主循环中 dance/normal 双分支（架构分裂点）
if (dancePlayer) {
    updateDance(dt);       // ← 完全独立的路径
} else {
    pet.update(dt);         // ← 正常状态机
    SpriteRenderer.renderSprite(ctx, pet.state, pet.frame, ...);
}
```

Rust 端 `PetState` enum 没有 `Dancing` 变体：

```rust
// 📄 core/src/pet.rs:10-24  — 现有 PetState（缺少 Dancing 变体）
pub enum PetState {
    Idle, Walk, Sleep, Talk, Happy, Confused,
    // Dancing(String),  ← ★ 改造目标：加入此行
}
```

后果：
- 前后端维护两套状态逻辑（Rust 测试覆盖不到 dance）
- Dance 无法和其他状态组合（比如 dance 中收到 AI 回复怎么办？）
- `is_dancing` 是全局原子变量，不是状态机的自然部分

### 7.2 改造方案

#### Step 1: PetState 加入 Dancing

```rust
// core/src/pet.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PetState {
    #[default]
    Idle,
    Walk,
    Sleep,
    Talk,
    Happy,
    Confused,
    Dancing(String),  // ← 新增：携带舞蹈名称
}
```

#### Step 2: Dance 作为一种特殊动画轨道

Dance 的帧数据同样来自 manifest（而不是硬编码的单帧 + 渲染层 offset）：

```json
{
    "dances": {
        "happy_twist": {
            "loop": true,
            "steps": [
                { "action": "jump",  "duration_ms": 300, "repeat": 1 },
                { "action": "spin",  "duration_ms": 400, "repeat": 1 },
                { "action": "wave",  "duration_ms": 500, "repeat": 1 },
                { "action": "shake", "duration_ms": 300, "repeat": 2 },
                { "action": "idle",  "duration_ms": 200, "repeat": 1 }
            ]
        }
    }
}
```

#### Step 3: 统一渲染路径

```javascript
// app.js — 主循环简化为单一路径
function loop(now) {
    const dt = now - lastTime;
    lastTime = now;

    if (!collapsed) {
        pet.update(dt);  // 所有状态（包括 Dancing）都走这里

        if (pet.state !== prevState) {
            syncStateClass(pet.state);
            flashSprite();
            Particles.onStateEnter(pet.state);
            prevState = pet.state;
        }

        // Dancing 状态额外传入舞蹈渲染参数
        const renderOpts = pet.state === 'dancing'
            ? pet.getDanceRenderOpts()
            : {};

        SpriteRenderer.renderSprite(
            ctx, pet.state, pet.frame, pet.facingRight, 8, renderOpts
        );
        Particles.tick(pet.state, dt);
    } else {
        SpriteRenderer.renderMini(ctx, pet.state);
    }

    requestAnimationFrame(loop);
}
```

`PetStateMachine.update()` 内部处理 Dancing 的步进逻辑（复用现有的 `advanceFrame()` 时间轴机制，只是帧数据来自 dance steps 而非固定状态帧）。

---

## 8. 实施优先级与风险评估

### 优先级矩阵

| # | 优化方向 | 改动量 | 效果 | 风险 | 优先级 |
|---|---------|--------|------|------|--------|
| 1 | **非均匀帧时长** | 小（改 `pet.js` update + `STATE_CONFIG`） | Idle 立刻更有生命力 | 低（向后兼容，可渐进迁移） | **P0 — 立即可做** |
| 2 | **三遍+回落** | 小（新增 `buildStateAnimation()` 工具函数） | 状态过渡更自然 | 低（纯 additive） | **P0 — 配合 #1 一起做** |
| 3 | **通知驱动+生命周期** | 中（重构状态切换 + bridge + gamepad） | 语义化、可调试 | 中（涉及前后端联动） | **P1** |
| 4 | **精灵表外化** | 中（新增加载器 + 工具链 + 文档） | 可换肤、美术友好 | 中（需处理加载时序、向后兼容） | **P1** |
| 5 | **Idle Variants** | 中（扩展 manifest 格式 + 新增控制器） | 长时间观看不枯燥 | 低（纯 additive，不影响现有 idle） | **P2** |
| 6 | **Dance 纳入状态机** | 中（重构 dancePlayer + PetState enum） | 架构统一、可测试 | 中（dancePlayer 重写、window move 逻辑解耦） | **P2** |

### 推荐实施路径

```
Phase 1 (1-2 天):  #1 + #2 — 改造 pet.js 的 STATE_CONFIG 和 update()
                     │  立刻见效：idle 呼吸更自然、状态过渡不突兀
                     │  纯前端改动，不碰 Rust
                     ▼
Phase 2 (2-3 天):  #3 — 通知系统
                     │  需要 bridge.rs + gamepad.rs + pet.js 协同改动
                     │  解决 confused 过早消失等问题
                     ▼
Phase 3 (2-3 天):  #4 — 精灵表外化
                     │  新建 sprite-loader.js
                     │  编写 Aseprite 导出脚本/工具
                     │  保留内置默认宠物作为 fallback
                     ▼
Phase 4 (2-3 天):  #5 + #6 — Idle variants + Dance 统一
                     在 #4 的 manifest 基础上自然扩展
```

### 关键注意事项

1. **前后端同步**: 8Bit 的 `core/src/pet.rs` 和 `app/frontend/js/pet.js` 是手动同步的两套状态机。任何动画逻辑改动必须**同时修改两处**，或者考虑长期方案：让 JS 直接从 Rust 获取状态配置（通过 Tauri invoke）。

2. **DPI 缩放**: `app.js` 中已有 DPI 处理逻辑（`scale = cssW / logicW`），精灵表外化后需要确保加载的像素数据在不同 DPI 下正确缩放。

3. **舞蹈窗口移动**: 当前 `applyDanceWindowMove()` 直接操作 WebView2 窗口位置，这是 Dance 特有的行为。如果 Dance 纳入状态机，需要把窗口移动逻辑作为 `Dancing` 状态的特殊渲染钩子，而不是混在通用 update() 里。

4. **折叠态 (mini)**: `renderMini()` 只取 state 的第 0 帧画头部。新的帧索引体系（sprite_index 指向精灵条的全局位置）需要确保第 0 帧仍然是"正面站立"的基础帧。

5. **CSS breath 动画**: 当前 `pet.css` 有一个 CSS `@keyframes breath` 做 scale(1.0↔1.02) 的微动画配合 idle。如果 idle 改为非均匀帧时长，应评估 CSS 动画是否仍然需要（建议保留作为额外的"活着"感层次）。

---

## 附录：Codex 关键源码索引

| 文件 | 行号 | 功能 |
|------|------|------|
| `codex-rs/tui/src/pets/model.rs` | 32-52 | `AnimationFrame` / `Animation` 结构体定义 |
| `codex-rs/tui/src/pets/model.rs` | 61-72 | `Pet` 结构体定义 |
| `codex-rs/tui/src/pets/model.rs` | 148-183 | `load_builtin_pet()` — 从缓存目录加载 |
| `codex-rs/tui/src/pets/model.rs` | 388-452 | `load_animations()` — 从 JSON spec 构建 Animation |
| `codex-rs/tui/src/pets/model.rs` | 484-582 | `default_animations()` — 14 个内置动画轨道 |
| `codex-rs/tui/src/pets/model.rs` | 584-596 | `idle_animation()` — 非均匀呼吸帧 |
| `codex-rs/tui/src/pets/model.rs` | 598-627 | `app_state_animation()` — 三遍+回落工厂函数 |
| `codex-rs/tui/src/pets/ambient.rs` | 46-90 | `PetNotificationKind` + 生命周期 |
| `codex-rs/tui/src/pets/ambient.rs` | 92-111 | `PetNotification` + 过期检测 |
| `codex-rs/tui/src/pets/ambient.rs` | 126-346 | `AmbientPet` — 运行时动画状态机 |
| `codex-rs/tui/src/pets/ambient.rs` | 283-301 | `current_animation()` — 通知→动画查找 |
| `codex-rs/tui/src/pets/ambient.rs` | 376-412 | `current_animation_frame()` — 核心时间轴查表 |
| `codex-rs/tui/src/pets/catalog.rs` | 18-67 | 8 个内置宠物目录 |
| `codex-rs/tui/src/pets/frames.rs` | 全文 | 精灵表切片为独立 PNG |
| `codex-rs/tui/src/pets/asset_pack.rs` | 全文 | CDN 资产下载与缓存 |

## 附录：8Bit 关键源码索引

| 文件 | 行号 | 功能 |
|------|------|------|
| `core/src/pet.rs` | 10-24 | `PetState` enum（6 个状态） |
| `core/src/pet.rs` | 26-61 | `frame_count()` / `frame_duration_ms()` / `auto_idle_timeout_ms()` |
| `core/src/pet.rs` | 66-81 | `Pet` 结构体 |
| `core/src/pet.rs` | 109-139 | `update()` — 帧计数器累加逻辑 |
| `core/src/pet.rs` | 142-153 | `set_state()` — 状态切换与重置 |
| `app/frontend/js/sprite.js` | 7-14 | `PALETTE` — 6 色调色板 |
| `app/frontend/js/sprite.js` | 17-34 | `IDLE_BASE` — 256 像素基底帧 |
| `app/frontend/js/sprite.js` | 37-43 | `cloneSprite()` — 帧派生工具 |
| `app/frontend/js/sprite.js` | 46-195 | 所有帧定义 + `SPRITES` 映射表 |
| `app/frontend/js/sprite.js` | 206-234 | `renderSprite()` — Canvas 渲染 |
| `app/frontend/js/pet.js` | 4-10 | `STATE_CONFIG` — 状态参数（均匀帧长） |
| `app/frontend/js/pet.js` | 12-83 | `PetStateMachine` 类 |
| `app/frontend/js/pet.js` | 42-69 | `update()` — JS 侧帧计数器 |
| `app/frontend/js/app.js` | 409-436 | `loop()` — 主渲染循环 |
| `app/frontend/js/app.js` | 414-430 | dance/normal 分支判断 |
| `app/frontend/js/app.js` | 473-517 | `updateDance()` — 独立舞蹈播放器 |
| `core/src/dance.rs` | 17-30 | `DanceAction` enum（5 个动作） |
| `core/src/dance.rs` | 33-43 | `DanceStep` / `DanceDef` |
| `core/src/dance.rs` | 77-124 | `validate_dance_def()` — 校验逻辑 |
