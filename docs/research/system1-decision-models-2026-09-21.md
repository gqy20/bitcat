# System 1 决策模型调研：Laya / Jev 生态与本地化可行性

调研日期：2026-09-21。基于 GitHub 仓库 README 与各项目自测 benchmark（gh CLI + 网页抓取），未做实机验证。本文所有 benchmark 数字均为**项目作者自测**，无第三方复现；引用时注意水分。来源事实与针对 BitCat 的设计判断分开标注。

## TL;DR

1. **"本地小决策模型"（System 1 / typed decisions）这个品类已经成型**：非自回归编码器、单次前向、输出 choice/score/noul 三种带校准置信度的类型化答案，151M–421M 参数。
2. **不存在原生多模态版本**（图像直接进、typed decision 出）。两个生态各自给出了替代答案：CLIP/SigLIP 双塔 = 图像版 `choice`；Jev 生态 = "感知→符号状态→文本决策模型"分层（jev-drone 范式）。
3. **效果分层明确**：easy/standard 级小判断（路由、二分）77–87% 可用；hard 级多跳推理 36.9% 不可用，闭源 Jev 在通用套件上 96.6% 仍有碾压优势。
4. **本地可行**：151M 级 int8 ONNX ≈ +150MB 内存、无 GPU、CPU 20–200ms。BitCat 决策频率（对话结束 1 次 / 截图 30s 1 次）对此完全无感。
5. **对 BitCat 的建议**：现在不做任何集成。9 条业务题初筛实测（见第六节）67–78% 未过 85% 线，且最想要的截图 gate 用途无区分度；`score` 原语（nudge/紧急度）和弃权机制表现合格。完整测评（50–100 条日志构造题）前不谈 ONNX 集成（`ort` crate / `usls` 路径已验证可行）。

---

## 一、品类：什么是 System 1 决策模型

代表项目 [NandhaKishorM/laya](https://github.com/NandhaKishorM/laya)（Apache 2.0，Convai Innovations）：

- 非自回归编码器（ModernBERT-large 421M / mmBERT-base 322M），对任意文本状态回答**类型化问题**，单次前向（T4 上 33ms），无文本生成、无 schema 解析、无幻觉。
- 三原语：`choice`（选标签+概率）、`score`（等级打分）、`noul`（校准 P(true)）。
- 用 RLCD（严格 proper scoring rule 的 RL）训练，置信度有统计意义，可做 `conf >= 0.85 自动路由，否则升级人工/大模型` 的门控。
- **诚实声明**：基座零样本接近随机（typed-decisions 上 0.36 vs 随机 0.32），价值全在微调。"是快速底座，不是零样本决策引擎。"

闭源对标物是 TypeSafe 的 [Jev](https://typesafe.ai)（System One，closed API，$0.042/1M tokens），在通用套件上准确率显著领先所有开源复刻。

## 二、多模态问题：直接对应物不存在（关键结论）

GitHub 搜索 `non-autoregressive multimodal decision` / `typed decisions vision` / `model router vision language` 全部为空。这是架构原因，不是市场空白没被发现：

- Laya = 文本编码器 + RLCD 校准分类头。视觉域对应生态位**已被 CLIP/SigLIP 双塔占据**：图像嵌入 × 文本标签余弦相似度 = zero-shot `choice`，单次前向、无生成。缺的只是校准置信度和 score/noul 原语。
- Jev 官方明确不是视觉模型。生态处理视觉/音频的方式高度一致：**感知与决策分离**。

### BitCat 管道的正确切分

```
现在:  BitBlt → dHash 去重 → Vision API → 文本 → 主 LLM（决策+生成全包）
应有:  BitBlt → dHash 去重 → [感知→紧凑场景 JSON] → [本地小决策模型] → 低置信才升级主 LLM
```

Laya 类模型只能站在"文本已存在"的半段：

| 决策点 | 输入 | Laya 类可用 |
| --- | --- | --- |
| 截图值不值得调 Vision | 像素 | ✗（除非先花一次 Vision 调用生成文本，门卫失去意义） |
| 摄像头帧 gating | 像素 | ✗ |
| 屏幕摘要注入前：有价值吗/有注入吗 | Vision 产出文本 | ✓（省 context 与安全，不省钱） |
| 成本路由：闲聊 vs 复杂任务 | 用户消息 | ✓ |
| 对话结束 mood 分类 | 对话文本 | ✓ |
| memory_candidate 重要度 | 文本 | ✓ |
| Agent Watch nudge 决策 | 会话事件 | ✓ |

## 三、Jev 生态调研：jev-drone 分层范式

最佳标本 [RomanSlack/jev-drone](https://github.com/RomanSlack/jev-drone)（101★）：纯摄像头无人机在 MuJoCo 过障碍，Jev 只做战术判断。

```
 500 Hz   几何控制器          普通代码（安全永远归代码）
 50 Hz   安全反射            普通代码
 15 Hz   相机 → 符号场景      经典 CV（深度+分割）压成 5 个距离扇区等紧凑 JSON
~2.5 Hz   战术判断            Jev 读场景 JSON，一次调用出 choice/score/noul
```

与 BitCat 惊人同构的两个细节：

- **场景指纹化，未变化的情境复用上次判断** ≈ BitCat 的 dHash 去重。
- **代码决定什么时候问**：开阔走廊目标在视野内就不调用（65s 飞行仅 ~110 次调用）≈ BitCat 的业务避让。

生态内多模态相关项目（无一例外都是"感知前置"）：

| 项目 | ★ | 多模态方式 |
| --- | --- | --- |
| RomanSlack/jev-drone | 101 | 经典 CV → 符号场景 → Jev |
| fhshaik/typesafe-mario | 319 | 模拟器结构化状态 → Jev（非像素） |
| Friedjof/jev-mobile | 小 | Android accessibility → Jev 逐步决策 |
| santos-sanz/jev-audio-beeper | 2 | 音频 → ffmpeg → Jev |

可自托管的开源 Jev 替代（均为文本输入）：

| 项目 | ★ | 路线 | 大小 |
| --- | --- | --- | --- |
| [TheoLeeCJ/SemIf](https://github.com/TheoLeeCJ/SemIf)（原 OpenJev） | 2.8k | 生成式模型直接读 option logprob，无解码循环；浏览器 WebGPU 可跑 | 2B–4B |
| [jaredpalmer/kev](https://github.com/jaredpalmer/kev) | 1.8k | Qwen3.5 基座，API 兼容 `/v1/systemone` | 0.8B/4B/9B |
| [TianyuCodings/NanoJev](https://github.com/TianyuCodings/NanoJev) | 1.7k | Qwen3-0.6B + 决策头，专训后极强（ViZDoom Basic 128/128 vs Jev 56/128） | 0.6B |
| [wfzyx/von](https://github.com/wfzyx/von) | 312 | 真非自回归编码器，sub-25ms，协议兼容 Jev | 395M |
| [Heman10x-NGU/openJev-verdict-2.0](https://github.com/Heman10x-NGU/openJev-verdict-2.0) | 231 | ModernBERT-base + GLiClass，RLCD | 151M |

SemIf 的技巧值得记住：**拿生成式模型的 option token 概率当分类器，零训练零样本**。若换 Qwen-VL 基座做同样的事即"多模态 typed decisions"——目前 GitHub 上无人做，是空位。

## 四、视觉入口：CLIP 路线

- [apple/ml-mobileclip](https://github.com/apple-aiml-research/ml-mobileclip)（1.7k★，CVPR'24 + TMLR'25）：S0 ≈ ViT-B/16 精度但 4.8× 快 / 2.8× 小，iPhone 实时 zero-shot 分类。图像塔几十 MB。
- [jamjamjon/usls](https://github.com/jamjamjon/usls)（443★）：**Rust** + ONNX Runtime 的 CV/VLM 库，50+ 模型（<1B），API 有 `encode_images()`/`encode_texts()`，Windows 支持 DirectML，自动下载缓存。是 BitCat 集成 CLIP 类模型的现成路径，无需 Python sidecar。

对 BitCat 的两个用途：**语义去重**（dHash 只见像素差异，CLIP 嵌入余弦可见"窗口换了但内容同类"）与**截图 gate**（嵌入 vs 固定标签，低置信照旧走 Vision API，失败模式良性）。注意 CLIP 置信度无校准，不能照搬 `conf >= 0.85` 门控姿势。

## 五、效果与设备要求（均为作者自测数字）

| 模型 | 大小 | 准确率 | 校准 | 延迟 |
| --- | --- | --- | --- | --- |
| openJev-verdict-2.0 | 151M | typed-decisions 77.1%（Laya 76.6%，Jev 72.7%） | ECE 1.44%（最优） | ~20–25ms/决策，浏览器 WebGPU 可跑 |
| von | 395M | jabr v2 71.5%（Jev 闭源 96.6%）；choice 83.4% | Brier 好 | GPU ~18ms |
| kev-4B | 4B | 新领域 79% | Brier 0.3+（差） | 需 32GB Mac / GPU |
| Laya | 421M | typed-decisions 76.6%（微调后） | ECE 0.081（温度拟合后） | T4 33ms / **CPU 193–464ms** |

verdict-2.0 按难度分层：**Easy 87.5% / Standard 69.4% / Hard 36.9%**——这是最重要的数字：小判断可用，多跳推理不可用。

设备门槛分层：

| 档位 | 能跑什么 | 内存增量 | 无 GPU 延迟 |
| --- | --- | --- | --- |
| 任何 8GB 现代 PC | verdict 151M int8、MobileCLIP-S0 | +150–300MB | 20–200ms |
| 16GB / NPU | von 395M、MobileCLIP-S2/B | +0.5–1.6GB | 10–50ms |
| 32GB Mac / 独显 | kev-4B/9B、SemIf-4B | +8GB+ | 对桌面宠物不现实 |

关键判断：非自回归编码器无 KV cache、内存静态、延迟可预测，天生适合本地；BitCat 决策频率极低（对话结束 1 次 / 截图 30s 1 次），200ms CPU 延迟完全无感。**真正约束只有内存增量（150MB 可接受，1.5GB 要过恐慌测试）和 Windows 集成路径（ONNX + `ort`，`usls` 已验证）。**

## 六、本地实测：openJev-verdict-2.0（151M，2026-09-21）

环境：Linux 无 GPU、Python 3.12 + torch 2.14 CPU、模型 605MB（fp32 safetensors）。repo 的 HF 权重自带 `model_fp16.onnx`（303MB）和 `calibrator.json`（T=2.80）。引擎 `DecisionEngine` 单次前向，CPU 实测延迟 **59–160ms/决策**（README 的 20–25ms 是 GPU 数字）。

用 9 条 BitCat 业务题（mood 分类 / 截图 gate / nudge 紧急度 / Agent Watch 通知时机 / 中文对照 / OOD）测试，结果：

| 用例 | 预期 | 实际 | 判定 |
| --- | --- | --- | --- |
| mood：焦虑→鼓励道谢 | supportive | supportive 0.535 | ✓ |
| mood：开玩笑逗宠物 | happy/playful | happy 0.377 + playful 0.303 | ✓（概率分散） |
| 截图 gate：40min 静止文档 | **false** | **true 0.883** | ✗ |
| 截图 gate：出现编译错误 | true | true 0.878 | ✓，但见下 |
| nudge：agent 正常运行 | none | none 0.574，期望分 0.67 | ✓ |
| nudge：失败且等待输入 | soon+ | soon 0.324，期望分 1.49 | ✓ |
| 通知时机：完成+用户看视频 | wait | wait 0.683 | ✓ |
| 中文同题 mood | 关心 | 关心 0.373 vs 担心 0.325 | △ 平手，不可依赖 |
| OOD：日食描述 | 弃权 | `__insufficient_evidence__` 0.479，is_abstention=true | ✓✓ |

**关键发现：**

1. **`score` 原语区分度最好**：正常运行 vs 失败等待，期望分 0.67 → 1.49、none 概率 0.574 → 0.245，方向和幅度都对。nudge/紧急度类决策可用。
2. **`noul` 在截图 gate 上无区分度**：静止文档（应 false）和编译错误（应 true）的概率几乎相同（0.883 vs 0.878）——模型疑似只对"存在屏幕摘要文本"起反应，没读语义。**BitCat 最想要的截图 gate 用途实测不及格。**
3. **弃权机制真实可用**：OOD 输入最高概率给了 `__insufficient_evidence__` 且 is_abstention=true，校准名不虚传，升级架构安全。
4. **中文不可依赖**：选对但相邻选项平手（concentration 0.125）。若集成，BitCat 的中文场景需把 context 翻译成英文再喂（或换多语基座重训）。
5. **置信度普遍偏低**（0.37–0.68），按 `conf >= 0.85` 门控大多数会升级到云 LLM——保守安全，但也意味着自动命中率有限。

按第六节 85% 准入线评估：方向正确 6–7/9 ≈ **67–78%，未过线**。截图 gate（最核心用途）明确不及格。结论维持"现在不集成"；若未来重试，优先测 `score` 类决策（nudge/紧急度），放弃 noul 做语义 gate，中文需英文化预处理。

测试脚本与权重保留在 `/tmp/openjev-verdict/`（bitcat_test.py，重启后丢失可从本文档复现）。

### 6.1 对照实测：Laya-multilingual（322M，同日）

同批用例跑 Laya multilingual checkpoint（`laya.load(subfolder="multilingual")`，CPU 73–112ms，与 verdict 同量级）：

| 用例 | verdict-2.0 | Laya-multilingual | 谁好 |
| --- | --- | --- | --- |
| mood 焦虑→鼓励 | supportive 0.535 ✓ | supportive 0.653 ✓ | Laya 略集中 |
| mood 玩笑 | happy 0.38/playful 0.30 拿不准 ✓ | **playful 0.766 ✓** | Laya |
| nudge 正常运行 | **none 0.574 ✓** | gentle-check-in 0.598，且 P(interrupt)=0.23 △ | **verdict** |
| nudge 失败等待 | 期望分 0.67→**1.49** 区分度 0.82 ✓ | 1.36→1.47 区分度 0.11 △ | **verdict** |
| 通知时机 | wait 0.683 ✓ | wait 0.485，**confidence 仅 0.054** ✓ | verdict |
| **中文 mood（应=关心）** | 平手 0.373/0.325 但**选对** | 开心 0.433 / 关心 0.303——**选错** ✗ | verdict |
| 中文 nudge | （未测） | score 1.42 应≈2，P(soon)=0.11 △ | — |
| OOD 日食 | **显式弃权** 0.479 ✓✓ | 无弃权机制，硬选 happy、confidence 0.12 △ | **verdict** |

**结论反转**：调研阶段判断"Laya multilingual 的中文支持是相对 verdict 的核心优势"——实测不成立。中文情绪细粒度任务上 Laya argmax 选错（开心 vs 关心），仅概率分布略有区分。加上三个结构性劣势：multilingual 版**未带 fitted temperatures**（官方 README 明说 ECE 0.314，需自己拟合校准）、**无弃权机制**（OOD 硬选）、score 区分度几乎为零——**verdict-2.0 在 BitCat 业务题上全面占优**。

Laya 剩余价值：官方微调 notebook（RLCD 数据构建→训练→校准→推 Hub 一条龙，Kaggle 免费 2×T4）比 verdict 的训练代码完整，若走到微调阶段可借用其训练管线。

## 七、对 BitCat 的建议（设计判断）

### 7.1 微调的作用边界（先说清不能做什么）

- **微调不产生新能力**：verdict-2.0 是纯文本编码器（ModernBERT），没有视觉通道，微调多少数据也长不出看图能力。图片永远在管道上游先被感知层（dHash / CLIP / Vision API）转成文本/符号。
- **微调的本质是教师蒸馏**：拿云 LLM 历史输出的"输入→判断"对教会本地 151M 模仿，替换例行调用——省钱 + 零延迟 + 离线可用，但判断内容和云 LLM 一样，不是新智能。
- **蒸馏规则毫无意义**：`agent_watch_nudges.jsonl` 有 7042 行，但那是 Rust 规则的输出——让模型学习模仿规则，不如规则本身（0ms、100% 复现）。微调标签只有三个来源：蒸馏规则（无意义）、教师蒸馏（✅ 唯一成立）、用户反馈（稀疏，短期不可指望）。

### 7.2 分阶段路线与触发条件

```
阶段 0（现在，唯一排期项）  AgentReaction 输入+输出 JSONL 落盘
阶段 1（攒够数千条）        日志构造测评集，只测 score 类 → 85% 线
阶段 2（过线后）            model_fp16.onnx (303MB) + ort crate 集成
阶段 3（阶段 1 不过线时）    教师蒸馏微调（Laya 官方 RLCD notebook 管线，租 GPU 一次性）
```

- **阶段 0 的双重价值都真实**：调试上，"它刚才为什么是这个心情"现在查不到（mood 决策只在内存 ring buffer 留 50 条，不落盘，标签数据每天在流失）；语料上，它是教师蒸馏唯一成立的标签源。实现量级：照 `reminder_events.jsonl` 模式加一个 writer，遵循日志规范（`*_chars`/`*_preview`）。
- **候选模型定为 verdict-2.0**（6.1 节对照后 Laya 出局）。中文问题的解法不是换多语言模型，而是微调时把中文对话数据混进训练集（BPE 词表骨架可训出中文能力，verdict 零样本平手 → 微调后理应拉开）。
- **集成形态**（若触发）：151M 级 ONNX + `ort`（或经 `usls`），默认进专家模式，显式开关 + 一句人话解释（"让宠物有一点下意识反应，不用每次都问大脑"）。
- **与项目规范的冲突要正视**：CLAUDE.md 明令"不要在模型前放小分类器做意图理解"。本路线做的是**对话收尾判断的本地化**（替换 Extractor 例行调用），不是意图理解前置分类器，是刻意例外；若走到上线，必须先更新 `docs/architecture/design-tradeoffs.md` 论证收益大于复杂度。
- **架构上的零依赖改进**：把 mood_policy / nudge 的决策语义统一成"类型化问题 + 置信度 + 低置信升级"的形式，今天由规则实现，未来可换本地模型，接口不变。

## 来源

- https://github.com/NandhaKishorM/laya （及 BENCHMARKS.md 章节）
- https://github.com/RomanSlack/jev-drone
- https://github.com/Heman10x-NGU/openJev-verdict-2.0
- https://github.com/wfzyx/von
- https://github.com/jaredpalmer/kev
- https://github.com/TianyuCodings/NanoJev
- https://github.com/TheoLeeCJ/SemIf
- https://github.com/apple-aiml-research/ml-mobileclip
- https://github.com/jamjamjon/usls
- https://github.com/AbdelStark/awesome-typesafe-jev
