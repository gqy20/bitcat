# BitCat 对话界面设计调研

日期：2026-10-02。目标：学习桌面浮动对话的形态、阅读层级与交互节奏，为下一轮设计提供依据。当前阶段以日常陪伴体验为主，Steam 发布暂缓。

本轮使用官方说明、官方示例图、现有代码和本地静态渲染。没有登录或安装这些产品的原生客户端；跨平台焦点、置顶、IME 和窗口跟随行为仍需要 Windows 实机验证。下文将来源事实、视觉观察与 BitCat 的设计假设分别说明。

**本轮结论**

对话界面的优雅感来自四件事：内容有层级，输入位置稳定，窗口在桌面上的关系清楚，操作能连续完成。颜色、圆角和阴影应服务于这些关系。

BitCat 值得验证三个使用状态：猫的短回应、用户主动聊天、用户专注阅读。输入常驻和历史可见主要服务主动聊天；短回应保持轻巧；专注阅读允许固定位置。它们可以复用当前窗口和会话数据，不必对应三个独立窗口。

此前“把所有内容都放进一个展开式会话面板”的建议需要细化：展开面板适合明确开始的对话；日常陪伴中短暂的一句话仍然需要与猫保持直接、轻量的关系。

**取样与证据**

| 样本 | 已核对的来源事实 | 本轮证据范围 |
| --- | --- | --- |
| [Raycast Quick AI](https://manual.raycast.com/ai/quick-ai) | 小窗口可追问，能将完整会话与附件转入 AI Chat；长用户消息可折叠 | 官方文字说明、快速问答和折叠消息示例图 |
| [Raycast AI Chat](https://manual.raycast.com/ai/ai-chat) | 独立会话窗口，支持历史、消息编辑、换行和流式回复 | 官方文字说明、包含设置侧栏的会话示例图 |
| [Claude Mac quick entry](https://support.claude.com/en/articles/12626668-use-quick-entry-with-claude-desktop-on-mac) | 快速输入框可发起聊天、查看最近对话，并显式附加截图或窗口内容 | 官方交互说明；未取得完整原生界面连续截图 |
| [Notion AI 官方指南](https://www.notion.com/en-gb/help/guides/everything-you-can-do-with-notion-ai?nxtPslug=everything-you-can-do-with-notion-ai) | 有轻入口、完整视图、历史入口和输入引导 | 官方空状态与覆盖层示例图；该图是指南中的版本示例，不视为所有当前客户端的实机状态 |
| [Copilot Quick View 历史示例](https://blogs.windows.com/windows-insider/2024/12/10/update-for-copilot-for-windows-begins-rolling-out-to-windows-insiders/) | 2024 年官方说明支持移动、调整大小和转入主窗口 | 历史行为说明；图片返回 403，未据此作视觉评价 |
| [Microsoft 365 Copilot 当前入口说明](https://support.microsoft.com/en-us/microsoft-365-copilot/access-microsoft-365-copilot-on-windows) | 可配置快捷键打开完整窗口或 quick view | 工作/学校账号产品的入口说明，不与消费版历史示例混为同一版本结论 |
| [OpenAI ChatKit 定制规范](https://developers.openai.com/api/docs/guides/chatkit-themes) | 可分别配置密度、字体、颜色、起始提示、输入区与历史/标题区域 | 官方组件规范；结论范围为可配置聊天组件，不是 ChatGPT 原生客户端视觉实测 |
| [Nintendo 角色对话设计访谈](https://www.nintendo.com/en-gb/Iwata-Asks/Iwata-Asks-Animal-Crossing-Let-s-Go-to-the-City/A-Day-in-the-Life-of-Animal-Crossing/5-Visiting-Your-Friends-with-the-DS-Suitcase/5-Visiting-Your-Friends-with-the-DS-Suitcase-233645.html) | 开发者强调动物的细微反应与话题自然衔接，并通过测试减少不自然的对话 | 2008 年历史设计访谈，用于角色表达与节奏分析，不代表最新游戏的 UI 视觉实测 |
| [Apple Popovers](https://developer.apple.com/design/human-interface-guidelines/popovers/) | 强调锚点关系、有限任务、关闭时保留工作、平滑尺寸转换与可脱离的面板 | 官方 HIG；正文另通过官方 DocC JSON 核对 |
| [Microsoft HAX 设计库](https://www.microsoft.com/en-us/haxtoolkit/library/) | 强调容易调用、容易收起、容易纠正、记住最近交互和谨慎改变行为 | 人机交互准则，用于评价操作链路 |
| [W3C 状态消息](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html)与[目标尺寸](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html) | 状态可被辅助技术识别且不需要抢焦点；目标尺寸标准包含尺寸、间距和例外条件 | 作为后续可访问性验收依据，本轮不宣称通过 WCAG 验收 |

取样覆盖快速输入、连续对话、完整阅读、系统锚定弹层、组件分层和角色表达六类问题。截图中的桌面背景、图片缩放和客户端版本不同，不能据图片像素比推断原生尺寸或性能。

**从样本学习什么**

1. Raycast 的快速窗口和完整会话具有连续关系。官方示例图中，正文直接使用文本层级，工具进度是弱化的一行，输入区位于稳定的底部；高级配置在另一块区域展开。对 BitCat 的启发是保留输入与阅读位置，把复制、扩展和配置放到更轻的操作层。模型选择、扩展管理和密集快捷键属于它的工具定位，是否进入 BitCat 应按现有产品预算判断。[Quick AI](https://manual.raycast.com/ai/quick-ai)、[AI Chat](https://manual.raycast.com/ai/ai-chat)。

2. Claude quick entry 的入口先服务“说一句话”。截图、窗口和语音是用户主动选择的上下文。对 BitCat 的启发是让输入成为明确的入口，并让观察范围可见；截图不适合藏在不易理解的聊天手势里。此结论来自操作说明，尚未评价其原生动画和具体排版。[官方说明](https://support.claude.com/en/articles/12626668-use-quick-entry-with-claude-desktop-on-mac)。

3. Notion 的指南示例使用留白、文字分组和较弱的工具图标组织空状态，输入区是主要焦点。另一张示例图把背景压暗，形成强烈的覆盖感。BitCat 可以学习其轻量引导与输入层级；日常陪伴需要继续保留桌面可见性，背景压暗的处理应另行评估。[官方指南](https://www.notion.com/en-gb/help/guides/everything-you-can-do-with-notion-ai?nxtPslug=everything-you-can-do-with-notion-ai)。

4. OpenAI ChatKit 把密度、主题、起始引导、输入工具与历史区域分开配置。对 BitCat 的启发是先确定每个状态需要哪些区域，再定皮肤。一个短回应状态可以比主动会话省略更多区域；先按场景设计，随后再评估组件实现方式。[官方定制规范](https://developers.openai.com/api/docs/guides/chatkit-themes)。

5. Apple 的弹层指南强调指向触发对象、保护草稿和在脱离后保留上下文。对 BitCat 的启发是短回应与猫建立空间关系，用户明确阅读时可固定位置；改变尺寸应表现为同一个对象的展开。该建议是把系统设计原则迁移到 Windows 桌宠的设计假设，仍需原生窗口验证。[Popovers](https://developer.apple.com/design/human-interface-guidelines/popovers/)。

6. Nintendo 的访谈把动物的细微反应与自然对话流放在一起讨论。对 BitCat 的启发是把猫的动作、文字出现与用户下一步输入视为同一个回应过程；避免每一轮都重新出现泛化的欢迎语和功能菜单。角色感可以来自称呼、语气和少量与话语对应的动作，设计原型需要让猫一起出现在画面里评估。[历史设计访谈](https://www.nintendo.com/en-gb/Iwata-Asks/Iwata-Asks-Animal-Crossing-Let-s-Go-to-the-City/A-Day-in-the-Life-of-Animal-Crossing/5-Visiting-Your-Friends-with-the-DS-Suitcase/5-Visiting-Your-Friends-with-the-DS-Suitcase-233645.html)。

**优化前 BitCat 的证据**

以下是本轮调研时的实现基线。后续改动、体验结果和限制见 [迭代评估](chat-interface-iteration-2026-10-02.md)。

代码入口：[bubble.html](../../app/frontend/bubble.html)、[bubble.css](../../app/frontend/css/bubble.css)、[bubble.js](../../app/frontend/js/bubble.js)、[bubble.rs](../../app/src/bubble.rs)。

| 已确认的现状 | 用户体验上的影响或待验证点 |
| --- | --- |
| 容器兼用 notice、compose、stream、reading-mode；宽度和高度由内容、状态与手动尺寸共同决定 | 需要明确每次状态变化的形态契约，区分合理展开与不必要的跳动 |
| 输入为单行 input；发送成功后 `hideInputSmooth()` 收起输入；`finishStreaming()` 显示回复操作但不调用 `showInput()` | 继续追问通常需要先点“继续说”重新进入输入；注释中的“结束后自动重新展开”与该路径不一致 |
| `submitChat()` 在 invoke 成功前清空输入；失败路径主要记录诊断和 console | 应保留可恢复草稿，并在界面给出发送失败与重试路径；这是代码路径核对，尚未注入原生 IPC 故障验证 |
| 正文区展示当前 AI 文本；输入消息没有对应的可见消息时间线 | 多轮追问缺少可回看的视觉上下文。已有记忆摘要含截断，不能直接作为完整聊天历史使用 |
| 回复后有“继续说 / 展开阅读 / 复制”等独立按钮 | 输入常驻后可移除“继续说”；复制与展开可以采用更轻、位置稳定的操作入口 |
| 外框、内框、网格纹理、纸张渐变、多层阴影、输入边框、按钮边框同时出现 | 本地渲染呈现明显的面板/便笺感；下一轮应比较层级与边界处理的组合 |
| 常规回复启动 15 秒自动隐藏；reading-mode 会停止自动隐藏 | 已有阅读保护可继续沿用，并补输入、选中、滚动和恢复会话的完整规则 |
| 流式完成时清除上翻标记并强制滚到底部 | 用户已向上阅读时，完成事件可能改变阅读位置；这条路径应纳入原生验证与新状态契约 |
| 阅读切换主要是尺寸与 CSS 类变化；Rust follower 对可见气泡继续跟随宠物 | 专注阅读能否固定在屏幕上，涉及窗口位置策略，不能只通过改颜色实现 |
| Rust 在流式结束时将 `chat_active` 置为 false | 前端阅读态与后端观察避让是否一致，需要验证；用户正在读的内容应获得保护 |
| 容器为 overflow:hidden，箭头位于容器边缘之外 | 静态渲染中没有形成明显的指向关系；需核对箭头裁切与原生定位后的实际效果 |

本地截图使用生产 HTML/CSS、示例文字与浏览器内临时 DOM 状态，分别固定为输入 360×300、手动放大的短回复 400×390、阅读 440×560。它们用于观察边框、文字、按钮与输入层级，不代表自动尺寸算法的实机输出。尤其不能把手动放大后的留白直接认定为自动布局缺陷。

**下一轮可以验证的状态契约**

| 状态 | 用户意图 | 可见内容 | 窗口与退出规则 |
| --- | --- | --- | --- |
| 短回应 | 看猫说一句话 | 猫的短文本，必要时一个继续入口 | 与猫保持锚定，轻量出现和退场，不抢键盘焦点 |
| 主动会话 | 连续聊几句 | 最近真实消息、稳定输入区、必要的停止操作 | 连续输入期间位置与主尺寸稳定；收起保留草稿和上下文 |
| 专注阅读 | 读长文、代码或表格 | 完整正文、可访问的复制和返回操作 | 用户明确展开，停止自动消失，可固定位置；返回时保留阅读位置 |

流式生成、工具执行、停止与失败是会话中的状态，不必都变成另一种窗口形态。新方案应同时定义“收起窗口”和“停止回复”的行为；用户能分别控制可见性和执行过程。[HAX](https://www.microsoft.com/en-us/haxtoolkit/library/) 支持这种对调用、收起与纠正的分开评价。

**三个值得对照的设计方向**

| 方向 | 形态假设 | 主要收益 | 需要验证的代价 |
| --- | --- | --- | --- |
| A：猫旁短气泡，主动时展开会话 | 少量短文本紧邻猫；用户输入后形成稳定的会话面板 | 陪伴与连续对话都有明确的位置 | 状态转换、展开方向和会话恢复必须一致 |
| B：紧凑的猫旁聊天面板 | 主动打开即显示最近消息与固定输入，阅读仍在同一面板 | 操作直接，状态数量较少 | 小屏幕占用、猫移动时的阅读稳定性 |
| C：输入入口与阅读面板分开 | 短输入靠近猫，复杂结果交给固定阅读区域 | 深度内容空间更充分 | 用户注意力需要跨位置移动，会话归属感与入口预算更难保持 |

A 最值得先验证，B 可以作为更简单的对照组。C 适合出现明确的深度阅读需求后再判断。这个排序是基于本项目陪伴优先原则的推断，还没有用户测试结果。

视觉上也应比较两种消息组织：用户消息用轻背景区分、猫的回复使用正文留白；或双方都使用平铺的角色标签与段落。不能只用一份文字方案决定优雅程度，需要放进猫和真实桌面环境观察。

**排版与操作的学习结论**

- 正文先保持共享文字规范，建立正文、状态、操作三层。像素特征可以留在猫和少量标识中，正文的重点是阅读。
- 选择一种主要边界表达，验证它在亮、暗、复杂壁纸上的可读性。透明度、暖色、圆角和阴影都不是单独的优雅指标。
- 用户消息与猫的回复要有可扫描的区别，避免每条内容都增加同样重量的外框；是否使用头像应结合桌面上已有的猫一起判断。
- 长文主要沿一个垂直方向阅读；代码和宽表格保留必要的横向滚动。需要在正文滚动、代码滚动与窗口调整之间验证输入路由。
- 正常工具进度使用较轻的单行状态。失败、授权和需要用户动作的情况保持可见并给出明确操作。
- 次级操作可以按需出现，但键盘聚焦时也应可发现；核心发送、停止和收起入口始终可找到。
- 状态更新不应夺取输入焦点；辅助技术通知应采用适当语义，避免逐个流式字符重复朗读。[W3C 状态消息](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html)。
- 轻量外观需要足够的命中区域。按目标尺寸、间距及适用例外核对，而不是把所有图标缩小。[W3C 目标尺寸](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)。

**后续学习原型的验证方式**

先对照 A/B 两种形态和两种消息组织，覆盖以下内容：一句短回应、三轮追问、中文长句、多行粘贴、长回复、代码和表格、工具失败、未授权能力、停止生成、收起后恢复、生成中上翻阅读。

记录能否直接继续输入、是否丢失草稿、操作区是否移动、是否误触、读过的内容是否被覆盖、是否需要额外找按钮。先看连续完成任务的表现，再决定视觉细节。

静态原型可检查布局、DOM 和键盘路径；Windows 实机检查置顶、位置跟随、IME、DPI 缩放、多显示器边界与滚轮转发。尺寸与动效时长先作为实验参数，经过这些场景再写进正式设计规范。

本轮临时图册位于 `.playwright-cli/chat-design-research/reference-board.html`。官方图片及本地状态截图均留在同目录；图册明确标注来源和证据类型。临时产物被 gitignore，长期可追溯依据是本文的来源链接与代码入口。
