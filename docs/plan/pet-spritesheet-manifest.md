# 宠物 Spritesheet Manifest 计划

> 状态：草案
> 更新日期：2026-05-15

本计划定义从当前 `sprite.js` 硬编码像素数组，迁移到外部、可由美术编辑的宠物资产格式的路径。它先作为设计草案存在：在 schema、fallback 规则和迁移路径稳定之前，不急于实现 loader。

## 目标

让宠物视觉资产数据化，同时保留当前内置 16x16 小猫的可靠性。

期望的最终状态：

- 内置 sprite 数据仍是安全 fallback；
- 外部宠物可以提供 `manifest.json` 和 spritesheet 图片；
- 动画时间轴、idle variants 和基础元数据放进 manifest；
- 用户最终可以不改 `app/frontend/js/sprite.js` 就添加新宠物；
- 前端能加载和渲染外部资产，同时继续保留语义化 `PetEvent` 行为。

## 非目标

- 不替换语义事件管线；`PetEvent`、`MoodPolicy` 和前端视觉映射继续保留。
- 不引入 embeddings、AI 自动选资产或运行时生成美术。
- 不要求 Rust 渲染 sprite。
- 在外部加载稳定前，不移除当前硬编码 sprite。
- 本次迁移不把舞蹈做成一等 `PetState` 变体。

## 当前约束

本地宠物渲染器目前默认：

- 每个 sprite frame 是 16x16 逻辑像素；
- `SPRITES[state][frame]` 返回一个长度为 256 的调色板索引数组；
- `renderSprite()` 通过固定 palette 绘制像素；
- `PetStateMachine` 输出视觉状态和 frame index；
- `renderMini()` 取当前状态第 0 帧，并裁剪头部区域；
- `PerformerHost` 可能渲染 `jump`、`spin`、`wave`、`shake` 等动作 sprite。

Manifest v1 应该贴合这个模型，而不是强迫重写渲染器。

## 资产布局

外部宠物资产应该放在用户数据目录下，而不是 app bundle 内：

```text
~/.ai-pad/pets/
  default-cat/
    manifest.json
    sprites.png
  dog/
    manifest.json
    sprites.png
```

App bundle 仍然通过 `sprite.js` 携带内置小猫。以后可以把内置小猫也导出成 bundled manifest 以保持一致，但第一版 loader 不需要依赖这一步。

## Manifest v1

第一版 schema 应保持小而明确。

```json
{
  "schemaVersion": 1,
  "id": "default-cat",
  "displayName": "Default Cat",
  "description": "The built-in 8-bit cat exported as a manifest-compatible pet.",
  "sprite": {
    "image": "sprites.png",
    "frameWidth": 16,
    "frameHeight": 16,
    "columns": 32,
    "rows": 1,
    "frameCount": 32
  },
  "palette": {
    "0": null,
    "1": [30, 30, 40, 255],
    "2": [255, 180, 140, 255],
    "3": [255, 220, 190, 255],
    "4": [40, 35, 50, 255],
    "5": [255, 120, 140, 255]
  },
  "states": {
    "idle": {
      "spriteFrames": [0, 1, 2, 3, 4, 5, 6],
      "frames": [
        { "sprite": 0, "duration": 1500 },
        { "sprite": 1, "duration": 120 },
        { "sprite": 2, "duration": 200 },
        { "sprite": 1, "duration": 120 },
        { "sprite": 0, "duration": 1800 }
      ],
      "loop": true,
      "variants": [
        {
          "name": "ear_twitch",
          "weight": 3,
          "cooldownMinMs": 8000,
          "cooldownMaxMs": 16000,
          "frames": [
            { "sprite": 4, "duration": 180 },
            { "sprite": 0, "duration": 140 }
          ]
        }
      ]
    },
    "focused": {
      "spriteFrames": [7, 8, 9, 10],
      "frames": [
        { "sprite": 7, "duration": 700 },
        { "sprite": 8, "duration": 140 },
        { "sprite": 7, "duration": 520 },
        { "sprite": 9, "duration": 120 }
      ],
      "loop": true
    },
    "happy": {
      "spriteFrames": [10, 11],
      "frames": [
        { "sprite": 10, "duration": 250 },
        { "sprite": 11, "duration": 120 },
        { "sprite": 10, "duration": 230 }
      ],
      "repeat": 3,
      "fallback": "idle"
    }
  },
  "actions": {
    "jump": { "sprite": 20 },
    "spin": { "sprite": 21 },
    "wave": { "sprite": 22 },
    "shake": { "sprite": 23 }
  },
  "mini": {
    "state": "idle",
    "frame": 0,
    "headRows": 10
  }
}
```

### 必需状态

只有包含以下状态的 manifest 才应视为有效：

- `idle`
- `walk`
- `sleep`
- `talk`
- `happy`
- `confused`

推荐状态：

- `focused`
- `preparing`
- `gameplay`
- `gamewin`
- `gamelose`

推荐状态缺失时，可以 fallback 到语义相近的状态：

| 缺失状态 | Fallback |
|---|---|
| `focused` | `idle` |
| `preparing` | `talk` |
| `gameplay` | `happy` |
| `gamewin` | `happy` |
| `gamelose` | `confused` |

## Loader 行为

推荐加载顺序：

1. 从设置中读取当前宠物 id。
2. 尝试读取 `~/.ai-pad/pets/<id>/manifest.json`。
3. 校验 manifest schema 和引用的 sprite 图片。
4. 将 `sprites.png` 加载到 offscreen canvas。
5. 根据 `frameWidth`、`frameHeight` 和 `columns` 切帧。
6. 把每一帧转换成当前渲染器使用的 palette-index 数组。
7. 构建运行时等价的 `SPRITES` 和 `STATE_CONFIG`。
8. 任何必需步骤失败时，记录 warning 并使用内置 sprite 数据。

v1 loader 应该采用 all-or-nothing 策略。局部加载外部资产很容易造成难排查的混合状态。

`spriteFrames` 是每个 state 的完整帧表，用于还原 `SPRITES[state]` 的局部帧索引；`frames[].sprite` 和 variant `frames[].sprite` 仍引用 spritesheet 的全局帧索引。没有 `spriteFrames` 时，loader 可退化为只用 timeline 中出现过的帧，但 idle variants 这类“被状态机按局部 index 触发”的帧会丢失，所以 fixture pack 应始终写出 `spriteFrames`。

## 校验规则

以下情况应拒绝 manifest：

- `schemaVersion` 不是 `1`；
- v1 中 `sprite.frameWidth` 或 `sprite.frameHeight` 不是 `16`；
- `frameCount` 超过 `columns * rows`；
- 任意 state 的 `frames` 数组为空；
- `spriteFrames` 存在但为空；
- 任意 frame 引用的 sprite index 不在 `[0, frameCount)` 范围内；
- 任意 duration `<= 0`；
- `repeat` 存在但不是正整数；
- `fallback` 引用了不存在的 state；
- variant 的 cooldown max 小于 cooldown min。

以下情况只 warning，不拒绝：

- 推荐状态缺失；
- 存在未知顶层 metadata 字段；
- 存在当前 app 未引用的额外 state。

## 渲染策略

v1 保留现有 palette-index 渲染器：

```mermaid
flowchart LR
  Image["sprites.png"] --> Slice["切分帧"]
  Slice --> Quantize["RGBA 映射到 palette index"]
  Quantize --> RuntimeSprites["SPRITES 等价数组"]
  Manifest["manifest.json"] --> RuntimeConfig["STATE_CONFIG 等价时间轴"]
  RuntimeSprites --> Renderer["renderSprite"]
  RuntimeConfig --> PetState["PetStateMachine"]
```

这样可以避免重写 `renderSprite()`，也能让现有测试继续有意义。如果未来 palette 量化限制太多，v2 再考虑直接渲染图片帧。

## Rust / Frontend 边界

v1 保持前端拥有视觉资产加载逻辑。

Rust 继续发送语义事件：

- `Notify`
- `React`
- `SetMode`
- `WalkTo`
- `PlayDance`

前端把这些事件映射到 manifest states。Rust 不需要知道 sprite index。

后续可选增强：

- 只有当设置页需要在打开宠物窗口前预览或拒绝外部宠物时，才增加 Rust 侧小型 manifest validator；
- 为诊断页生成精简的 state/timeline 摘要。

## 迁移计划

### Phase A：Schema 与测试 fixture

- 将 manifest schema 固化为文档。
- 创建一个镜像当前内置小猫的 fixture manifest。
- 增加一个小型 fixture spritesheet。
- 增加校验成功/失败测试。

### Phase B：带 fallback 的 loader

- 新增 `sprite-loader.js`。
- 如果配置了外部宠物，则尝试加载。
- 任意失败时回退到当前 `sprite.js` 数据。
- 保持公开 renderer API 稳定。

### Phase C：设置页与预览

- 增加设置页宠物 id 选择入口。
- 用易懂文案展示校验错误。
- 使用同一条 `renderSprite()` 路径提供预览。

### Phase D：导出内置资产

- 把当前小猫导出成 manifest-compatible asset pack。
- 至少发布一个带 loader 的版本后，再考虑是否弱化 `sprite.js` fallback。

## 待决策问题

- 自定义宠物是否只允许放在 `~/.ai-pad/pets`，还是开发期也允许 project-local pets？
- Palette 是否必须存在，还是 v1 就支持直接 RGBA 图片渲染？
- 外部宠物是否允许独立覆盖 performance action sprites？
- v1 是否固定 16x16 frame，还是允许 32x32 并带 scale factor？
- Manifest 校验只放前端，还是也通过 Rust 设置命令执行？

## 推荐下一步

实现 loader 之前，先从当前内置小猫导出一个小型 fixture pack：

```text
app/frontend/__fixtures__/pets/default-cat/
  manifest.json
  sprites.png
```

这个 fixture 能让 loader 测试不依赖用户数据目录，也能给当前 8-bit 小猫一个稳定的参考资产格式。
