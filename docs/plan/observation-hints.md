# Observation Hints：截图分析的受限背景注入计划

> 状态：设计草案，未开始实现。  
> 目标：让截图观察更懂当前工作背景，同时保持 Vision 层的证据边界，避免记忆污染截图事实。

## 背景

当前截图分析与记忆已经在 **对话上下文层** 整合：

- `vision::analyze_screenshot()` 负责单帧截图结构化分析。
- `screenshot::build_recent_analyses_context()` 把最近截图分析注入聊天上下文。
- `screen_summary::generate_summary()` 定期聚合截图分析，并由 `ScreenSummaryStore::build_context()` 注入聊天上下文。
- `gamepad.rs` 在用户对话前拼接用户画像、自动画像、长期记忆、短期记忆、最近截图和屏幕摘要。

这个设计的优点是边界清晰：Vision 只描述画面，对话 Agent 再结合记忆推理。缺点是截图分析本身比较“失忆”，对项目背景、用户习惯和近期活动趋势缺乏感知，容易输出泛泛描述。

本计划引入一个中间层：**Observation Hints（观察提示）**。它不是完整记忆注入，而是经过白名单过滤、低权重、可审计的背景提示。

## 设计原则

1. **视觉事实优先**

   `confirmed_text`、应用名称、文件名、URL、命令、歌名、数字等只能来自截图可见证据，不能由观察提示补全。

2. **背景只能辅助活动推断**

   Observation Hints 只允许帮助 `inferred_activity`、活动类别和描述语气，不允许变成确定事实。

3. **来源可解释**

   每条 hint 都带来源类型，例如 `explicit_profile`、`app_state`、`screen_trend`、`long_term_profile`。

4. **默认低风险**

   第一版不接入自由检索的短期对话记忆，也不直接注入长期记忆全文。只注入用户显式画像、应用状态和屏幕摘要趋势。

5. **可关闭、可审计**

   观察提示应可通过配置或设置页关闭；调试日志只记录 hint 来源和字符数，避免把完整提示写入普通日志。

## 非目标

- 不让 Vision 根据记忆确认具体画面文字。
- 不把 `MemoryStore.build_context()` 直接塞进 Vision prompt。
- 不引入 embeddings、向量检索或不可解释 RAG。
- 不让截图分析自动写入长期记忆。
- 不改变现有 `recent_screenshots` 工具的职责。

## 数据来源

### P0：低风险来源

1. **用户显式画像**

   来源：`config/user.yml` / `UserProfile::build_context()` 的结构化字段。

   可注入内容：

   - 用户角色或工作类型，例如“开发者”。
   - 语言偏好，例如“中文简洁回答”。
   - 用户显式声明的长期偏好。

   禁止注入内容：

   - 隐私性强的身份细节。
   - 不必要的个人资料。
   - 历史聊天里模型自动推断出的画像。

2. **应用运行状态**

   来源：app 层已经掌握的本地状态。

   可注入内容：

   - 当前处于手动截图还是后台观察。
   - 是否处于聊天、游戏、录音等模式。
   - 当前项目/应用的固定产品背景，例如“BitCat 桌面 AI 伙伴”。

   禁止注入内容：

   - 剪贴板内容。
   - shell 输出。
   - 当前未授权的窗口/进程枚举。

3. **近期屏幕活动趋势**

   来源：`ScreenSummaryStore` 的结构化摘要，但需要再次压缩。

   可注入内容：

   - 最近活动类别，例如 coding、browsing、documents。
   - 粗粒度趋势，例如“最近多次看到代码编辑器和终端”。
   - 时间范围，例如“过去 15 分钟”。

   禁止注入内容：

   - 具体文件名。
   - 具体命令。
   - URL。
   - 歌名、歌手、播放量等媒体元数据。
   - `uncertain_text` 里的猜测。

### P1：谨慎来源

4. **长期记忆白名单检索**

   来源：`LongTermMemory`，但不直接用 `retrieve()` 的原始文本。

   只允许注入：

   - 用户明确确认的项目背景。
   - 技术栈偏好。
   - 长期工作方式偏好。

   需要新增过滤规则：

   - 仅接受 `source` 为用户明确输入或人工确认的条目。
   - 仅接受 tags 命中 `project`、`tech_stack`、`preference`、`workflow` 的条目。
   - 丢弃包含路径、URL、命令、账号、聊天内容、媒体标题的条目。

第一版建议先不做 P1，等 P0 的效果和误判率稳定后再接。

## Prompt 形态

Observation Hints 应作为 Vision prompt 的附加段落，而不是混进原始 `vision.prompt` 正文：

```text
[观察提示]
以下背景只用于理解用户活动类别，不得用于确认画面文字、文件名、应用名、URL、歌名、命令或数字。
如果截图证据与观察提示冲突，必须相信截图证据。
- explicit_profile: 用户偏好中文简洁回答。
- app_state: 这是一次手动截图观察。
- screen_trend: 最近 15 分钟活动主要偏 coding，偶尔使用终端。
[/观察提示]
```

Vision 输出规则需要补充：

- `confirmed_text` 只能来自截图。
- `uncertain_text` 只能来自截图里看起来像但无法确认的文本。
- `inferred_activity` 可以参考观察提示，但必须保持低确定性表达。
- `description` 可以轻微利用活动趋势，但不能新增具体事实。
- 如果使用了观察提示辅助推断，`risk_flags` 可增加 `context_used`。

## 数据结构草案

```rust
pub struct ObservationHints {
    pub items: Vec<ObservationHint>,
}

pub struct ObservationHint {
    pub source: ObservationHintSource,
    pub text: String,
}

pub enum ObservationHintSource {
    ExplicitProfile,
    AppState,
    ScreenTrend,
    LongTermProfile,
}
```

文本构建约束：

- 总长度默认不超过 500 字。
- 单条 hint 不超过 80 字。
- 默认最多 6 条。
- 构建时执行敏感模式过滤：路径、URL、命令行、邮箱、长数字串、明显媒体元数据。
- 过滤命中时整条丢弃，不做局部替换，避免制造新的模糊事实。

## 实现路径

### Phase 1：P0 Observation Hints 构建器

新增 core 模块：

- `core/src/observation_hints.rs`

职责：

- 定义 `ObservationHints` / `ObservationHint` / `ObservationHintSource`。
- 从用户显式画像、app 状态、屏幕摘要趋势构建 hints。
- 提供 `to_prompt_section()`。
- 提供敏感内容过滤 helper。

预期 API：

```rust
pub fn build_observation_hints(input: ObservationHintInput) -> ObservationHints;

pub struct ObservationHintInput<'a> {
    pub explicit_profile_context: Option<&'a str>,
    pub app_state: Option<&'a str>,
    pub screen_summary_context: Option<&'a str>,
}
```

### Phase 2：Vision 调用接入

修改：

- `core/src/vision.rs`
- `app/src/screenshot.rs`
- `core/src/prompts.rs`
- `config/prompts.yml`

方案：

- `vision::analyze_screenshot()` 增加可选 `observation_hints: Option<&ObservationHints>` 参数，或新增 `VisionRequestContext` 结构体承载 prompt 配置和 hints。
- app 截图路径在调用 Vision 前构建 hints。
- 将 hints 追加到 Vision preamble 末尾。
- 配置增加开关：

```yaml
vision:
  observation_hints_enabled: true
  observation_hints_max_chars: 500
```

兼容策略：

- 默认启用 P0 hints，但如果配置缺失，使用默认值。
- hints 构建失败时静默降级为空，不影响截图分析。

### Phase 3：结构化输出标记

修改：

- `core/src/vision.rs`

方案：

- `risk_flags` 增加 `context_used` 可选值。
- 或新增字段 `context_used: bool`。

倾向方案：先扩展 `risk_flags`，避免改动太大；如果后续 UI 需要明确展示，再升级为独立字段。

### Phase 4：长期记忆白名单接入

前置条件：

- P0 已稳定。
- 有截图分析误判样本对照。
- 设置页提供关闭开关。

方案：

- 新增 `LongTermMemory::observation_profile_hints()` 或独立过滤函数。
- 只取明确 tag/source 的低风险条目。
- 记录 hint 来源为 `long_term_profile`。

## 测试计划

### 单元测试

`core/src/observation_hints.rs`：

- 空输入返回空 prompt。
- 用户画像可生成低风险 hint。
- screen summary 只保留活动类别，不保留具体文件名/URL/命令。
- 超长输入按字符数截断，中文不按字节切。
- 敏感内容命中时整条丢弃。

`core/src/vision.rs`：

- hints disabled 时 prompt 与旧行为一致。
- hints enabled 时 preamble 包含观察提示边界规则。
- 多显示器 prompt 与 hints 可同时存在。

### 快照测试

适合用 `insta` 覆盖：

- `ObservationHints::to_prompt_section()`。
- Vision prompt 组合结果。
- 带 `context_used` 的 `VisionAnalysis` 序列化。

### 回归样例

需要准备几类截图：

- 模糊代码编辑器：不能因项目背景确认具体文件名。
- 音乐播放器：不能因历史偏好确认歌名/歌手。
- 浏览器文档页：可以推断“浏览技术文档”，但 URL 只能来自画面。
- 终端小字：不能根据近期 coding 趋势补全命令。

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| Vision 把背景当事实 | prompt 明确禁止，字段级约束，测试覆盖模糊文本场景 |
| 长期记忆污染截图分析 | P0 不接长期记忆；P1 只做白名单过滤 |
| token 成本增加 | hints 默认 500 字以内，日志跟踪 Vision token 变化 |
| 隐私观感变差 | 设置页可关闭；提示来源可审计；不注入敏感细节 |
| 旧截图/摘要格式不兼容 | 不改截图记录主结构，先只改 prompt 输入 |

## 验收标准

- Vision 分析仍能在无 hints 时正常工作。
- 开启 hints 后，截图描述能更稳定地区分“写代码 / 浏览文档 / 看媒体 / 通讯”等活动类别。
- 模糊文字、媒体元数据、命令和 URL 不会因为 hints 被写入 `confirmed_text` 或确定性 description。
- 聊天上下文仍保留现有记忆整合路径，不被 Observation Hints 替代。
- 用户可以通过配置关闭 Observation Hints。

## 推荐实施顺序

1. 先实现 `ObservationHints` 数据结构和纯函数构建器。
2. 用单元测试锁住过滤规则和 prompt 输出。
3. 接入 Vision 调用，但默认只使用 P0 来源。
4. 实测几天截图误判率和 token 增量。
5. 再决定是否接入长期记忆白名单。
