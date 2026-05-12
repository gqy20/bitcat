# 3D 化架构方案 — 从像素桌宠到体素 3D 游戏

> 创建日期：2026-05-13
> 状态：规划中（未开始实施）

## 1. 背景与目标

### 当前状态

项目是一个 Tauri 2.0 桌面宠物应用，渲染管线为 **纯 2D 像素艺术**：

| 层 | 技术 | 数据格式 |
|---|------|---------|
| 后端状态机 | `core/src/pet.rs` | `Pet { state, x, y, facing_right, frame }` |
| 前端渲染 | Canvas 2D `fillRect` 逐像素绘制 | 16×16 调色板索引数组，8x 放大到 128×128 |
| 窗口 | WebView2 透明窗口 | 单 canvas，`requestAnimationFrame` 驱动 |

精灵数据定义在 `app/frontend/js/sprite.js`：每个状态对应一个帧数组（每帧 256 字节 = 16×16 像素），通过调色板映射到 RGBA 颜色后逐像素 `fillRect`。

### 目标

1. **短期**：将桌宠从 2D 像素升级为 **3D 体素风格**（类似 Minecraft 角色）
2. **长期**：具备 AI 生成完整 3D 游戏场景的能力

### 约束

- 必须兼容现有 Tauri 透明窗口、拖拽吸附、多窗口模型
- 必须兼容现有 AI Agent 管线（AI 输出 → IPC → 前端播放）
- 桌宠窗口尺寸小（128×128），3D 方案必须在此尺寸下有良好表现
- 桌面应用场景，包体积敏感度低（本地加载无网络开销）

---

## 2. 技术选型：Three.js 统一渲染层

### 选型对比

| 方案 | 桌宠适配 | 游戏扩展性 | 透明窗口支持 | AI 生成友好度 |
|------|---------|-----------|-------------|-------------|
| **Three.js（推荐）** | ✅ `alpha: true` 即可 | ✅ 完整 3D 引擎 | ✅ WebView2 (Chromium) 原生支持 | ✅ voxel → InstancedMesh 一一映射 |
| 原生 WebGL | ✅ 但手写量大 | ✅ 完全控制 | ✅ | ❌ 代码太底层，维护成本高 |
| Babylon.js | ✅ | ✅ | ✅ | ⚠️ 生态/社区比 Three.js 小 |
| Bevy (Rust) | ❌ 需独立 OS 窗口 | ✅ | ⚠️ 与 Tauri 集成复杂 | ❌ 编译时资产，不利于 AI 动态生成 |
| CSS 3D Transform | ⚠️ "纸片人"效果 | ❌ 无法扩展为真 3D | ✅ | ⚠️ 表现力有限 |

### 选择 Three.js 的理由

1. **统一技术栈**：桌宠和未来游戏共用同一渲染引擎，避免后续迁移成本
2. **WebView2 透明 + WebGL 成熟可行**：Chromium 的 `webgl` context 配合 `alpha: true` + Windows DWM 合成，是经过验证的路径
3. **InstancedMesh 高效体素渲染**：5000 以内的 voxel 数量可在集成显卡上稳定 60fps
4. **生态丰富**：后期需要物理引擎（cannon-es / ammo.js）、后处理、GLTF 加载器时都有成熟方案
5. **AI 友好**：voxel 数据（三维颜色数组）与 AI 输出的 JSON 结构天然对齐
6. **Tree-shaking 后体积可控**：只引入需要的模块，~40-80KB gzip

### 架构总览

```
┌──────────────────────────────────────────────────┐
│                Tauri 2.0 应用                      │
│                                                  │
│  ┌────────────┐   ┌────────────────────────────┐ │
│  │  pet 窗口   │   │    game 窗口（Phase 3）     │ │
│  │  128×128   │   │    自由尺寸                  │ │
│  │            │   │                            │ │
│  │  Three.js  │   │    Three.js                │ │
│  │  ┌──────┐  │   │  ┌──────────────────────┐  │ │
│  │  │VoxelCat│  │   │  │ Scene / Camera / Lights│  │ │
│  │  │ 场景   │  │   │  │ Terrain + Characters  │  │ │
│  │  └──────┘  │   │  │ AI 生成的 3D 内容       │  │ │
│  │            │   │  └──────────────────────┘  │ │
│  │ Isometric  │   │    PerspectiveCamera        │ │
│  │ Orthographic│   │    OrbitControls           │ │
│  └────────────┘   └────────────────────────────┘ │
│                                                  │
│  共享层：                                         │
│  ├── voxel 数据格式（三维数组）                     │
│  ├── 动画系统（关键帧 / 骨骼）                     │
│  ├── AI 生成管线（IPC 协议）                       │
│  └── 调色板 / 材质系统                             │
└──────────────────────────────────────────────────┘
```

---

## 3. 核心技术设计

### 3.1 WebView2 透明 + WebGL

WebView2 基于 Chromium，WebGL 透明通道完全受支持：

```javascript
const renderer = new THREE.WebGLRenderer({
  alpha: true,                 // 透明通道 → DWM 合成桌面背景
  antialias: false,            // 128px 不需要抗锯齿，省 GPU
  powerPreference: 'high-performance',
});
renderer.setPixelRatio(1);     // 小窗口不需要 HiDPI 缩放
renderer.setSize(128, 128);
// 不设 setClearColor 或设为 (0, 0, 0, 0) = 全透明
```

**Fallback 策略**：如果检测到 WebGL 不可用（极少数情况），降级回 Canvas 2D 等距投影渲染。

### 3.2 体素数据格式

#### VoxelMap — 三维颜色数组

```typescript
// 核心数据结构：voxel 地图
interface VoxelMap {
  // 三维尺寸
  size: [number, number, number];  // [width, height, depth] 如 [12, 14, 10]
  // 体素数据：扁平化的一维数组，索引 = z * size_y * size_x + y * size_x + x
  // 值 = 调色板索引（0 = 空/透明）
  voxels: number[];
}

// 示例：一个 12×14×10 的猫（1680 个位置，实际约 600-800 个体素）
const catVoxelMap: VoxelMap = {
  size: [12, 14, 10],
  voxels: [
    // z=0 层（正面）
    0,0,1,1,1,1,1,1,1,1,0,0,
    0,1,2,2,2,2,2,2,2,2,1,0,
    ...
  ],
};
```

#### 调色板（复用现有设计）

```typescript
// 从 sprite.js 迁移，增加法线感知的明暗变体
const PALETTE = {
  0: null,                          // 透明（空）
  1: [30, 30, 40, 255],             // 轮廓
  2: [255, 180, 140, 255],          // 肤色（主色）
  3: [255, 220, 190, 255],          // 高光
  4: [40, 35, 50, 255],             // 眼睛
  5: [255, 120, 140, 255],          // 嘴巴/腮红
};

// Three.js 中每个调色板索引 → Color 对象
function paletteColor(index: number): THREE.Color | null {
  const c = PALETTE[index];
  return c ? new THREE.Color(c[0]/255, c[1]/255, c[2]/255) : null;
}
```

### 3.3 InstancedMesh 渲染

所有体素共享同一个 `BoxGeometry`，通过 instance 变换矩阵定位：

```typescript
class VoxelRenderer {
  private mesh: THREE.InstancedMesh;
  private dummy: THREE.Object3D;  // 复用的变换对象

  constructor(scene: THREE.Scene, voxelMap: VoxelMap) {
    const geometry = new THREE.BoxGeometry(1, 1, 1);
    const material = new THREE.MeshLambertMaterial({ vertexColors: true });

    // 收集非空 voxel
    const instances: Array<{ pos: [number,number,number], colorIndex: number }> = [];
    const [sx, sy, sz] = voxelMap.size;
    for (let z = 0; z < sz; z++) {
      for (let y = 0; y < sy; y++) {
        for (let x = 0; x < sx; x++) {
          const idx = z * sy * sx + y * sx + x;
          if (voxelMap.voxels[idx] !== 0) {
            instances.push({ pos: [x, y, z], colorIndex: voxelMap.voxels[idx] });
          }
        }
      }
    }

    this.mesh = new THREE.InstancedMesh(geometry, material, instances.length);
    this.dummy = new THREE.Object3D();

    // 设置每个 instance 的变换和颜色
    const colorAttr = new THREE.InstancedBufferAttribute(
      new Float32Array(instances.length * 3), 3
    );

    for (let i = 0; i < instances.length; i++) {
      this.dummy.position.set(...instances[i].pos);
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(i, this.dummy.matrix);

      const c = paletteColor(instances[i].colorIndex);
      colorAttr.setXYZ(i, c?.r ?? 0, c?.g ?? 0, c?.b ?? 0);
    }
    this.mesh.instanceColor = colorAttr;

    scene.add(this.mesh);
  }
}
```

**性能预估**：

| 体素数量 | Draw Calls | GPU 内存 | 预期 FPS（集成显卡） |
|---------|-----------|---------|-------------------|
| ~500（简单猫） | 1 | ~50KB | 120+ |
| ~2000（精细猫） | 1 | ~200KB | 60+ |
| ~5000（游戏角色） | 1 | ~500KB | 30-60 |
| ~20000（游戏场景） | 1 | ~2MB | 需要 Frustum Culling |

### 3.4 相机配置

#### Phase 1-2：桌宠 — 正交等距相机

```typescript
// 固定等距视角（isometric），模拟 2.5D 效果
const aspect = 128 / 128;
const frustumSize = 20;  // 视锥体大小（适应 ~12×14×10 的 voxel 猫）
const camera = THREE.OrthographicCamera(
  -frustumSize * aspect / 2, frustumSize * aspect / 2,
  frustumSize / 2, -frustumSize / 2,
  0.1, 100
);
// 经典等距角度：绕 Y 轴旋转 45°，再俯视 ~35.264°（atan(1/√2)）
camera.position.set(20, 20, 20);
camera.lookAt(0, 7, 0);  // 看向猫的中心偏上
```

#### Phase 3：游戏 — 透视相机 + 自由控制

```typescript
const camera = THREE.PerspectiveCamera(60, width / height, 0.1, 1000);
// OrbitControls 允许用户旋转/缩放视角
const controls = new THREE.OrbitControls(camera, renderer.domElement);
```

### 3.5 光照系统

```typescript
// 简单但有效的三点光照
const ambientLight = new THREE.AmbientLight(0xffffff, 0.5);  // 环境光填充阴影

const keyLight = new THREE.DirectionalLight(0xffffff, 0.8);
keyLight.position.set(5, 15, 10);  // 主光源（模拟从右上方照射）

const fillLight = new THREE.DirectionalLight(0xffeedd, 0.3);
fillLight.position.set(-5, 5, -5);  // 补光减少过暗区域
```

光照自动作用于 `MeshLambertMaterial`，不同朝向的面会产生明暗差异，增强立体感。无需手动计算面颜色。

### 3.6 动画系统

#### 方案 A：关键帧位移（推荐起步）

继承现有舞蹈系统的模式——AI 或预设数据输出时间轴上的 voxel 位移：

```typescript
interface VoxelAnimation {
  name: string;
  loop: boolean;
  steps: AnimationStep[];
}

interface AnimationStep {
  action: string;          // 'idle' | 'walk' | 'jump' | 'blink' | ...
  duration_ms: number;
  voxel_mods?: Array<{
    position: [number, number, number];  // 哪个体素
    offset: [number, number, number];    // 位移量
    colorIndex?: number;                 // 可选变色
  }>;
  camera_offset?: [number, number, number];
}
```

#### 方案 B：Morph Targets（进阶）

对于平滑变形（呼吸、眨眼），预计算每帧的完整 voxel 地图，在帧间插值：

```typescript
// 每个状态的多帧 voxel 地图（类比现有 SPRITES 结构）
const VOXEL_SPRITES: Record<string, VoxelMap[]> = {
  idle: [idleFrame0, idleFrame1, idleFrame2, idleFrame3],
  walk: [walkA, walkB, walkC, walkD],
  sleep: [sleepBase, sleepBreathe],
  // ...
};
```

#### 方案 C：骨骼动画（远期）

当 voxel 数量大且需要复杂动作时，引入骨骼系统绑定 voxel 组：

```
头骨 ← 绑定头部 voxel
躯干骨 ← 绑定身体 voxel
左腿骨 ← 绑定左脚 voxel
...
```

推荐先用方案 A+B，Phase 3 再考虑方案 C。

### 3.7 角色部位组合与变身系统

#### 3.7.1 当前精灵的形状缺陷

当前 `sprite.js` 的 IDLE_BASE（16×16 像素）可视化后，**本质上是一个"长了脸的圆球"**：

```
Row 0:   ···████████····    ← 头顶轮廓
Row 1:   ··██░░░███░░░░██·   ← 额头
Row 2-4: ·██░░░░░░░░░░░░██· ← 脸（含眼睛高光）
Row 5-7: ·██░░░░░░░░░░░░██· ← 脸中部
Row 8:   ·██░░░♥░░░░░♥░░░██· ← 腮红/嘴
Row 9-11:·██░░░░░░░░░░░░██· ← 下巴收窄
Row 12:  ···██░░░░░░░██···   ← 脖子（仅4像素宽！）
Row 13:  ·····██████····     ← 身体（仅6像素）
Row 14:  ······████····      ← 底部尖
Row 15:  ·················     ← 空
```

**核心问题**：
- **头身比约 5:1**，躯干只有 rows 13-14 共 2 行有效像素
- **没有独立的手臂和腿部**——walk 动画只能抖动底部几个像素模拟抬脚
- 所有状态帧是**单体扁平数组**（256 个数字），无法让某个部位独立运动

#### 3.7.2 设计目标：组合式部位 + 变身动画

核心创意：**进入游戏模式时，Q 版大头猫"长出"完整身体**。这不是变形（morphing），而是"生长"（reveal）——完整角色数据始终存在于内存中，普通模式下隐藏 body/arms/legs，游戏模式触发时逐个渐显弹出。

变身时间轴（总时长 ~1.5s）：

```
  0ms          400ms         800ms        1200ms       1500ms
   │            │             │            │            │
   ▼            ▼             ▼            ▼            ▼

  (O_O)      ⬡(O_O)⬡     ╱(O_O)╲     ╱(O_O)╲     ╱(O_O)╲
   ●         ┃●      ┃   ┃ ●    ┃   ┃│●│  │┃   ┃│●│ /│┃
             ┃       ┃   ┃/  \┃   ┃/ /\ \┃   ┃/ /  \┃┃
                       ╲    ╱    ╲  ╱  ╱    ╲  ╲    ╱
                        ╰──╯     ╰──╯  ╰────╯  ╰────╯

  Q版大头     肩膀隆起     手臂展开     躯干拉长     腿部弹出+落地
 （当前）    （躯干冒出）  （手长出）   （躯干成型） （完整角色）
```

#### 3.7.3 数据结构：部位定义（Canvas 2D 与 Three.js 同构）

**关键设计原则**：部位数据是纯描述性的，不绑定任何渲染 API。同一套数据同时适用于 Canvas 2D 组合渲染和 Three.js Scene Graph。

```typescript
// ====== 核心数据结构（渲染无关）======

/** 单个部位的像素/体素数据 */
interface BodyPart {
  /** 部位名称 */
  name: 'head' | 'torso' | 'left_arm' | 'right_arm' | 'left_leg' | 'right_leg';
  /** 像素尺寸 [宽, 高] —— Canvas 2D 用；Three.js 中对应 voxelMap.size 的 XY 截面 */
  size: [number, number];
  /**
   * 枢轴点（局部坐标）。
   * - Canvas 2D：translate + scale 的原点
   * - Three.js：Group.position 的挂载点（子部件连接到父部件的位置）
   */
  pivot: [number, number];
  /**
   * 相对于角色根原点的默认偏移。
   * - Canvas 2D：drawImage 的目标坐标
   * - Three.js：Group.position（相对于父 Group）
   */
  baseOffset: [number, number];
  /**
   * 父部位名称（构建层级树）。
   * root 的 parent 为 null。
   * 层级关系：root → torso → {head, left_arm, right_arm, left_leg, right_leg}
   */
  parent: string | null;
  /**
   * 各状态下的帧数据。
   * - Canvas 2D：调色板索引的一维数组（类比现有 SPRITES）
   * - Three.js：VoxelMap[]（每个元素是一个三维 voxel 地图）
   */
  frames: Record<string, PartFrameData[]>;
  /** 该部位在变身动画中的生长时间点（ms） */
  growAt: number;
}

/** 联合类型：Canvas 2D 帧数据 OR Three.js VoxelMap */
type PartFrameData = number[] | VoxelMap;

// ====== 完整角色定义示例 ======

const CHARACTER: Record<string, BodyPart> = {
  head: {
    name: 'head',
    size: [10, 10],
    pivot: [5, 9],          // 脖子连接点（局部底部中心）
    baseOffset: [3, 0],      // 挂载在 torso 上方
    parent: 'torso',         // torso 的子节点
    frames: {
      idle:  [headIdle0, headIdle1, headIdle2, headIdle3],   // 眨眼动画
      sleep: [headSleepClosed, headSleepBreathe],
      talk:  [headTalkSmall, headTalkLarge, headTalkClosed],
      happy: [headHappySmile, headHappyBlink, headHappySmile],
    },
    growAt: 0,               // 头始终可见（不变身时就是大头猫形态）
  },
  torso: {
    name: 'torso',
    size: [8, 10],
    pivot: [4, 9],           // 胯部连接点（局部底部）
    baseOffset: [4, 9],      // 角色的根节点偏移
    parent: null,            // torso 是根节点
    frames: {
      idle:  [torsoIdle],
      walk:  [torsoIdle],    // 躯干走路时轻微上下起伏（由动画层处理）
    },
    growAt: 400,             // 第 400ms 开始从脖子向下生长
  },
  left_arm: {
    name: 'left_arm',
    size: [3, 7],
    pivot: [1, 0],           // 肩膀连接点（局部顶部）
    baseOffset: [-4, 10],    // 挂在 torso 左肩
    parent: 'torso',
    frames: {
      idle:  [armDown],
      wave:  [armUp, armMid, armDown],  // 挥手动作
      walk:  [armBack, armFwd],          // 走路摆动
    },
    growAt: 600,             // 第 600ms 从肩膀向外伸展
  },
  right_arm: {
    name: 'right_arm',
    size: [3, 7],
    pivot: [1, 0],
    baseOffset: [5, 10],     // 挂在 torso 右肩
    parent: 'torso',
    frames: {
      idle:  [armDown],
      wave:  [armUp, armMid, armDown],
      walk:  [armFwd, armBack],          // 与左臂相位差 180°
    },
    growAt: 650,             // 比左臂稍晚 50ms，增加节奏感
  },
  left_leg: {
    name: 'left_leg',
    size: [3, 8],
    pivot: [1, 0],           // 胯部连接点（局部顶部）
    baseOffset: [-2, 19],    // 挂在 torso 左胯
    parent: 'torso',
    frames: {
      idle:  [legStand],
      walk: [legLiftA, legLiftB, legStand],  // 抬脚→落地
    },
    growAt: 1000,            // 第 1000ms 从胯部向下弹出
  },
  right_leg: {
    name: 'right_leg',
    size: [3, 8],
    pivot: [1, 0],
    baseOffset: [3, 19],     // 挂在 torso 右胯
    parent: 'torso',
    frames: {
      idle:  [legStand],
      walk: [legStand, legLiftA, legLiftB],  // 与左腿相位差 180°
    },
    growAt: 1050,            // 比左腿稍晚 50ms
  },
};

// 层级树（用于递归渲染/变换传播）
const BODY_HIERARCHY = {
  root: 'torso',
  children: {
    torso:  ['head', 'left_arm', 'right_arm', 'left_leg', 'right_leg'],
    head:   [],
    left_arm: [],
    right_arm: [],
    left_leg: [],
    right_leg: [],
  },
};
```

#### 3.7.4 渲染实现：两种后端共用同一数据

##### Canvas 2D 后端（Phase 1 过渡 / Fallback）

```javascript
function renderCharacter2D(ctx, character, transformState) {
  // 从根节点递归渲染层级树
  renderPart(ctx, 'torso', character, transformState);
}

function renderPart(ctx, partName, character, state) {
  const part = character[partName];
  const progress = getGrowProgress(state.elapsed, part.growAt);

  if (progress <= 0) return;  // 还没开始生长 → 不绘制

  ctx.save();

  // 1. 定位到该部位的挂载点（父部件已设置好的坐标系内）
  ctx.translate(part.baseOffset[0], part.baseOffset[1]);

  // 2. 生长动画：从枢轴点向外缩放
  const growScale = easeOutBack(clamp01(progress));
  ctx.scale(growScale, growScale);

  // 3. 部位自身偏移（让 scale 原点落在枢轴/挂接处）
  ctx.translate(-part.pivot[0], -part.pivot[1]);

  // 4. 绘制部位像素数组（复用现有 fillRect 逻辑）
  const frameData = getCurrentFrame(part, state);  // 根据状态+frameIndex 取帧
  drawPixelArray(ctx, frameData, part.size[0], part.size[1]);

  // 5. 递归渲染子部位（它们继承当前 ctx 变换 = 层级联动）
  for (const childName of BODY_HIERARCHY.children[partName]) {
    renderPart(ctx, childName, character, state);
  }

  ctx.restore();
}
```

##### Three.js 后端（Phase 1 主线）

部位定义直接映射为 Three.js Scene Graph 的 `THREE.Group` 层级：

```typescript
class VoxelCharacter {
  private root: THREE.Group;              // = torso Group
  private parts: Map<string, PartMesh>;   // name → InstancedMesh + Group 包装

  constructor(scene: THREE.Scene, characterDef: typeof CHARACTER) {
    this.root = new THREE.Group();
    scene.add(this.root);

    // 递归构建 Scene Graph（与 BODY_HIERARCHY 一一对应）
    this.buildPart('torso', characterDef, this.root);
  }

  private buildPart(name: string, def: typeof CHARACTER, parent: THREE.Group) {
    const partDef = def[name];

    // 每个部位 = 一个 Group（负责变换）+ 一个 InstancedMesh（负责体素渲染）
    const group = new THREE.Group();
    group.position.set(...partDef.baseOffset, 0);  // 挂载到父部件的位置
    parent.add(group);

    // 创建该部位的 InstancedMesh（使用 VoxelMap 帧 0 作为初始几何）
    const initialVoxelMap = partDef.frames.idle?.[0] as VoxelMap;
    if (initialVoxelMap) {
      const mesh = this.createInstancedMesh(initialVoxelMap);
      group.add(mesh);
    }

    this.parts.set(name, { group, mesh, def: partDef });

    // 递归构建子部位
    for (const childName of BODY_HIERARCHY.children[name]) {
      this.buildPart(childName, def, group);
    }
  }

  /** 变身动画：按 timeline 逐个部位 scale 从 0→1 */
  transform(elapsedMs: number) {
    for (const [name, part] of this.parts) {
      const progress = getGrowProgress(elapsedMs, part.def.growAt);
      if (progress <= 0) {
        part.group.visible = false;
      } else if (progress >= 1) {
        part.group.visible = true;
        part.group.scale.setScalar(1);
      } else {
        part.group.visible = true;
        // easeOutBack 给躯干/腿部带回弹感
        const s = name === 'left_leg' || name === 'right_leg'
          ? easeOutBounce(progress)   // 腿用弹跳曲线（落地重量感）
          : easeOutBack(progress);    // 其他用回弹曲线
        part.group.scale.setScalar(s);
      }
    }
  }
}
```

**两种后端的数据映射对照表**：

| 概念 | Canvas 2D | Three.js |
|------|-----------|----------|
| 部位容器 | `ctx.save()/translate()/scale()/restore()` | `THREE.Group` + `position/scale` |
| 枢轴点 | `ctx.translate(-pivotX, -pivotY)` | `Group` 原点即枢轴（子 mesh 相对 Group 的偏移） |
| 像素数据 | `number[]` 调色板索引数组 → `fillRect` | `VoxelMap` → `InstancedMesh` instance matrices |
| 层级联动 | 递归函数中 ctx 变换自动继承 | Scene Graph 父子关系自动继承 |
| 生长动画 | `ctx.scale(easeOutBack(t))` | `group.scale.setScalar(easeOutBack(t))` |
| 可见性控制 | `if (progress <= 0) return` 不绘制 | `group.visible = false/true` |
| 面朝翻转 | `ctx.scale(-1, 1)` 整体翻转 | `root.scale.x = -1` 或绕 Y 轴旋转 |

#### 3.7.5 变身 Timeline 定义

```typescript
interface TransformTimeline {
  totalDuration: number;       // 总时长 ms
  steps: TransformStep[];
}

interface TransformStep {
  time: number;                // 触发时间点（ms）
  part: string;                // 目标部位名，'all' 表示整体
  action: 'grow' | 'settle';  // grow=生长显现, settle=整体定格弹跳
  easing: EasingFunction;      // 缓动曲线
}

const GAME_ENTER_TIMELINE: TransformTimeline = {
  totalDuration: 1500,
  steps: [
    { time: 0,    part: 'head',     action: 'grow',   easing: linear },       // 头不变
    { time: 400,  part: 'torso',    action: 'grow',   easing: easeOutBack }, // 躯干从脖子冒出（回弹）
    { time: 600,  part: 'left_arm', action: 'grow',   easing: easeOutCubic }, // 左手展开
    { time: 650,  part: 'right_arm',action: 'grow',   easing: easeOutCubic }, // 右手稍晚（节奏错开）
    { time: 1000, part: 'left_leg', action: 'grow',   easing: easeOutBounce },// 左腿弹出（弹跳感）
    { time: 1050, part: 'right_leg',action: 'grow',   easing: easeOutBounce },// 右腿稍晚
    { time: 1300, part: 'all',      action: 'settle', easing: spring },       // 整体微弹跳落地
  ],
};

// 退出游戏模式：倒序播放（腿先缩 → 手收回 → 身体缩小 → 回到大头猫）
const GAME_EXIT_TIMELINE: TransformTimeline = {
  totalDuration: 1000,  // 退出更快，更有"收回"感
  steps: [
    { time: 0,    part: 'left_leg',  action: 'shrink', easing: easeInBack },
    { time: 50,   part: 'right_leg', action: 'shrink', easing: easeInBack },
    { time: 200,  part: 'left_arm',  action: 'shrink', easing: easeInCubic },
    { time: 250,  part: 'right_arm', action: 'shrink', easing: easeInCubic },
    { time: 400,  part: 'torso',     action: 'shrink', easing: easeInBack },
    // head 始终保持可见（回到 Q 版大头猫形态）
  ],
};
```

各部位生长方式与视觉效果的对应关系：

| 部位 | 生长方向 | 缓动曲线 | 视觉效果 |
|------|---------|---------|---------|
| **torso** | 从 neck 向下 | `easeOutBack` | 像是从头里"挤"出来，末端微回弹 |
| **arms** | 从 shoulder 向外伸展 | `easeOutCubic` | 挥手臂般自然展开 |
| **legs** | 从 hip 向下弹出 | `easeOutBounce` | 落地时的重量感和弹性 |
| **整体 settle** | 微缩放回弹 | `spring` 物理 | "站稳了"的定格感 |

#### 3.7.6 IPC 通信：进入/退出游戏模式

Rust 侧新增两个 IPC 命令，前端监听事件驱动变身动画：

```rust
// app/src/lib.rs（新增）

/// 进入游戏模式：通知前端播放变身动画（长出身体）
#[tauri::command]
async fn cmd_enter_game_mode(window: tauri::Window) -> Result<(), String> {
    window.emit("game-mode-enter", ())?;
    Ok(())
}

/// 退出游戏模式：通知前端播放反向变身（缩回身体）
#[tauri::command]
async fn cmd_exit_game_mode(window: tauri::Window) -> Result<(), String> {
    window.emit("game-mode-exit", ())?;
    Ok(())
}
```

前端事件处理（`app.js` 扩展）：

```javascript
// 在 setupTauriEvents() 中新增
window.__TAURI__.event.listen('game-mode-enter', () => {
  console.log('[pet] ▶ 进入游戏模式 — 开始变身');
  transformAnimator.start(GAME_ENTER_TIMELINE);
});

window.__TAURI__.event.listen('game-mode-exit', () => {
  console.log('[pet] ◀ 退出游戏模式 — 反向变身');
  transformAnimator.start(GAME_EXIT_TIMELINE);
});
```

主循环集成（`app.js` 的 `loop()` 函数扩展）：

```javascript
function loop(now) {
  const dt = now - lastTime;
  lastTime = now;

  if (!collapsed) {
    if (transformAnimator.active) {
      // 变身动画优先级最高，劫持渲染
      transformAnimator.update(dt);
      voxelCharacter.render(ctx || renderer);  // 根据后端选择
    } else if (dancePlayer) {
      updateDance(dt);
    } else {
      // 正常模式：状态机驱动各部位独立帧
      pet.update(dt);
      voxelCharacter.setState(pet.state, pet.frame);
      voxelCharacter.render(ctx || renderer);
      Particles.tick(pet.state, dt);
    }
  }

  requestAnimationFrame(loop);
}
```

#### 3.7.7 与现有功能的兼容性

| 现有功能 | 如何适配部位组合模型 |
|---------|-------------------|
| **状态机** (`pet.rs` / `pet.js`) | 零改动。状态切换时各部位自动选取对应 `frames[state][frame]` |
| **舞蹈系统** (`play-dance`) | 扩展：dance step 可指定 `target_part` + `rotation/offset`，驱动特定部位运动 |
| **折叠态** (48×48) | 只渲染 `head` 部位的 mini 版（其他部位 `visible = false`） |
| **面朝翻转** (`facingRight`) | 对整个 Character root 做 `scaleX = -1`（Canvas 2D）或 `root.scale.x = -1`（Three.js） |
| **吸附竖条** | 变身状态下禁用吸附（或只显示 head 进入 snap 模式） |
| **AI 换装** | AI 输出新的部位 `frames` 数据 → 替换对应 `PartFrameData[]` → 下一帧生效 |
| **拖拽** | 拖拽移动整个 root Group / ctx 坐标系，部位相对位置不变 |

---

## 4. 与现有系统的集成

### 4.1 保持不变的部分

| 模块 | 文件 | 说明 |
|------|------|------|
| 状态机 | `core/src/pet.rs` | 纯逻辑，不涉及渲染，零改动 |
| 前端状态机镜像 | `app/frontend/js/pet.js` | 同上 |
| IPC 通信 | `lib.rs` → JS events | `pet-event` / `play-dance` 协议不变 |
| 窗口管理 | Tauri window API | 透明、拖拽、吸附逻辑不变 |
| Bubble 窗口 | `bubble.rs` + HTML | 独立窗口，不受影响 |
| Panel 窗口 | `panel.html` | 独立窗口，不受影响 |
| AI Agent | `agent.rs` | 输出格式扩展（新增 voxel 字段），核心不变 |

### 4.2 需要替换的部分

| 当前文件 | 替换为 | 改动范围 |
|---------|--------|---------|
| `sprite.js`（SPRITES + renderSprite） | `voxel-cat.js`（VoxelMap + VoxelRenderer） | 重写渲染原语 |
| `app.js` 中的 `SpriteRenderer.renderSprite()` 调用 | `VoxelCat.render()` 调用 | 主循环替换 |
| `particles.js`（Canvas 2D 粒子） | Three.js `Points` 粒子系统 | 可并行过渡 |
| `dancePlayer` 渲染分支 | VoxelAnimation 播放器 | 扩展现有协议 |

### 4.3 IPC 协议扩展

现有 `play-dance` 事件已能承载动画数据，扩展为支持 voxel：

```typescript
// 现有协议（保持兼容）
interface DanceEvent {
  name: string;
  steps: Array<{ action: string; duration_ms: number }>;
  loop_?: boolean;
}

// 扩展协议（新增可选字段）
interface VoxelEvent extends DanceEvent {
  voxel_map?: VoxelMap;           // 可选：完整的 voxel 地图（换装/变形）
  voxel_anim?: VoxelAnimation;    // 可选：体素级动画
}
```

前端收到事件时：
- 有 `voxel_map` → 重建 VoxelRenderer（换装）
- 有 `steps` → 播放动画（复用现有 dancePlayer 逻辑）
- 都没有 → 回退到状态机驱动（idle/walk/sleep...）

---

## 5. AI 生成管线

### 5.1 与现有舞蹈系统的同构性

当前舞蹈系统已经验证了「AI 输出 → 前端播放」的模式：

```
用户请求跳舞 → agent.chat_stream() → AI 输出 dance JSON
  → cmd_play_dance() → app.emit("play-dance", payload)
  → JS dancePlayer = { steps, index, time, loop }
  → requestAnimationFrame 循环渲染
```

3D 化后扩展为：

```
用户请求"变成机器人" → AI 输出 voxel_map JSON
  → cmd_update_voxel() → app.emit("update-voxel", payload)
  → JS VoxelCat.rebuild(voxel_map)
  → Three.js InstancedMesh 更新
```

### 5.2 AI 可输出的内容类型

| 类型 | 输出示例 | 用途 |
|------|---------|------|
| **静态外观** | 完整 VoxelMap | 换装、变身、节日皮肤 |
| **表情动画** | 局部 voxel 位移 + 变色 | 眨眼、嘴型变化、情绪表达 |
| **动作序列** | AnimationStep[] | 跳舞、攻击、施法 |
| **游戏场景** | 多个 VoxelMap + 位置 + 交互规则 | 生成小游戏关卡 |

### 5.3 Prompt 工程建议

在 `prompts.yml` 的 `agent.preamble` 中追加 voxel 输出规范：

```yaml
agent:
  preamble: |
    你是一个可爱的 3D 体素猫助手。当用户要求改变外观或生成 3D 内容时，
    使用 voxel_map 格式输出。voxel_map 是一个三维数组，size 为 [W,H,D]，
    voxels 使用调色板索引：0=空, 1=轮廓(#1E1E28), 2=肤色(#FFB48C),
    3=高光(#FFDCBE), 4=眼睛(#282332), 5=嘴巴(#FF788C)。
```

---

## 6. 实施路线图

### Phase 1：桌宠 3D 化（核心）

**目标**：将 pet 窗口的渲染从 Canvas 2D 像素替换为 Three.js 体素 + 组合式部位模型

**任务清单**：

#### 1.1 基础设施
- [ ] 引入 Three.js（npm install three 或 CDN 引入）
- [ ] 创建 `voxel-cat.js`：VoxelMap 数据结构 + VoxelRenderer 类
- [ ] 实现 InstancedMesh 渲染管线（正交等距相机 + 光照，见 3.3 节）

#### 1.2 部位数据定义（见 3.7.3 节）
- [ ] 定义 `BodyPart` 接口 + `CHARACTER` 常量（6 个部位：head/torso/arms/legs）
- [ ] 定义 `BODY_HIERARCHY` 层级树
- [ ] 绘制 head 部位的多状态帧像素数据：idle(4帧)、walk(4帧)、sleep(2帧)、talk(3帧)、happy(3帧)、confused(2帧)
- [ ] 绘制 torso 部位的帧数据
- [ ] 绘制 left_arm / right_arm 的帧数据（idle + wave + walk 各状态）
- [ ] 绘制 left_leg / right_leg 的帧数据（idle + walk 各状态）
- [ ] 将每个部位的像素数据转换为 VoxelMap 格式（或保留 Canvas 2D 数组由渲染器适配）

#### 1.3 组合渲染器（双后端，见 3.7.4 节）
- [ ] 实现 `VoxelCharacter` 类（Three.js Scene Graph 版）：递归构建 Group 层级 + InstancedMesh
- [ ] 实现 Canvas 2D fallback 渲染函数（`renderCharacter2D`）
- [ ] 实现部位帧切换逻辑：根据 `pet.state` + `pet.frame` 选取各部位对应帧
- [ ] 实现 facingRight 翻转：root `scaleX = -1`（Three.js）或 `ctx.scale(-1, 1)`（Canvas 2D）

#### 1.4 变身动画系统（见 3.7.5 节）
- [ ] 定义 `GAME_ENTER_TIMELINE` 和 `GAME_EXIT_TIMELINE`
- [ ] 实现 `TransformAnimator` 类：按 timeline 驱动各部位 `scale` 0→1 或 1→0
- [ ] 实现缓动函数库：`easeOutBack`、`easeOutBounce`、`easeOutCubic`、`spring`
- [ ] Rust 侧新增 `cmd_enter_game_mode` / `cmd_exit_game_mode` IPC 命令
- [ ] 前端监听 `game-mode-enter` / `game-mode-exit` 事件 → 启动变身动画

#### 1.5 集成与替换
- [ ] 替换 `app.js` 主循环中的 `SpriteRenderer.renderSprite()` → `VoxelCharacter.render()`
- [ ] 主循环增加 `transformAnimator.active` 分支（优先级最高，劫持渲染）
- [ ] 实现折叠态 mini 渲染：只显示 head 部位，其他 visible=false
- [ ] 移除旧的 `sprite.js` 渲染调用（保留数据供参考）
- [ ] 手动测试：透明窗口、拖拽、吸附、折叠/展开、所有状态切换、进入/退出游戏模式变身

**验收标准**：
- pet 窗口显示 3D 体素猫（组合式部位），背景透明
- 所有 6 种状态正常切换和动画（各部位独立选取帧）
- **进入游戏模式时播放 ~1.5s 变身动画：躯干→手臂→腿部依次长出**
- **退出游戏模式时反向播放：身体缩回 Q 版大头猫**
- 拖拽、吸附、折叠功能不受影响
- 性能：CPU < 1%，内存增量 < 20MB

### Phase 2：动画增强

**目标**：让 3D 猫"活"起来

**任务清单**：

- [ ] 实现呼吸微动（idle 时全体素 Z 轴正弦偏移 ±0.05）
- [ ] 实现眨眼动画（眼部 voxel 临时变色/消失）
- [ ] 走路动画改进（腿部 voxel 组交替抬升 + 身体轻微上下起伏）
- [ ] 粒子系统迁移到 Three.js `Points`（开心时星星、困惑时问号）
- [ ] 舞蹈系统迁移到 VoxelAnimation（jump/spin/wave/shake 用 voxel 位移实现）
- [ ] 简单的鼠标交互：hover 时猫转头看鼠标（头部 voxel 组旋转）
- [ ] 光照变化：根据系统时间调整色温（白天暖光/夜晚冷光）

**验收标准**：
- idle 有明显呼吸感
- 走路动画比 2D 版更生动
- 舞蹈效果 3D 化（跳跃有抛物线轨迹、旋转有真实翻滚感）
- 粒子效果不低于 2D 版品质

### Phase 3：游戏生成能力

**目标**：AI 可以生成可交互的 3D 游戏内容

**任务清单**：

- [ ] 新增 game 窗口类型（Tauri 多窗口，大画布如 960×640）
- [ ] 切换到 PerspectiveCamera + OrbitControls
- [ ] 实现 voxel 地形渲染（地面 + 障碍物）
- [ ] 第一人称/第三人称控制器（键盘 WASD + 鼠标视角）
- [ ] 简单碰撞检测（AABB vs voxel grid）
- [ ] AI 输出协议扩展：游戏场景描述（地形、目标、规则）
- [ ] 物理引擎集成（cannon-es，用于抛物、重力）
- [ ] 音效系统（Web Audio API）
- [ ] 游戏 UI（血条、物品栏、对话框 —— HTML overlay）

**验收标准**：
- AI 能生成一个完整的可玩 3D 小游戏（如迷宫探索、平台跳跃）
- 用户可以自由操控角色
- 游戏运行在独立窗口，不影响桌宠功能

---

## 7. 窗口尺寸考量

当前 128×128 对于 3D 来说偏小。建议按阶段调整：

| 阶段 | pet 窗口尺寸 | 说明 |
|------|------------|------|
| 现在 | 128×128 | 2D 像素，足够 |
| Phase 1 | **160×160** 或 **192×192** | 3D 体素需要稍大空间展示深度 |
| Phase 2 | 160×160（不变） | 动画增强不需更大窗口 |
| Phase 3 | pet 保持 160×160 | game 窗口独立，可达 960×640 或全屏 |

放大窗口的同时需同步更新：
- `lib.rs` 中 `create_pet_window()` 的宽高参数
- `app.js` 中 `canvas.width/height`
- 吸附竖条宽度（比例调整）
- 折叠态尺寸（建议 56×56 或 64×64）

---

## 8. Fallback 策略

如果 WebGL 在用户环境不可用（罕见但可能）：

```javascript
function createRenderer(canvas) {
  // 尝试 WebGL
  try {
    const gl = canvas.getContext('webgl', { alpha: true }) ||
               canvas.getContext('experimental-webgl', { alpha: true });
    if (gl) return new ThreeJSVoxelRenderer(canvas);
  } catch (_) {}

  // Fallback: Canvas 2D 等距投影
  console.warn('[pet] WebGL 不可用，降级到 Canvas 2D 等距渲染');
  return new Canvas2DFallbackRenderer(canvas);
}
```

Canvas 2D fallback 用画家算法 + 等距投影公式绘制伪 3D，保证功能可用只是视觉效果降级。

---

## 9. 风险与缓解

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| WebView2 + WebGL 透明在某些 Win10 版本异常 | 桌宠显示异常 | 低 | Fallback 到 Canvas 2D；要求 Win11+ |
| Three.js 包体积影响启动速度 | 首次加载慢 ~200ms | 低 | 本地文件加载；可预编译 chunk |
| 128→192px 窗口放大破坏现有布局 | 吸附/折叠坐标错位 | 中 | 全面测试；按比例缩放所有硬编码值 |
| Voxel 数据手写繁琐 | 开发效率低 | 中 | 先做 2-3 个状态验证管线；后续写编辑器工具或 AI 辅助生成 |
| AI 输出的 voxel 数据质量不稳定 | 游戏内容不可玩 | 中 | Phase 3 先做模板库 + 参数化变形，AI 只调参数 |
| InstancedMesh 不支持单 voxel 显隐（只能变透明色） | 动画受限 | 低 | 用颜色索引 0（透明材质）模拟隐藏；或改用普通 Mesh（性能仍足够） |
| **部位接缝处露缝或重叠** | **变身/动画时视觉瑕疵** | **中** | **部位间保留 1px 重叠区；枢轴点精确对齐；Three.js 中用轻微 Z-offset 避免深度冲突** |
| **变身动画时长/节奏不佳** | **体验不自然** | **中** | **timeline 参数可配置（写入 actions.yml 或 prompts.yml）；提供调试面板微调** |
| **6 个部位的帧数据量大** | **初始开发周期长** | **高** | **先只做 idle + walk 两个状态验证完整管线；其他状态后续补全；head 帧可复用现有 IDLE_BASE 数据** |

---

## 10. 参考资源

- **Three.js 文档**: https://threejs.org/docs/
- **InstancedMesh 性能**: https://threejs.org/examples/#webgl_instancing_mesh
- **Minecraft 风格体素渲染教程**: https://labs.phaser.io/view.html?src=src/3D%20Voxels/Voxel.js
- **等距投影数学**: https://en.wikipedia.org/wiki/Isometric_projection
- **Tauri 2.0 多窗口**: https://v2.tauri.app/window-management/
- **WebView2 透明背景**: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/overview
