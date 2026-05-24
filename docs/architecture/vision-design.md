# 桌面截图 + AI 视觉分析设计方案

> 项目代号：BitCat | 技术栈：Tauri 2.0 + rig-core + windows-sys (Rust)
> 设计日期：2026-05-10 | 平台：Windows 11

## 一、目标

让 BitCat 能够**周期性或按需截取桌面屏幕**，通过 AI 视觉模型分析画面内容，从而实现：

- 被动观察：猫猫"看一眼"屏幕，气泡描述当前状态
- 主动建议：检测到编译错误/长时间空闲/新应用启动时主动反应
- 视觉辅助：OCR 读屏、UI 元素定位、配色分析等
- 与记忆系统联动：截图分析结果写入 facts.md，丰富长期记忆

## 二、截图技术方案

### 方案选型：BitBlt（零新依赖）

| 方案 | 截图耗时(1080p) | 新依赖 | 复杂度 |
|------|----------------|--------|--------|
| **BitBlt (推荐)** | 10-30ms | **无**（已有 `windows-sys`） | 低 ~40行 |
| DXGI Desktop Duplication | 2-5ms | `windows-capture` crate | 高 |
| `screenshots` crate | 20-50ms | 新依赖 | 中 |

**选择 BitBlt 的理由**：
1. 项目已有 `windows-sys = 0.61`，只需加 GDI features
2. 周期截图（30s~5min 间隔）不需要 DXGI 级性能
3. 代码最简，约 40 行核心逻辑

### Cargo.toml 改动

```toml
# app/Cargo.toml — 在现有 windows-sys 基础上追加 features
windows-sys = { version = "0.61", features = [
    "Win32_System_Console",          # 已有
    "Win32_Gdi",                     # ← 新增：BitBlt, CreateCompatibleDC, GetDIBits
    "Win32_Foundation",              # ← 新增：HBITMAP, RECT 等
    "Win32_UI_WindowsAndMessaging",  # ← 新增：GetDesktopWindow, ReleaseDC
]}
```

### 核心代码结构

```rust
// app/src/screenshot.rs

use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DeleteObject, DeleteDC, GetDC, SelectObject, GetDIBits,
    SRCCOPY, DIB_RGB_COLORS, BITMAPINFOHEADER, BITMAPINFO,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetDesktopWindow, ReleaseDC, EnumDisplayMonitors, MonitorFromPoint,
};
use windows_sys::Win32::Foundation::*;

// ---- 数据结构 ----

#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub pixels: Vec<u8>,       // BGRA 格式像素数据
    pub width: u32,
    pub height: u32,
    pub source: CaptureSource, // 来源信息（哪块屏/拼接）
}

#[derive(Debug, Clone)]
pub enum CaptureSource {
    Primary { device_name: String },
    Secondary { index: usize, device_name: String },
    All { screens: Vec<ScreenInfo },  // 拼接模式，记录每块屏的信息
}

#[derive(Debug, Clone)]
pub struct ScreenInfo {
    pub device_name: String,
    pub bounds: (i32, i32, i32, i32), // left, top, width, height
    pub is_primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ScreenshotTarget {
    Primary,
    Secondary(usize),
    All,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScreenshotConfig {
    pub target: ScreenshotTarget,          // 截哪块屏
    pub max_width: u32,                    // 最大输出宽度（默认 960，实测最佳性价比）
    pub jpeg_quality: u8,                  // JPEG 质量 1-100（默认 80）
    pub interval_sec: u64,                 // 空闲间隔秒数
    pub dedup: bool,                       // 是否变化检测去重
    pub similarity_threshold: f64,         // 相似度阈值（0-1）
    pub min_width: u32,                    // 最小宽度限制（低于此不发送 API）
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            target: ScreenshotTarget::All,
            max_width: 960,       // ← 实测验证：80% 准确率 + 32KB/张
            jpeg_quality: 80,     // ← 实测验证：甜点质量
            interval_sec: 300,
            dedup: true,
            similarity_threshold: 0.95,
            min_width: 480,       // ← 480px 以下识别率太低，不浪费调用
        }
    }
}

// ---- 核心函数 ----

/// 枚举所有显示器信息
pub fn enumerate_displays() -> Vec<ScreenInfo>;

/// 截取指定目标屏幕（支持多屏 / 拼接）
pub fn capture_target(target: &ScreenshotTarget) -> Result<CapturedFrame, String> {
    match target {
        ScreenshotTarget::Primary => capture_primary_screen(),
        ScreenshotTarget::Secondary(idx) => capture_secondary_screen(idx),
        ScreenshotTarget::All => capture_all_screens_stitched(),
    }
}

/// 单屏截图：BitBlt 指定显示器的 DC 区域
fn capture_display(hdc_monitor: HDC, bounds: &RECT) -> Result<CapturedFrame, String>;

/// 多屏拼接：按虚拟坐标从左到右排列，水平拼接
fn capture_all_screens_stitched() -> Result<CapturedFrame, String>;

/// 将 BGRA 像素编码为 JPEG 字节（用 image crate）
pub fn encode_jpeg(pixels: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>, String>;

/// 计算感知哈希（用于变化检测，跳过相似帧）
pub fn perceptual_hash(pixels: &[u8], width: u32, height: u32) -> u64;

/// 从文件加载配置（screenshot.yml 或 config/actions.yml [screenshot] 段）
pub fn load_config() -> Result<ScreenshotConfig, String>;
```

### 关键问题：透明窗口遮挡

pet / bubble 窗口是 `transparent(true) + always_on_top(true)`，**会出现在 BitBlt 截图中**。

解法：截图前临时隐藏，截图后恢复。

```
hide(pet_window)
hide(bubble_window)
  ↓  (~5ms)
BitBlt 截图
  ↓  (~20ms)
show(bubble_window)
show(pet_window)
  ↓
总耗时 ~30ms，5分钟一次的频率下用户完全感知不到闪烁
```

### 多显示器支持

#### 实际环境（当前用户配置）

| 屏幕 | 主/副 | 分辨率 | 虚拟坐标 | 说明 |
|------|-------|--------|---------|------|
| DISPLAY1 | **Primary** | 1536×960 | (0, 0) | 笔记本屏幕 |
| DISPLAY2 | Secondary | 1920×1080 | (-1920, 0) | 外接显示器（主屏左侧） |

**默认只截 Primary Screen 会漏掉副屏上的活动**（如参考文档、聊天窗口、日志面板等）。

#### 三种截图模式

```rust
/// 截图目标模式
#[derive(Debug, Clone, Deserialize)]
pub enum ScreenshotTarget {
    /// 只截主屏
    Primary,
    /// 只截副屏（按索引指定）
    Secondary(usize),
    /// 截所有屏幕，水平拼接成一张大图
    All,
}
```

| 模式 | 输出尺寸示例 | 适用场景 | Token 成本 |
|------|-------------|---------|-----------|
| `primary` | 1536×960 → 压缩后 ~50KB | 只关注主屏操作 | 低 |
| `secondary(1)` | 1920×1080 → 压缩后 ~65KB | 副屏是主力工作屏时 | 低 |
| `all` | 3456×1080 → 压缩后 ~120KB | 需要全局视野时 | 中（~2x） |

#### 拼接实现

`All` 模式下按虚拟桌面坐标排序，从左到右逐屏 BitBlt，再水平拼接：

```
虚拟桌面布局：
┌─────────────────────┬──────────────────┐
│   DISPLAY2          │   DISPLAY1       │
│   (-1920,0)         │   (0,0)          │
│   1920×1080         │   1536×960       │
│   [副屏]            │   [主屏]          │
└─────────────────────┴──────────────────┘
                         ↑ Primary

All 模式输出：3456×1080（两屏拼接）
```

```rust
/// 截取指定目标屏幕（支持多屏）
pub fn capture_target(target: &ScreenshotTarget) -> Result<CapturedFrame, String> {
    match target {
        ScreenshotTarget::Primary => {
            // GetDC(0) + BitBlt 主屏区域（等价于旧逻辑）
        }
        ScreenshotTarget::Secondary(idx) => {
            // EnumDisplayMonitors 找到第 N 个非主屏
            // BitBlt 该屏的 hdc + 偏移量
        }
        ScreenshotTarget::All => {
            // 1. EnumDisplayMonitors 枚举所有显示器
            // 2. 按 Bounds.Left 排序（从左到右）
            // 3. 逐屏 BitBlt 到各自内存位图
            // 4. 计算总宽 = Σ各屏宽度，总高 = max(各屏高度)
            // 5. 创建拼接画布，逐屏 Blt 过去
            // 6. 返回整张拼接图
        }
    }
}
```

#### 用户配置

在 `config/actions.yml` 或独立 `config/screenshot.yml` 中配置：

```yaml
# screenshot.yml（或嵌入 config/actions.yml 的 [screenshot] 段）
screenshot:
  # 截图目标: primary / secondary / all
  target: "all"

  # 如果选 secondary，指定索引（从 0 开始，跳过 primary）
  secondary_index: 1

  # 输出分辨率（统一默认 960，实测最佳性价比点）
  max_width: 960       # 默认 960；0 = 不缩放

  # JPEG 质量 (1-100)，实测 80 是甜点
  jpeg_quality: 80      # 80 → ~32KB/张，AI 能读大部分文字

  # 间隔秒数（空闲兜底模式的间隔）
  interval_sec: 300     # 5 分钟一次

  # 是否启用变化检测（感知哈希去重）
  dedup: true
  similarity_threshold: 0.95  # >95% 相似则跳过

  # 最小宽度限制（低于此值不发送 API，浪费调用）
  min_width: 480         # 480px 以下识别率太低
```

也可以通过**面板 UI** 动态切换：

```
┌─ 截图设置 ──────────────────┐
│                               │
│  目标屏幕:  (●) 全部屏幕      │
│             ( ) 仅主屏        │
│             ( ) 仅副屏 2      │
│                               │
│  间隔:    [5] 分钟 ▼          │
│  质量:    [80] %              │
│                               │
│  当前状态: ● 观察中           │
│  最近截图: 14:32 (双屏拼接)   │
│                               │
│  [预览最近一张]  [立即截取]    │
└───────────────────────────────┘
```

#### 多屏场景下的 AI Prompt 调整

多屏截图时 prompt 需要提示 AI 理解布局：

```rust
const VISION_PROMPT_MULTI_MONITOR: &str = r#"你是 BitCat，一个住在电脑屏幕边缘的桌面 AI 伙伴。
你刚刚看了一眼主人的屏幕（可能是多块屏幕拼接的）。用一句话描述你看到了什么。

注意：
- 如果看到左右两部分明显不同的内容，说明是多屏拼接，分别描述
- 左边通常是副屏，右边通常是主屏
- 关注正在被使用的活跃窗口，不用描述空白桌面背景

语气活泼可爱，30字以内。
"#;
```

### 性能特征

| 操作 | 单屏 (1536×960) | 双屏拼接 (3456×1080) | 说明 |
|------|-----------------|---------------------|------|
| hide/show 窗口 | ~5ms 各 | ~5ms 各 | Tauri window 操作 |
| BitBlt 拷贝 | ~15-25ms | ~35-50ms（×2 屏 + 拼接 Blt） | CPU 内存拷贝 |
| GetDIBits 提取像素 | ~5-10ms | ~15-20ms | 取决于总像素数 |
| JPEG 编码 (缩放后) | ~10-20ms | ~25-40ms | 用 image crate |
| **总计** | **~40-60ms** | **~80-120ms** | 在独立线程执行，不阻塞 gamepad_loop |

双屏拼接约 2x 耗时和 2.5x 文件量，但仍在可接受范围（5分钟一次）。

## 三、AI 视觉分析

### API 格式

当前模型 `glm-5v-turbo` 支持 vision。消息格式遵循 Anthropic Image Block 规范：

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "描述一下屏幕上正在发生什么？你在哪些应用窗口里？" },
    {
      "type": "image",
      "source": {
        "type": "base64",
        "media_type": "image/jpeg",
        "data": "/9j/4AAQSkZJRg...（base64）"
      }
    }
  ]
}
```

### rig-core 集成状态

早期实现曾因 rig-core v0.36 的 image content block / Extractor 组合需要验证，而在 `core/src/vision.rs` 中保留独立 HTTP 客户端和 Anthropic-compatible 请求构造。该判断已经过期：当前 vision 主链路已使用 rig `Extractor<VisionAnalysis>`，图片通过 `UserContent::image_base64()` 进入 message。

B3 的最终决策是：**不再把自由文本作为主接口，也不为旧截图/摘要数据保留兼容层**。

**当前路径：rig Extractor**
- 用 `VisionAnalysis` 作为截图分析的强类型返回值
- 用 `StructuredSummary` 作为屏幕摘要的强类型返回值
- bubble 只显示 `analysis.description`，存储和记忆注入使用结构字段派生
- 旧 raw request helper、自由文本解析路径和 `parse_anthropic_usage()` 已删除

**不采用：扩展 Tool**
- 不给 PetAgent 增加 `screenshot_analysis` tool
- 截图观察是后台观察链路，不是用户对话中由模型自行选择的工具能力

### 成本优化（实测数据）

> 以下数据来自 2026-05-10 实测：同一张桌面截图（1536×960 原始 PNG），经 glm-5v-turbo vision API 验证。

#### 分辨率 vs 文件大小 vs AI 识别准确度

| 分辨率 | JPEG 质量 | 文件大小 | AI 能否读文件名 | 准确度 | 单次成本 |
|--------|---------|---------|----------------|--------|---------|
| 1536×960 (原始 PNG) | 无损 | **1006 KB** | ✅ 清晰可读 | 85% | ~$0.008 |
| **1280×800 (默认)** | 95 | **126 KB** | ✅ **几乎全部可读** | **95%** | ~$0.004 |
| **960×600 (推荐)** | **80** | **~32 KB** | ✅ 大部分可读，小字模糊 | **80-90%** | **~$0.002** ⭐ |
| 960×600 | 60 | **23 KB** | ✅ 可读（偶尔更准） | 90% | ~$0.0015 |
| 640×400 | 80 | **17 KB** | ⚠️ 图标可辨，文字不可读 | 诚实说"看不清" | ~$0.001 |
| 480×300 | 70 | **9 KB** | ❌ 接近不可用（SSL 可能失败） | — | — |

**结论：统一使用 960×600 / JPEG quality=80 作为默认配置。**
- 32KB 文件量，API 调用快速
- 大部分文件名/图标能正确识别
- 成本极低（约 ¥0.015/次），5 分钟一次可忽略不计
- 不再区分"通用观察"和"详细分析"模式——统一体验更好

### 图片处理流水线（已验证）

```
BitBlt 原始 BGRA          等比缩放              JPEG 编码              Base64 → API
┌──────────────┐      ┌──────────────┐       ┌──────────────┐      ┌──────────────┐
│ 1536×960     │      │  960×600     │       │  JPEG q=80   │      │  base64      │
│ 5.6 MB (BGRA) │ ───► │  1.4 MB      │ ───►  │  ~32 KB      │ ───►  │  ~43 KB      │
│               │  -75% │  (等比缩放)    │  -98% │  (有损压缩)   │  +33%  │  (发送格式)    │
└──────────────┘      └──────────────┘       └──────────────┘      └──────────────┘
总压缩比: 5.6 MB → 43 KB (130:1)    Token: ~350t    成本: ~¥0.015/次
```

### Prompt 设计（已验证：反幻觉版）

> **重要：经过实测验证，不加约束时 AI 会编造文件名（如把模糊文字读成 `ai`、`gene-family-agent`）。以下提示词已在 glm-5v-turbo 上测试通过。**

```rust
/// 统一使用的反幻觉视觉提示词（所有场景共用）
const VISION_PROMPT: &str = r#"你是 BitCat，一个住在电脑屏幕边缘的桌面 AI 伙伴。你刚刚看了一眼主人的屏幕。

严格遵守以下规则：
1. 如果你无法看清文字、标签、文件名，必须说"看不清"，绝对不要猜测或编造
2. 对于模糊的图标，只描述颜色和形状，用"看起来像是"而非"就是"
3. 不要编造任何具体的名称、数字、文字内容
4. 与其编造细节，不如诚实说"这个太小了喵~我看不太清"
5. 回复控制在 80 字以内，语气活泼可爱，像猫的视角

请描述你看到的屏幕内容。"#;
```

#### 实测效果对照（同一张桌面截图）

| 测试项 | 无约束提示词 | 反幻觉提示词后 |
|--------|-------------|---------------|
| 文件夹名识别 | ❌ 编造 `ai`、`gene-family-agent` | ✅ 只说"黄色文件夹"，不编名字 |
| 图标身份断言 | ❌ 断言"VSCode""终端" | ✅ "看起来像是蓝色图标" |
| 不确定时的表达 | ❌ 自信给出错误细节 | ✅ "名字好像被截断了""看不太清" |
| 低分辨率(640px)时 | — | ✅ 主动说"有些文件名太小了喵~我看不太清" |

**关键发现：高分辨率下模型确实能"阅读"文字（1280 时准确报出了 `hyde.ppt/svg/pdf/png`、`回收站`、`tmp`、`ai`），低分辨率下反幻觉提示词效果最佳——两者配合使用最安全。**

### 不同场景的 prompt 变体

在 `VISION_PROMPT` 基础上追加场景特定指令：

```rust
// 异常检测模式（追加到 VISION_PROMPT 后面）
const VISION_APPEND_ALERT: &str = r#"
额外任务：检查屏幕上是否有异常。
重点关注：编译/构建错误（红色文字）、弹窗警告、程序崩溃提示。
如果有异常简要说明；没有则说"一切正常"。"#;

// OCR 读屏模式（追加到 VISION_PROMPT 后面）
const VISION_APPEND_OCR: &str = r#"
额外任务：读取屏幕上所有可见的文字内容。
按区域分组输出，保持原文语言。"#;

示例回复：
- "主人又在 VSCode 里写 Rust 呢~"
- "浏览器开了好多标签页！🐱"
- "咦，终端红了，是不是报错了？"
"#;

// 场景 2：异常检测
const VISION_PROMPT_ALERT: &str = r#"你是 BitCat，关注屏幕上的异常情况。
检查是否有：编译/构建错误、弹窗警告、程序崩溃、网络断开等。
如果有异常，简要说明；如果没有，说"一切正常"即可。
"#;

// 场景 3：OCR 读屏
const VISION_PROMPT_OCR: &str = r#"读取屏幕上所有可见的文字内容。
保持原文语言和格式，按区域分组输出。
"#;
```

## 四、触发策略

### 混合触发模型

```
┌─────────────────────────────────────────┐
│           触发决策器                      │
├─────────────────────────────────────────┤
│                                         │
│  用户主动触发 ──────► 立即截图+分析       │
│  （手柄按键 / 点击猫猫）                  │
│        "你在看什么？"                    │
│                                         │
│  事件驱动 ──────────► 防抖 5s 后截图     │
│  （窗口切换 / 显著鼠标活动）              │
│                                         │
│  空闲兜底 ──────────► 每 5min 最多 1 张  │
│  （2min 无事件时激活）                   │
│        让猫猫偶尔"看一眼"                │
│                                         │
│  变化检测 ──────────► 感知哈希 >95% 相似  │
│  则跳过本次，不浪费存储和分析             │
│                                         │
└─────────────────────────────────────────┘
```

### 存储管理

```
~/.bitcat/screenshots/
├── 2026-05-10/
│   ├── 143022_primary.jpg        # 主屏截图
│   ├── 143022_all.jpg            # 双屏拼接图
│   ├── 143022_analysis.json      # AI 分析结果
│   └── 143022_hash.txt           # 感知哈希（用于去重）
├── 2026-05-11/
│   ...
├── screenshot.yml               # 截图配置（target/interval/quality 等）
└── .keep_days                   # 保留天数配置（默认7天）
```

文件命名规则：`{HHmmss}_{target}.jpg`，target = `primary` / `secondary` / `all`。

- FIFO 自动清理超期文件
- 总容量上限 ~500MB（桌面宠物不需要长期存档）
- 分析结果 JSON 同时写入 memory 系统（facts.md）

## 五、交互场景设计

### Level 1：被动观察（基础，优先实现）

| 触发 | 条件 | 猫猫行为 | 技术实现 |
|------|------|---------|---------|
| 空闲观察 | 2min 无操作 | 气泡显示屏幕描述 | 定时器→截图→vision API→bubble |
| 用户询问 | 手柄特定按键组合 | 回答"屏幕上是什么？" | 同上，prompt 不同 |
| 异常提醒 | 检测到红色 error 文字 | Confused 状态+"报错了？" | vision prompt 专注异常检测 |

### Level 2：主动建议（进阶）

| 场景 | 检测方法 | 猫猫反应 | 记忆联动 |
|------|---------|---------|---------|
| 编译失败 | OCR 检测 terminal 红色文字 | 变 Confused + 气泡提示 | 记录到 chat_summary.md |
| 长时间未操作 | 鼠标不动 + 屏幕不变 >30min | `"好无聊…"` + Sleep | — |
| 切换应用 | 窗口标题列表变化 | 评论新应用 | 写入 facts.md（"用户常用VSCode"） |
| 摸鱼检测 | 检测到视频/游戏窗口 | `"又在摸鱼 🐱"` | — |

### Level 3：视觉辅助（高级，可选）

| 能力 | 说明 | 实现复杂度 |
|------|------|-----------|
| OCR 读屏 | 提取屏幕所有可见文字 | 中（已有 vision prompt） |
| UI 定位 | "点保存按钮"→AI 返回坐标→模拟点击 | 高（需坐标校准） |
| 配色分析 | 分析页面配色方案 | 低（纯 prompt） |
| 无障碍模式 | 为视障用户朗读屏幕 | 中（TTS 接入） |

## 六、架构设计

### 模块划分

```
app/src/
└── screenshot.rs              # 截图模块（~200行）
    ├── capture_screen()       # BitBlt 截取桌面
    ├── encode_jpeg()          # 缩放 + JPEG 编码
    ├── perceptual_hash()      # 变化检测哈希
    ├── save_screenshot()      # 存储到 ~/.bitcat/screenshots/
    └── ScreenshotConfig       # 间隔/分辨率/质量配置

core/src/
└── vision.rs                  # 视觉分析模块
    ├── VisionAnalysis         # 强类型截图分析结果
    ├── VisionState            # 屏幕状态分类
    ├── analyze_screenshot()   # rig Extractor 图片输入 → 返回 VisionAnalysis
    └── wiremock tests         # Anthropic tool_use 协议回归
```

### 线程模型

```
main thread (gamepad_loop, 80ms tick)
  │
  │  启动时：
  │  std::thread::spawn(|| screenshot_loop())
  │
  ▼
screenshot_thread (独立线程，不阻塞手柄输入)
  │
  ├── 循环:
  │   ├── wait_next_trigger()    // 等待定时器/事件信号/用户请求
  │   ├── hide_overlay_windows() // 隐藏 pet/bubble
  │   ├── capture_screen()       // BitBlt
  │   ├── show_overlay_windows() // 恢复窗口
  │   ├── encode_jpeg(960x540)   // 压缩
  │   ├── if !similar_to_last(): // 感知哈希去重
  │   │   ├── save_screenshot()
  │   │   └── if analysis_enabled:
  │   │       └── vision.analyze_screen() → 结果
  │   │           ├── emit("pet-event", bubble_text)
  │   │           └── memory.add_fact(...)  // 写入记忆
  │   └── sleep_until_next()
  │
  └── 通过 channel/mutex 与主线程通信
```

### 与记忆系统的联动

```
截图分析 ──→ 提取事实/偏好 ──→ memory/facts.md      （知识输入）
截图分析 ──→ 活动摘要     ──→ memory/chat_summary.md （上下文丰富）
截图分析 ──→ 应用使用频率   ──→ memory/user_prefs.md   （习惯学习）
memory 系统 ──→ 用户偏好设置 ──→ 决定截图策略/频率      （策略控制）
```

## 七、隐私设计

参考 Microsoft Recall 事件后的行业共识：

| 措施 | 实现 |
|------|------|
| **默认关闭** | 功能需用户在 config/actions.yml 或面板中手动开启 |
| **可视化指示** | 截图时托盘图标变色 / pet 状态微变（如眨眼动画） |
| **排除列表** | 可配置不捕获的窗口（密码管理器、银行 App、无痕浏览器） |
| **本地优先** | 截图和分析默认本地完成，不上传云端 |
| **自动清理** | 7 天滚动删除旧截图，可配置 |
| **用户审计** | `~/.bitcat/screenshots/` 目录可直接浏览所有截图 |

## 八、代码量估算

| 类别 | 文件 | 行数 |
|------|------|------|
| 生产代码 | `app/src/screenshot.rs` | ~280（含多屏枚举/拼接/配置） |
| 生产代码 | `core/src/vision.rs` | ~180-260（B3 后） |
| 配置文件 | `screenshot.yml` | ~25 |
| 修改现有 | `screen_summary.rs` / 调用点（B3 强类型化） | ~200-360 |
| 修改现有 | `app/src/lib.rs`（启动 screenshot 线程） | ~20 |
| 修改现有 | `panel.rs`（截图设置面板 UI） | ~40 |
| 测试 | screenshot 多屏/编码/哈希/存储/配置 | ~140 |
| 测试 | vision / summary 请求构造与结构体解析 | ~150-280 |
| **合计** | | **~850-1100 行（含 B3 强类型化后估算）** |

## 九、演进路线图

```
Phase 1（当前目标）
  ├── BitBlt 截图 ✅
  ├── JPEG 压缩 + 感知哈希去重
  ├── 用户主动触发："看看屏幕"
  └── 基础观察：定时截图 → AI 描述 → 气泡显示

Phase 2
  ├── 事件驱动触发（窗口切换时截图）
  ├── 异常检测（error 文字识别）
  ├── 与记忆系统联动（facts.md 写入）
  └── 隐私控制面板（排除列表/开关）

Phase 3（远期）
  ├── UI 元素定位 + 自动点击
  ├── OCR 读屏功能
  ├── 多帧时序分析（理解操作序列）
  └── Screenpipe 风格的可回溯时间线
```

## 十、参考资源

- [Screenpipe (18.6k stars, Tauri+Rust 同栈)](https://github.com/mediar-ai/screenpipe) — 事件驱动截图的最佳架构参考
- [Microsoft OmniParser (微软开源 UI 元素检测)](https://github.com/microsoft/OmniParser) — 截图→UI元素定位
- [Anthropic Vision API 文档](https://docs.anthropic.com/en/docs/build-with-claude/vision) — 图片消息格式规范
- [Windows BitBlt 文档](https://learn.microsoft.com/en-us/windows/win32/gdi/bitblt) — GDI 截图 API
- [Microsoft Recall 架构分析](https://www.kaspersky.ru/blog/how-to-disable-copilot-recall-spyware/37769/) — 隐私设计教训
