# Steam 发布策略：免费基础版 + DLC 捆绑包

> 创建日期：2026-05-18
> 状态：规划中（未开始实施）
> 关联文档：
> - 宠物资源包分层：[plan/pet-asset-packaging.md](pet-asset-packaging.md)
> - 积分体系设计：[plan/progression-capability-unlock.md](progression-capability-unlock.md)
> - 小游戏系统：[plan/minigame-system.md](minigame-system.md)
> - 调研基础：[research/core-gameplay-progression-research.md](../research/core-gameplay-progression-research.md)

## 一、策略概述

**核心模式**：BitCat 桌宠本体免费上架 Steam，通过 DLC 捆绑包提供增值内容。

**为什么这个模式适合 BitCat**：

1. **桌宠天然是"钩子产品"** — 免费让用户把桌宠放在桌面上，陪伴感本身就是留存动力
2. **内容有清晰的"核心 vs 增值"边界** — AI 对话是核心，外观/额外游戏/配饰是增值
3. **代码已预留分层标记** — 宠物 manifest 的 `releaseTier: builtin | optional` 已经标注了哪些免费哪些付费
4. **Steam 桌宠品类验证过** — Wallpaper Engine、Bongo Cat 都采用"本体免费/低价 + DLC"模式

**不做的模式**：

| 模式 | 为什么不做 |
|------|----------|
| 全价付费（$10~15） | 桌宠不是刚需工具，付费门槛会扼杀用户获取 |
| 订阅制（月费） | 本地应用 + 云端 API 的组合不适合订阅，用户反感 |
| F2P + 微交易（抽卡/宝石） | 破坏桌宠的陪伴感，变成"另一个手游" |
| 按功能收费（$3 解锁 AI 工具） | AI 能力是核心体验，限制等于自杀 |
| 广告 | 桌面应用嵌入广告体验极差 |

---

## 二、内容分割方案

### 免费基础版（本体）

必须包含完整的、令人满意的桌宠体验。用户不花一分钱也要觉得"这东西真好用"。

| 模块 | 免费版内容 | 原因 |
|------|-----------|------|
| **宠物外观** | `piggy` + `cat`（2 个） | 一个默认终端风 + 一个经典像素风，足够体验核心 |
| **AI 对话** | 完整流式对话 + 情绪收尾 | 产品灵魂，动了就没有用户 |
| **记忆系统** | 短期记忆（字符预算注入）+ 长期记忆（200 条） | 对话质量的基础，200 条够日常使用 |
| **用户画像** | `user.yml` 完整功能 | 对话个性化的基础 |
| **舞蹈系统** | 5 个基础动作 + 2 个预设 + AI 编舞 | AI 编舞是核心差异化，不能锁 |
| **小游戏** | Snake / Memory / Catch / Battle 四个内置玩法 | 免费版直接证明"桌宠能玩"，DLC 以后主要卖皮肤/主题/扩展玩法 |
| **截图观察** | 手动截图 + 基础后台观察（30s 间隔） | AI 感知的基础能力 |
| **AI 工具** | `get_time` / `read_file` / `perform_dance` / `play_dance` / `search_memory` / `remember` | 基础 AI 能力 |
| **设置页** | 完整设置 + 成长系统展示 | 不限制设置功能 |
| **面板** | 2×2 游戏启动器 + YAML 可扩展快捷入口 | 基础交互 |
| **Agent Watch** | 基础只读模式 | 开发者/高级用户的入口 |

**免费版 exe 大小**：~23 MB（和当前几乎一样）

### 付费 DLC 内容

| DLC | 名称 | 建议价格 | 核心内容 |
|-----|------|---------|---------|
| **DLC 1** | 角色收藏包 | $1.99 | 10 个宠物外观 + 4 套气泡主题 + 配饰 + 问候语 |
| **DLC 2** | 游戏扩展包 | $1.99 | 3 个额外游戏 + 6 套游戏皮肤 + 音乐响应舞蹈 |
| **DLC 3** | 终极伙伴包 | $3.99 | DLC 1 + DLC 2 全部内容 + 独家内容 |

---

## 三、DLC 1：角色收藏包（$1.99）

### 包含内容

**10 个宠物资源包**：

| 宠物 ID | 风格 | 精灵图大小 | 质量层级 |
|---------|------|----------|---------|
| bsod | 蓝屏终端风 | 953 KB | polished |
| null-signal | 信号干扰风 | 489 KB | polished |
| dewey | 图书馆风 | 782 KB | standard |
| fireball | 火焰风 | 1.0 MB | standard |
| rocky | 岩石风 | 660 KB | standard |
| seedy | 种子风 | 915 KB | standard |
| byte-bun | 程序化生成 | 126 KB | generated |
| mossbot | 程序化生成 | 137 KB | generated |
| moonbit | 程序化生成 | 145 KB | generated |
| sparkle | 程序化生成 | 159 KB | generated |

总计约 5.5 MB 精灵图。当前代码中 `manifest.json` 已标注 `releaseTier: "optional"`。

**4 套气泡主题**：

| 主题 | 视觉风格 |
|------|---------|
| 樱花粉 | 粉色调 + 柔和圆角 + 花瓣粒子 |
| 终端绿 | 黑底绿字 + 等宽字体 + 光标闪烁 |
| 复古像素 | 像素边框 + bitcat 色彩 + 扫描线效果 |
| 深海蓝 | 深蓝渐变 + 水波纹 + 气泡粒子 |

**5 个宠物配饰**：

| 配饰 | 佩戴位置 | 精灵要求 |
|------|---------|---------|
| 小礼帽 | 头顶 | 8×8 px 叠加层 |
| 墨镜 | 眼睛区域 | 10×4 px |
| 领结 | 颈部 | 8×6 px |
| 皇冠 | 头顶 | 10×6 px |
| 蝴蝶结 | 耳朵 | 8×8 px |

配饰以精灵叠加层方式实现，不需要修改基础宠物 spritesheet。

**3 套问候语包**：

| 包名 | 风格 | 示例 |
|------|------|------|
| 元气模式 | 活泼积极 | "早上好呀！今天也要元气满满！" |
| 毒舌模式 | 傲娇关心 | "哦，你还记得回来啊。" |
| 猫语模式 | 大量猫叫 | "喵呜~ 喵喵！喵~" |

### 技术实现

```rust
// core/src/dlc.rs
pub enum DlcId {
    CharacterPack,   // DLC 1
    GameExpansion,   // DLC 2
    UltimateBundle,  // DLC 3
}

pub fn is_dlc_owned(dlc: DlcId) -> bool {
    // Steam API: ISteamApps::BIsDlcInstalled(dlc_app_id)
    // 离线模式回退到本地缓存
}
```

前端加载宠物时：

```javascript
// sprite-loader.js 扩展
async function loadPetAssetPack(baseUrl) {
  const manifest = await fetch(`${baseUrl}/manifest.json`).then(r => r.json());

  // DLC 门控
  if (manifest.releaseTier === 'optional' && !isDlcOwned('character_pack')) {
    throw new Error('此宠物需要角色收藏包 DLC');
  }

  // ... 正常加载流程
}
```

---

## 四、DLC 2：游戏扩展包（$1.99）

### 包含内容

**额外小游戏皮肤与扩展规则**：

| 游戏 | 引擎类 | 代码行数 | 特色 |
|------|--------|---------|------|
| Memory 记忆翻牌 | `MemoryEngine` | 已在本体 | DLC 提供高级牌面、主题和更大盘面预设 |
| Catch 接物 | `CatchEngine` | 已在本体 | DLC 提供季节物品、特殊规则和挑战预设 |
| Battle 射击对战 | `BattleEngine` | 已在本体 | DLC 提供敌人主题、弹幕皮肤和挑战波次 |

**6 套游戏皮肤**：

| 皮肤 | 适用游戏 | 改变内容 |
|------|---------|---------|
| 万圣节 | Snake | 蛇→幽灵，食物→南瓜 |
| 圣诞节 | Snake | 蛇→驯鹿，食物→礼物 |
| 海洋 | Memory | 牌面→海洋生物 |
| 太空 | Memory | 牌面→星球/飞船 |
| 糖果 | Catch | 下落物→糖果 |
| 忍者 | Battle | 宠物→忍者，怪物→妖怪 |

**音乐响应式舞蹈**：

`MusicReactiveDance` 是比基础舞蹈更高级的表现形式，分析系统音频让宠物随音乐节拍舞动。代码已有（`audio_reactive.rs` + `music-reactive-player.js`），只是门控到 DLC。

### 技术实现

游戏引擎创建时的门控：

```javascript
// game_engine.js 扩展
function createEngine(gameType, config) {
  const freeGames = ['snake'];
  const dlcGames = ['memory', 'catch', 'battle'];

  if (!freeGames.includes(gameType)) {
    if (!isDlcOwned('game_expansion')) {
      showDlcPrompt('游戏扩展包', `解锁 ${gameType} 需要"游戏扩展包" DLC`);
      return null;
    }
  }

  // ... 正常创建引擎
}
```

Rust 侧 `minigame.rs` 扩展：

```rust
impl MinigameType {
    pub fn requires_dlc(&self) -> Option<DlcId> {
        match self {
            MinigameType::Snake => None,           // 免费
            MinigameType::Memory => Some(DlcId::GameExpansion),
            MinigameType::Catch => Some(DlcId::GameExpansion),
            MinigameType::Battle => Some(DlcId::GameExpansion),
        }
    }
}
```

---

## 五、DLC 3：终极伙伴包（$3.99）

包含 DLC 1 + DLC 2 的全部内容，另加独家内容：

| 独家内容 | 说明 |
|---------|------|
| 5 个高级舞蹈预设 | 街舞/芭蕾/迪斯科/机械舞/雨中曲（超出基础 2 个预设） |
| 3 个桌面摆件精灵 | 小鱼缸/像素花盆/迷你地球仪（在桌面漂浮的装饰精灵） |
| 长期记忆扩容 | 200 条 → 1000 条 |
| 高级 AI 工具 | `launch_program` / `send_hotkey` / `read_clipboard` / `force_foreground` / `shell` 的基础访问权（仍需 progression Lv5 + 用户逐项授权） |

**重要原则**：高级 AI 工具的"基础访问权"不等于"自动启用"。用户仍需在 progression 系统中达到 Lv5 并逐项授权。DLC 只是提前解锁了"可以学"的资格，不跳过安全流程。

---

## 六、技术实现路径

### 6.1 Steam SDK 集成

当前项目已有 Steam SDK 探测逻辑（`900fec7 feat(steam): add local sdk probe`）。

```rust
// core/src/dlc.rs — 新增模块

/// Steam DLC 状态查询。
pub struct DlcStore {
    steam_initialized: bool,
    owned_dlcs: BTreeSet<DlcId>,
    /// 离线缓存，避免每次启动都查 Steam
    cache_path: PathBuf,
}

impl DlcStore {
    /// 初始化 Steam API 并查询已拥有的 DLC。
    pub fn init() -> Result<Self> {
        // 1. 尝试 steamworks crate 初始化
        // 2. 查询 BIsDlcInstalled for each DlcId
        // 3. 写入本地缓存
        // 4. 初始化失败时回退到缓存
    }

    pub fn is_owned(&self, dlc: DlcId) -> bool {
        self.owned_dlcs.contains(&dlc)
    }
}
```

依赖：`steamworks` crate（Rust Steam SDK 绑定）。

### 6.2 DLC App ID 分配

Steam 发布时需要为每个 DLC 分配独立 App ID：

| App ID | 内容 | 类型 |
|--------|------|------|
| 主 App ID | BitCat 桌宠本体 | 免费 |
| DLC App ID 1 | 角色收藏包 | $1.99 |
| DLC App ID 2 | 游戏扩展包 | $1.99 |
| DLC App ID 3 | 终极伙伴包 | $3.99 |

### 6.3 前端 DLC 门控

```javascript
// app/frontend/js/dlc.js — 新增模块

const DLC_IDS = {
  character_pack:  null,  // 运行时填充 Steam DLC App ID
  game_expansion:  null,
  ultimate_bundle: null,
};

async function initDlc() {
  // 从 Rust 侧查询已拥有的 DLC
  const owned = await invoke('cmd_get_owned_dlcs');
  Object.keys(DLC_IDS).forEach(id => {
    DLC_IDS[id] = owned.includes(id);
  });
}

function isDlcOwned(dlcKey) {
  return DLC_IDS[dlcKey] === true;
}

// 显示 DLC 购买提示（非侵入式）
function showDlcPrompt(title, message) {
  // 气泡提示 + Steam 商店链接
  // 不阻塞当前操作，只是告知
}
```

### 6.4 离线模式

Steam 不在线时回退到本地缓存。缓存文件：

```
~/.bitcat/dlc_cache.json  — { "character_pack": true, "game_expansion": false, ... }
```

缓存每 24 小时刷新一次。超过 7 天无法验证时，DLC 内容暂时不可用（Steam 政策要求 DRM 验证周期）。

### 6.5 不加 DRM 的备选方案

如果不想接入 Steam SDK（增加复杂度和依赖），可以用更简单的方式：

```rust
// 方案 B：纯文件存在性检查
fn is_dlc_owned_file(dlc: DlcId) -> bool {
    let path = exe_dir().join("dlc").join(dlc.folder_name());
    path.exists()
}
```

DLC 内容以文件夹形式存在于安装目录。免费版不包含这些文件夹，付费版（或手动购买 DLC 后）解压到对应位置。

这个方案更简单但不防盗版。对于桌宠来说，防盗版不是核心诉求——让更多人用上才是。

---

## 七、文件清单

### 新增文件

| 文件 | 内容 | 预估行数 |
|------|------|---------|
| `core/src/dlc.rs` | DlcId 枚举 + DlcStore + 门控逻辑 | ~200 |
| `app/frontend/js/dlc.js` | 前端 DLC 查询 + UI 提示 | ~120 |
| `config/dlc.yml` | DLC 定义（ID/名称/AppID/包含内容） | ~40 |

### 修改文件

| 文件 | 改动 | 预估行数 |
|------|------|---------|
| `core/src/lib.rs` | `pub mod dlc` | 1 |
| `core/src/minigame.rs` | `MinigameType::requires_dlc()` | ~15 |
| `app/frontend/js/sprite-loader.js` | 加载时检查 `releaseTier` + DLC 门控 | ~20 |
| `app/frontend/js/game_engine.js` | `createEngine` 加 DLC 检查 | ~15 |
| `app/src/game.rs` | 启动游戏前验证 DLC | ~10 |
| `app/src/lib.rs` | 注册 DlcStore + `cmd_get_owned_dlcs` | ~20 |
| `app/src/settings.rs` | 设置页展示 DLC 状态 | ~10 |
| `app/frontend/js/settings.js` | DLC 管理 tab | ~80 |

### 不修改的文件

以下文件**完全不需要改动**来支持 DLC 分割：
- `core/src/agent.rs` — AI 对话管线不变
- `core/src/memory.rs` — 记忆系统不变
- `core/src/vision.rs` — 截图系统不变
- `core/src/progression.rs`（未实现） — Bit 商店仍然用于免费内容解锁，DLC 内容是额外层

**总改动量：~530 行**（其中 200 行是新模块，其余是小改动）

---

## 八、定价策略

### 定价依据

| 参考 | 价格 | BitCat 对比 |
|------|------|----------|
| Bongo Cat Mver | 免费 | BitCat 有 AI，价值更高 |
| Wallpaper Engine | $3.99 | BitCat 是桌宠不是壁纸，但定价可参考 |
| Desktop Duck | 免费 | 简单桌宠，BitCat 复杂度高很多 |
| Stream Pets | 免费 + $4.99 高级版 | 最接近的模式 |

### 定价方案

```
本体：免费
  ├── 2 个宠物 + AI 对话 + 记忆 + 四个内置小游戏 + 基础舞蹈
  └── 完整核心体验，不阉割

DLC 1 角色收藏包：$1.99
  └── 10 个宠物 + 4 主题 + 5 配饰 + 3 问候语包

DLC 2 游戏扩展包：$1.99
  └── 3 个游戏 + 6 皮肤 + 音乐响应舞蹈

DLC 3 终极伙伴包：$3.99
  └── DLC 1 + DLC 2 + 独家内容（省 $0 打包购买）
```

DLC 3 和单独买 1+2 价格相同（$3.99 = $1.99 + $1.99），没有折扣。打包的意义是便利，不是价格激励。如果后续想促销，可以通过 Steam 季节性打折（-10%~-25%）。

### 折扣节奏建议

| 时机 | 折扣 | 目的 |
|------|------|------|
| 首发 2 周 | 本体免费，DLC 无折扣 | 建立价值认知 |
| Steam 夏促/冬促 | DLC -20% | 跟随平台流量 |
| 发售 3 个月后 | DLC -25%~33% | 扩大付费转化 |
| 重大更新时 | DLC -10% 短促 | 更新流量转化 |

---

## 九、风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| 免费版内容太少，用户觉得"没什么可玩" | 留存差 | 免费版必须包含完整 AI 对话 + 2 宠物 + 四个内置小游戏 + 编舞，核心体验不缩水 |
| 用户觉得"什么都要钱" | 差评轰炸 | 永远不在 UI 里主动弹 DLC 广告；只在用户主动尝试 DLC 内容时温和提示 |
| Steam SDK 集成增加复杂度 | 构建和维护成本 | 首版用文件存在性检查（方案 B），Steamworks 接入推迟到正式发布前 |
| DLC 内容被破解/复制 | 收入损失 | 桌宠的付费价值在于持续更新，不在于防盗版；被破解说明有人在乎 |
| 长期记忆扩容收费观感差 | 隐私/伦理争议 | 免费版 200 条已经够用；DLC 只是"更多"，不是"更好" |
| AI 工具收费被认为"功能阉割" | 用户信任 | AI 工具完全免费，DLC 只提供"提前解锁学习资格"，不跳过安全授权 |

### 绝对不做的事

1. **不弹 DLC 广告** — 不在气泡、面板、通知中推送付费内容
2. **不限制 AI 对话质量** — 免费和付费用同一个模型
3. **不卖隐私/安全相关功能** — 截图频率、记忆删除、权限管理永远免费
4. **不做 Pay-to-Win** — 小游戏不卖数值加成
5. **不做 FOMO 限定** — 不做"限时独占宠物"

---

## 十、实施阶段

### Phase A：DLC 基础设施（~200 行）

- 新增 `core/src/dlc.rs`：DlcId + 文件存在性检查（方案 B，不接 Steam SDK）
- 新增 `app/frontend/js/dlc.js`：`isDlcOwned()` + `showDlcPrompt()`
- 新增 `config/dlc.yml`：DLC 定义
- 修改 `sprite-loader.js`：加载时检查 `releaseTier`
- 修改 `game_engine.js`：创建引擎时检查 DLC

验收：免费版能看到但无法使用 DLC 宠物和游戏，提示"需要角色收藏包 DLC"。

### Phase B：内容分割

- 将 10 个 `optional` 宠物资源包从内置移动到 `dlc/characters/` 目录
- 将 Memory/Catch/Battle 引擎代码保持在本体内（代码不分隔），运行时门控
- 准备气泡主题 CSS 和配饰精灵图

验收：删除 `dlc/` 目录后免费版正常运行，只缺少付费内容。

### Phase C：Steam SDK 接入（发布前）

- 集成 `steamworks` crate
- 分配 DLC App ID
- 替换文件存在性检查为 `BIsDlcInstalled`
- 实现 `DlcStore` 离线缓存
- Steam 创意工坊预留（用户自制宠物皮肤上传）

验收：Steam 商店页可购买 DLC，购买后重启应用自动解锁。

### Phase D：首发上架

- Steam 商店页素材（宣传图/视频/描述）
- 免费本体 + 3 个 DLC 同时上架
- 首发无折扣，积累评价

---

## 十一、收入预估（参考）

保守估计（基于 Steam 桌宠品类的典型转化率）：

| 指标 | 保守 | 中性 | 乐观 |
|------|------|------|------|
| 首月下载量 | 2,000 | 5,000 | 15,000 |
| DLC 转化率 | 3% | 8% | 15% |
| 平均付费金额 | $2 | $3 | $4 |
| 首月收入 | $120 | $1,200 | $9,000 |
| 年累计收入 | $500 | $5,000 | $30,000 |

Steam 抽成 30%，实际收入 × 0.7。

关键变量：AI 对话质量（决定留存）、社区口碑（决定传播）、更新频率（决定长尾）。

---

## 十二、未来扩展方向

DLC 发布后可以持续追加内容（不需要改代码，只需新增资产 + 更新 `config/`）：

- **季节性宠物包**：春节限定宠物/圣诞限定配饰（时间限定 → FOMO 风险，需要谨慎）
- **RPG 扩展包**：如果 `topdown-rpg-ai.md` 计划落地，RPG 内容本身就是一个独立 DLC
- **创意工坊**：让用户自制宠物皮肤上传 Steam Workshop，免费的社区生态
- **联名宠物**：和独立游戏/VTuber 联名的宠物外观
