# 统一文字规范

## 唯一来源

`app/frontend/css/typography.css` 管理字号、字重、行高和完整文字样式。`fonts.css` 导入它并管理字体文件及字体族；所有窗口通过 `fonts.css`（直接或经 `ui-tokens.css`）获得同一套变量，不需要引入其他窗口的配色。

`ui-tokens.css` 不再重复声明字号。旧 `--text-*`、`--font-size-*` 名称由 typography.css 提供语义别名，数值仍只有一个来源。

## 文字角色

| 角色 | 变量 | 字号 | 建议字重 / 行高 |
| --- | --- | --- | --- |
| 页面标题 | `--type-page` | 26px | bold / heading（1.3） |
| 窄窗口标题 | `--type-page-compact` | 23px | bold / heading |
| 分区标题 | `--type-section` | 16px | bold / heading |
| 条目标题 | `--type-item` | 15px | semibold / heading |
| 正文、设置标签 | `--type-body` | 14px | regular 或 semibold / body（1.5） |
| 按钮、紧凑正文 | `--type-control` | 13px | semibold / label（1.25） |
| 时间、来源、辅助说明 | `--type-caption` | 12px | regular / compact（1.4） |
| 短标签 | `--type-tag` | 11px | medium / compact |
| 统计数字 | `--type-metric` | 23px | bold / tight（1.1） |
| 统计数字紧凑变体 | `--type-metric-compact/small/tight` | 20 / 18 / 16px | 按容器宽度选用 |
| 代码和配置内容 | `--type-code` | 13px | 等宽字体 |

默认根文字样式使用 14px 正文、400 字重及 1.5 行高；紧凑窗口使用 13px、1.4 行高作为继承基线。

字重只使用 regular 400、medium 500、semibold 600、bold 700、display 800。display 专用于游戏展示数字等强调场景；普通标题不使用它。

新组件优先使用完整样式，例如 `font: var(--font-page)`、`font: var(--font-section)`、`font: var(--font-body)`、`font: var(--font-caption)`、`font: var(--font-control)`。已有复杂组件可分别引用字号、字重与行高变量。

## 紧凑窗口

Agent Watch、通知与 Inbox 使用 `--type-dense-*`：标题 14px、正文 13px、次要内容 12px、元信息 11px、短状态徽标 10px。10px 不用于正文。保留紧凑窗口的明确密度，不把所有窗口强制套成设置页尺寸。

原先 9.4、10.2、10.5、12.8、13.5px 等局部微调已归并。固定高度控件的 `--line-*` 是垂直对齐尺寸；不要把 22px 按钮行盒用于文章行高。气泡 Markdown 保留标题层级，正文 14px；代码等相对缩放由共享变量管理。

## 边界与例外

- 图标字号、宠物粒子符号、游戏 UI 展示数字使用独立角色，不能当成正文档位。
- 游戏 Canvas 绘制字号、设置页自定义资源占位图的 Canvas 字号依画布尺寸计算；不纳入 DOM 排版角色。
- `@font-face` 的字重范围用于字体加载，不属于组件字重。
- 开发专用的 `test.html`、`game_dev.html` 不作为产品窗口纳入本次规范。
- 不改变权限、功能入口、持久化格式和任何默认能力。

## 检查

在 `app/frontend` 运行：

```sh
npm run check:typography
npm test
```

检查器覆盖生产 CSS 和语音窗口内联样式，拒绝局部数值字号、字重、行高和数值字体简写。新增角色先修改 typography.css 和本规范，再在组件引用；不要绕过检查增加局部数值。

## 产品自检

- 时刻：照料、互动与陪伴中的文字展示。
- 四个疑问：一致的标题、正文与辅助信息层级帮助扫描已有内容。
- 预算：入口与控件 +0 / -0。
- 文案：无新增用户术语；仅规范既有文字的呈现。
- 恐慌测试：不涉及权限与默认能力变更。

## 英文与数字：JetBrains Mono

英文、ASCII 数字及拉丁标点统一采用 JetBrains Mono v2.304，随应用内置 400/500/600/700/800 的正体与斜体 WOFF2，不依赖在线 CDN。中文界面继续使用 MiSans；代码区中文回退至 Sarasa Mono SC。

`--font-latin` 管理拉丁字体，`--font-ui`、`--font-brand`、`--font-mono` 均以它为首选。Canvas 游戏从 CSS 读取同一个 `--font-ui` 字体栈，保留按画布计算的字号。遗留 Geist 和 Source Code Pro 资源不再被字体样式引用。

来源：[JetBrains 官方仓库 v2.304](https://github.com/JetBrains/JetBrainsMono/tree/v2.304/fonts/webfonts)。原版文件未修改，授权文件随资源保存在 `assets/fonts/LICENSE-jetbrains-mono.txt`。
