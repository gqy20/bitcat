# BitCat Steam 发布调研与准备清单

更新日期：2026-05-17  
适用对象：`BitCat` 当前 Windows/Tauri 桌面应用形态

## 结论先行

BitCat 可以按 Steam 的“game or software/application”流程准备上架。Steam 不强制集成 Steamworks API；只要能通过 SteamPipe 上传可启动构建、完成商店页/定价/内容问卷、通过 Valve 审核，就可以发布。但这个项目有几类会被审核重点关注的能力：

- 实时生成式 AI：普通对话、Vision 截图分析、AI 工具调用、记忆写入。
- 屏幕与剪贴板读取：后台截图、手动截图、最近截图记录、读剪贴板工具。
- 系统级操作：执行 shell、启动程序、模拟快捷键、置顶窗口。
- 外部服务/API：Anthropic 或兼容模型 API、用户 API Key、可能的网络调用成本。

建议把 Steam 首发定位为“桌面宠物/生产力陪伴软件”，而不是传统游戏。商店页要坦诚说明 AI、截屏、记忆、系统操作权限，并把高敏功能做成默认关闭或明确用户触发。技术打包上，Steam depot 应上传解压后的 release 目录，而不是把 portable zip 当作最终安装内容。

## 官方硬性流程

1. 注册/完成 Steamworks Partner onboarding。
   Steamworks 要先完成账号、银行、税务、公司/个人信息验证，之后才能创建第一个 Application。

2. 为每个新 App 支付 Steam Direct Fee。
   当前官方口径是每个新 App 支付 `100 USD` 或等值费用；费用不可退款，但产品在 Steam 商店或应用内购买达到 `1,000 USD Adjusted Gross Revenue` 后可在后续付款中抵扣。

3. 创建 Application/AppID。
   Steamworks 后台创建新 App 后，会得到 AppID、默认 depot、商店页 checklist、构建 checklist、内容问卷等。

4. 下载 Steamworks SDK。
   SDK 里包含 SteamPipe/SteamCMD 上传工具，以及示例 `app_build*.vdf` / `depot_build*.vdf` 脚本。

5. 准备 SteamPipe 构建与 depot。
   depot 是交付给用户的一组文件。Windows 首发可以只做一个 Windows 64-bit depot：`bitcat.exe` + `config/*.yml` + 必要资源/许可证文件。SDL2 当前静态链接，不需要额外复制 `SDL2.dll`。

6. 完成商店页、定价和内容问卷。
   商店页、提议定价、产品构建都要给 Valve 审核。内容问卷里生成式 AI 部分必须详细描述“开发中使用的 AI”和“运行时 live-generated AI”，并说明 guardrails。

7. 提交商店页审核。
   Valve 通常 3-5 个工作日审核商店页，官方建议至少提前 7 个工作日提交，避免返工影响排期。

8. 发布 Coming Soon 页。
   新产品正式发售前，Coming Soon 页必须至少公开 2 周。Steam 建议较早挂页积累愿望单。

9. 提交产品构建审核。
   构建审核通常也是 3-5 个工作日，官方建议至少提前 7 个工作日提交。商店页要先审核通过，才能提交构建审核。

10. 到点手动 Release App。
    审核通过不会自动发售。到发布时刻，需要有 `Publish app changes to Steam` 和 `Manage pricing and discounts` 权限的账号点击绿色 `Release App` / `Publish Now` / `Release Now`。

## 建议时间线

保守排期按 6-8 周准备：

- T-8 周：确定产品名、定价、隐私策略、AI/截屏权限策略，创建 AppID。
- T-7 周：跑通 SteamPipe 上传到私有 beta branch，Steam 客户端本机安装测试。
- T-6 周：准备商店文案、5 张以上 1920x1080 截图、短 trailer、胶囊图、图标。
- T-5 周：完成内容问卷，提交商店页审核。
- T-4 周：商店页通过后点击 `Post as Coming Soon`。
- T-3 周：提交 near-final 构建审核，同时继续修 bug。
- T-2 周：Coming Soon 已满足最低 2 周；冻结首发功能，做干净机器测试。
- T-0：确认构建、价格、launch discount、商店包都 ready 后手动发布。

## SteamPipe 打包落地方案

当前仓库已经有：

```powershell
make release
make dist
cargo run -p xtask -- package-portable --version v0.1.0 --release-dir target/release --out-dir .
```

Steam depot 不建议上传 zip。建议新增一个 `xtask package-steam` 或复用 `package-portable` 的 staging 逻辑，输出类似：

```text
target/steam-content/
  bitcat.exe
  config/
    actions.yml
    buttons.yml
    panel_action.yml
    prompts.yml
    user.yml
  LICENSES.third-party.md
  README.md
```

Steamworks 里设置：

- Launch option：`bitcat.exe`
- OS：Windows
- Architecture：64-bit
- Install folder：建议稳定为 `bitcat` 或 `BitCat`
- Depot：首发一个 Windows depot 即可
- Branches：`default` 用于正式，`beta` / `internal` 用于测试

最小 SteamPipe 脚本形态：

```vdf
"AppBuild"
{
  "AppID" "YOUR_APP_ID"
  "Desc" "BitCat Windows release v0.1.x"
  "ContentRoot" "..\\content\\"
  "BuildOutput" "..\\output\\"
  "Depots"
  {
    "YOUR_DEPOT_ID"
    {
      "FileMapping"
      {
        "LocalPath" "*"
        "DepotPath" "."
        "recursive" "1"
      }
    }
  }
}
```

上传命令形态：

```powershell
tools\ContentBuilder\builder\steamcmd.exe +login <build_account> +run_app_build ..\scripts\app_build_bitcat.vdf +quit
```

首次建议先上传到私有 beta branch，在 Steam 客户端里安装运行，验证这些点：

- 非开发机可启动，WebView2 依赖表现正常。
- `config/*.yml` 能从 exe 同目录加载。
- `~/.bitcat/` 下日志、截图、记忆、设置写入正常。
- 托盘、透明窗口、置顶、全局热键、手柄 SDL2 轮询可用。
- 没有开发 API Key、`.env`、日志、测试产物、`.pdb` 被打进 depot。

## Steamworks API：首发可不接，但有三项值得评估

Steam 明确说 Steamworks API 不是发布必需项，但推荐接入。BitCat 首发可以先不接 Steamworks API，把风险集中在核心体验上。后续优先级：

1. Steam Cloud
   用于同步设置、用户画像、长期记忆和自定义舞蹈。当前数据主要在 `~/.bitcat/`，Auto-Cloud 对这种路径不一定最顺手；建议先把可同步用户数据迁到 `%APPDATA%/bitcat/` 或 `%LOCALAPPDATA%/bitcat/`，再配置 Auto-Cloud。不要同步截图原图、token 日志、临时日志。

2. Steam Input
   当前用 SDL2 直接读手柄，能跑即可。若要让 Steam Deck/手柄配置体验更原生，再接 Steam Input action manifest。

3. Steam DRM / 所有权校验
   Steam DRM wrapper 不是强反盗版方案，而且可能增加误报/兼容风险。这个项目更适合后续用 Steamworks API 校验 Steam 启动/所有权，而不是首发强行 wrapper。

## 商店页素材清单

必须准备：

- Header Capsule：`920x430`
- Small Capsule：`462x174`
- Main Capsule：`1232x706`
- Vertical Capsule：`748x896`
- Library Capsule：`600x900`
- Library Hero：`3840x1240`
- Library Logo：`1280` 宽或 `720` 高 PNG
- Library Header Capsule：`920x430`
- Shortcut Icon：`256x256` `.ico` 或 `.png`
- App Icon：`184x184` `.jpg`
- 截图：至少 5 张，最低 `1920x1080`，16:9

素材规则重点：

- 胶囊图只放产品美术、产品名、官方副标题；不要放评分、奖项、折扣文案、跨产品宣传或杂项营销字。
- 截图要展示真实产品运行画面，不要用概念图、预渲染静帧、奖项/营销文案图。
- 对 BitCat 来说，截图应覆盖：宠物状态、聊天气泡、设置页、手柄面板、截图观察权限提示、小游戏/舞蹈。
- Trailer 建议 45-75 秒，展示真实桌面使用：出现宠物、手柄触发、AI 对话、手动截图、设置里关闭/开启敏感功能。

## 定价与折扣

Steam 支持多币种定价，初始定价和价格调整会由 Valve 审核，通常 1-2 个工作日。BitCat 当前更像小型桌面软件/玩具，建议先做同类竞品价格带调研后再定价。

可选 launch discount：

- 只能在发布前配置。
- 折扣范围：10%-40%。
- 时长：7-14 天。
- 产品发布后 30 天内不能再做普通折扣，launch discount 是发布时例外。

如果后续要做订阅、AI 点数、内购，Steam 规则会更敏感：在 Steam 内销售的应用内交易必须使用 Steam Wallet。更稳妥的首发方案是 BYOK（用户自带 API Key）或免费额度/本地功能，不在应用内售卖外部支付的 AI 服务。

## AI、隐私与审核风险

必须在内容问卷和商店页/隐私政策里写清楚：

- 运行时使用生成式 AI：聊天回复、截图分析、活动摘要、记忆候选、舞蹈/工具意图。
- Live-generated guardrails：系统提示词、工具 schema 校验、权限 hook、危险命令阻断、输出长度限制、用户可关闭截图观察、敏感功能需确认。
- 截图处理：何时截图、是否默认开启、发送给哪个模型服务、保存在哪里、保留多久、如何删除。
- 记忆处理：短期/长期记忆保存位置、用户如何审查/删除、是否上传到模型上下文。
- 外部服务：API Key 来源、base_url 可配置、请求会发送到第三方模型服务。

发布前建议改成：

- 首次启动显示权限/隐私 onboarding。
- 后台截图观察默认关闭，用户显式开启后才运行。
- `shell`、`launch_program`、`send_hotkey`、`read_clipboard` 默认处于受限模式，首次使用逐项确认。
- 设置页提供“一键清除截图/记忆/日志”。
- 不随包发布 `.env` 或开发者 API Key。
- Store page 不承诺尚未完成的能力；未来功能写成 roadmap/公告，不写成 launch features。

## BitCat 发布前技术清单

P0，发布前必须：

- 做 `package-steam` staging，确保 depot 内容可重复生成。
- 干净 Windows 机器/VM 从 Steam 客户端安装测试。
- 首次启动隐私/权限确认。
- 截图观察默认关闭或强提示。
- API Key 不入包，`.env` 不入包。
- 审核模式/演示模式：没有 API Key 时仍能打开宠物、设置、面板、小游戏，并给出清晰提示。
- Store 文案与构建功能逐项一致。
- 第三方许可证文件随 depot 分发。
- `make test-fast`、`make test-app`、前端 vitest 通过。

P1，强烈建议：

- Steam beta branch 自动上传脚本。
- Auto-update 只走 Steam，不在应用内自更新。
- 将可同步用户数据从 `~/.bitcat` 迁到更标准的 AppData 路径，便于 Steam Cloud。
- 添加崩溃日志/诊断导出，但用户可控。
- 建立发布回滚流程：保留上一个 stable build，可在 Steamworks builds 页面回滚。

P2，后续增强：

- Steam Cloud 同步设置/舞蹈/长期记忆。
- Steam Input action manifest。
- 成就/统计：比如首次聊天、完成一次舞蹈、完成小游戏。
- Steam Deck/Big Picture 适配测试。

## 建议商店定位草稿

短描述方向：

> BitCat 是一个住在桌面角落的 AI 伙伴。它可以用手柄唤起聊天、快捷面板、小游戏和表演，也可以在你允许时观察屏幕并帮你整理上下文。

标签方向：

- Software
- Utilities
- Casual
- Cute
- Pixel Graphics
- Artificial Intelligence
- Productivity
- Singleplayer

注意：如果选择 “Game” 类目，用户会期待更明确的游戏循环；如果选择 “Software”，流量可能小一些但预期更准确。当前形态建议优先 Software/Utility，等小游戏和桌宠玩法足够完整再考虑更强游戏化包装。

## 参考来源

- Steamworks Getting Started：`https://partner.steamgames.com/doc/gettingstarted`
- Steam Direct Fee：`https://partner.steamgames.com/doc/gettingstarted/appfee`
- Steam Review Process：`https://partner.steamgames.com/doc/store/review_process`
- Steam Release Process：`https://partner.steamgames.com/doc/store/releasing`
- Steam Coming Soon：`https://partner.steamgames.com/doc/store/coming_soon`
- Steam Content Survey / Generative AI：`https://partner.steamgames.com/doc/gettingstarted/contentsurvey`
- SteamPipe Uploading：`https://partner.steamgames.com/doc/sdk/uploading`
- Steam Depots：`https://partner.steamgames.com/doc/store/application/depots`
- Steam Graphical Assets：`https://partner.steamgames.com/doc/store/assets`
- Store Graphical Assets：`https://partner.steamgames.com/doc/store/assets/standard`
- Graphical Asset Rules：`https://partner.steamgames.com/doc/store/assets/rules`
- Steam Pricing：`https://partner.steamgames.com/doc/store/pricing`
- Steam Discounting：`https://partner.steamgames.com/doc/marketing/discounts`
- Steamworks API Overview：`https://partner.steamgames.com/doc/sdk/api`
- Steam Cloud：`https://partner.steamgames.com/doc/features/cloud`
- Steam DRM：`https://partner.steamgames.com/doc/features/drm`
- Common Redistributables：`https://partner.steamgames.com/doc/features/common_redist`

