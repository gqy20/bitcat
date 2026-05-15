# 音乐响应舞动调研与实现计划

日期：2026-05-15

## 背景

当前音乐舞动已经具备第一版可用链路：WASAPI loopback 或 fake source 产生音乐帧，后端通过 `performance-frame` 发送 `energy / bass / onset / silence`，前端 `MusicReactivePlayer` 以 sprite-only 方式驱动宠物动作。稳定性边界已经明确：音乐模式默认不移动真实桌面窗口，只在 canvas 内做表演，避免高频 `setPosition()` 压垮 WebView、设置页和右键菜单。

下一阶段目标不是继续加捕获能力，而是让宠物“更像在听音乐”：能区分静音、稳拍、强拍、高潮、回落，并用连续、可控的动作状态表达出来。

## 调研摘要

### 1. 实时 beat tracking 不应一开始追求完整 BPM

MIR 里的实时节拍跟踪通常围绕 onset detection function、局部脉冲、beat stability、inter-beat interval 等概念构建。较完整的算法可以做 beat lookahead、稳定性估计和 IBI 输出，但复杂度明显高于当前需求。对桌宠舞动来说，第一阶段更需要“稳定的重音触发”和“能量趋势”，而不是严格音乐学意义的 tempo。

工程取舍：

- 第一版保留 lightweight 分析：短窗口 RMS、能量包络、onset peak、低频近似、静音时长。
- 不急着引入完整 FFT beat tracker 或 ML 模型。
- 若后续要做 BPM/beat grid，再考虑 aubio/Essentia 思路或移植简化 beat tracker。

### 2. onset 是动作重音，不是动作状态本身

资料和实践都指向一个共同点：onset/beat 是瞬时事件，适合触发“点头、下压、弹起、眨眼、耳朵抖动”这类 accent，但不适合每帧直接切换主动作。若每次 onset 都重置 action，会导致动画抖动、断裂、难以停止。

工程取舍：

- `beat` / `onset` 只作为 accent 信号。
- 主动作由前端状态机控制，至少保持 300-800ms。
- energy 使用平滑值，beat 使用短 TTL pulse。

### 3. Web Audio 的可视化实践适合前端诊断，不适合当前主音频源

Web Audio `AnalyserNode` 提供实时 FFT 和 time-domain 数据，常用于浏览器内音频可视化。但本项目需要捕获电脑全局播放声音，主线仍应放在 WASAPI loopback。Web Audio 的经验可用于前端调试面板设计：频谱条、平滑常数、低频/高频分段、低频驱动粗动作，高频驱动细节动作。

工程取舍：

- 主捕获继续用 Rust WASAPI。
- 设置页可以借鉴 visualizer，展示 `energy / bass / treble / beat / silence`。
- 诊断刷新必须低频节流，避免 DOM 重绘影响停止入口。

### 4. WASAPI loopback 必须遵守 packet 读取和释放节奏

Microsoft 文档强调，`IAudioCaptureClient::GetBuffer` 和 `ReleaseBuffer` 需要成对调用，同一线程内按顺序处理；loopback 录制使用 render endpoint，把正在播放的音频复制到 capture buffer。当前实现已经遵循这个基本模型，但需要继续增强错误和状态处理。

工程取舍：

- 在 packet drain 内也检查 stop flag，避免停止后继续处理太久。
- 记录 `AUDCLNT_BUFFERFLAGS_SILENT`、device invalidated、resource invalidated 等状态，用于设置页错误提示。
- WASAPI thread 只负责分析和发送低频帧，不掺动画逻辑。

### 5. 动画层应以状态机承接音乐语义

游戏动画状态机的常见经验是：状态由稳定规则控制，动画只表现状态，不反过来主导业务。套到音乐舞动上，后端音频帧不应直接指定 `jump/shake/wave`，而应提供音乐语义，前端状态机再决定动作组合。

工程取舍：

- 后端输出“音乐特征”和少量语义字段。
- 前端维护 `IdleSilence / Groove / Bounce / Hype / Recover`。
- 状态切换有滞后、冷却、最短驻留时间，避免每帧乱跳。

## 推荐数据模型

后端 `MusicDanceFrame` 建议从当前字段扩展为：

```rust
pub struct MusicDanceFrame {
    pub session_id: u64,
    pub energy: f32,              // 平滑整体能量 0..1
    pub volume: f32,              // 当前短窗口 RMS 0..1
    pub bass: f32,                // 低频/低频近似能量 0..1
    pub treble: f32,              // 高频/瞬态近似能量 0..1
    pub onset: bool,              // 能量突增
    pub beat: bool,               // 更稳定的重音点
    pub silence: bool,
    pub silence_ms: u32,
    pub beat_interval_ms: Option<u32>,
}
```

前端可派生：

```js
{
  intensity: "low" | "mid" | "high",
  groove: "silent" | "steady" | "busy",
  phrase: "intro" | "active" | "peak" | "cooldown",
  accent: 0.0-1.0
}
```

## 前端状态机草案

### IdleSilence

进入条件：

- `silence === true` 持续超过 600ms
- 或 `energy < silenceThreshold`

表现：

- 使用 idle/呼吸。
- 不响应普通 onset，只等待能量恢复。

### Groove

进入条件：

- 有稳定中低能量。
- `beat` 偶尔出现，但未达到高能量。

表现：

- `wave` 为主。
- 每个 beat 给 `offsetY` 或 squash 一个短 pulse。
- 动作连续，避免频繁换 action。

### Bounce

进入条件：

- `bass` 或 `beat` 明显。
- energy 中等。

表现：

- 用短跳/下压表达低频。
- beat 触发 accent，不重置整个动作。

### Hype

进入条件：

- `energy` 高于阈值一段时间。
- 或连续强 onset。

表现：

- `shake` / `jump` 交替，但每个动作有最短持续时间。
- 高能量时增加表情和粒子，而不是扩大真实窗口移动。

### Recover

进入条件：

- 从 Hype 掉到中低能量。

表现：

- 动作收敛到 `wave`。
- 防止高能段结束后立刻静止。

## Fake Source 升级计划

为调舞感，fake source 应从单一波形扩展为多模式：

| 模式 | 用途 |
|------|------|
| `steady` | 稳定四拍，用于测试 Groove/Bounce |
| `busy` | 高频 onset，用于测试防抖和最短状态驻留 |
| `hype` | 高能量段，用于测试 Hype/Recover |
| `silence` | 静音/恢复，用于测试 IdleSilence |

设置页可以先做模式按钮或 select，不急着落配置文件。

## 调参入口

建议设置页先加运行期参数：

| 参数 | 默认值 | 说明 |
|------|------:|------|
| `sensitivity` | 1.0 | 能量整体增益 |
| `beatThreshold` | 0.16 | onset/beat 判定阈值 |
| `silenceThreshold` | 0.008 | 静音阈值 |
| `style` | `cute` | `subtle` / `cute` / `energetic` |
| `spriteMotion` | 1.0 | canvas 内动作幅度 |
| `windowMotion` | `off` | 未来可选，默认关闭 |

所有参数第一版可以仅存在前端内存，等体验稳定后再进入 settings 配置。

## 分阶段路线

### Phase 1：舞感状态机

- 扩展 `MusicReactivePlayer` 为状态机。
- 保持 sprite-only。
- fake source 增加 `steady / busy / hype / silence`。
- 设置页显示当前 music state、beat、silence_ms。

验收：

- fake steady 下动作稳定连续。
- fake busy 下不会抖成一团。
- silence 进入 idle 不超过 1s。

### Phase 2：后端特征增强

- `MusicDanceFrame` 增加 `volume / treble / beat / silence_ms / beat_interval_ms`。
- WASAPI loop 内加入 envelope 和 beat interval 估计。
- packet drain 内检查 stop flag。
- 记录可恢复错误并发 `performance-error`。

验收：

- 真实播放音乐时 `beat` 不必完美，但不能密集乱闪。
- stop 能在 200ms 级别进入停止状态。

### Phase 3：调参和正式入口

- 设置页加入 sensitivity/style/fake mode。
- 提供正式“开始音乐舞动”入口。
- 文档更新用户操作说明。

验收：

- 普通用户无需看日志即可启动/停止。
- 设置页能解释当前状态和错误。

### Phase 4：可选窗口摆动

只有在 sprite-only 稳定后再做。

约束：

- 默认关闭。
- 低频 4-8fps。
- pending guard，未完成移动不排队。
- 位移小于 10-20px。
- 托盘停止入口保留。

## 参考资料

- Microsoft Learn: [IAudioCaptureClient::GetBuffer](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer)
- Microsoft Learn: [AUDCLNT_STREAMFLAGS loopback recording](https://learn.microsoft.com/en-us/previous-versions/aa363088%28v%3Dvs.85%29)
- windows-rs: [IAudioCaptureClient](https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Media/Audio/struct.IAudioCaptureClient.html)
- MDN: [AnalyserNode](https://devdoc.net/web/developer.mozilla.org/en-US/docs/Web/API/AnalyserNode.html)
- MDN: [Visualizations with Web Audio API](https://devdoc.net/web/developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API/Visualizations_with_Web_Audio_API.html)
- aubio: [documentation](https://aubio.org/documentation)
- TISMIR: [A Real-Time Beat Tracking System with Zero Latency and Enhanced Controllability](https://transactions.ismir.net/articles/10.5334/tismir.189?_rsc=1tt34)
- arXiv: [OBTAIN: Real-Time Beat Tracking in Audio Signals](https://arxiv.org/abs/1704.02216)
- Defold: [Animation State Machine example](https://defold.com/examples/animation/animation_states)
