# 像素猫接入与重建

本批接入选定的燕尾服黑白猫，以及橘猫、狸花、三花、奶牛、黑猫、白猫、英短蓝猫、暹罗、布偶、缅因共 11 包。

## 应用内入口

设置 → 它怎么陪着我 → 它长什么样 → 名称带「像素」的猫 → 保存。
默认仍是原版狸花猫；选择「默认」并保存可恢复原画风。原版 15 包均保留，已有保存地址不改变，未列在选择器中的原版地址仍可经现有自定义地址入口使用。
选择器复用 11 个旧位置，包括用缅因替代丁香猫的位置，没有新增卡片或控件。

## 烘焙资源

发行资源：`app/frontend/__fixtures__/pets/cat-pixel-*/`，每包一个 `spritesheet.png` 和 `manifest.json`。
统一 96×96 帧，八列排布。燕尾服猫 46 帧，其余十包各 50 帧，共 546 帧；每包有十六帧行走。PNG 与配置总量约 2.9 MB。
日常运行只切片播放；生成模型、图像分析工具及 Canvas 四肢计算均不在运行路径。

帧覆盖：待机、眨眼、起身、行走、坐下、睡觉、开心、好奇、拖拽反馈。说话/思考/工作/准备/游戏进行复用专注待机，失败/阻止复用好奇姿势；这些映射写在 manifest 的 aliases 与 metadata.sharedPoses，不会回退到旧猫图像。
11 类猫均有独立的提起、两张悬空变化、着地、缓冲和坐稳原画，已接入 pickup / dragging / drop。普通行走后的坐下仍为起身姿势逆序编排。仍有分层动画感，定位为试用美术，非最终逐像素精修资源。

## 生成与重建

`prompts.json` 保存内置 image_gen 的 21 次生成提示词：十类猫各一张起身图、一张身体/四肢分层图，另补燕尾服猫的反馈姿势。`drag-prompts.json` 另保存 11 张六姿势拖拽图的提示词。`sources.json` 保存经过检查的图像路径和 alpha 主体边界。
角色生成保持原图，模型无需重跑即可烘焙：

```sh
# 从仓库根目录启动仅本机可访问的静态服务
python -m http.server 4189 --bind 127.0.0.1
# 另一个终端，需安装 agent-browser
node app/frontend/tools/build-pixel-cat-packs.mjs http://127.0.0.1:4189
```

构建脚本读取已保存源图，调用 `pixel-cat-pack-builder.js` 进行同一套 Canvas 烘焙，输出运行资源。黑白猫使用前一轮已验收的 v2 烘焙动作。全套打包继续使用 `make build` / `xtask prepare-frontend`，未增加另一条发布打包路径。

## 协议与状态

- `render.facing: left` 指定源图朝向，渲染器按运动方向镜像；省略时沿用旧资源朝右假设。
- `render.stableBody: true` 停用重复 CSS 呼吸缩放与状态闪光。
- `states.walk.locomotion` 提供 `enterAction`、`exitAction`、逻辑像素速度 `speed`。加载时校验速度和动作引用。
- `PetStateMachine` 起身期间不推进位移，抵达后切坐下。重复目标不重播起身，睡眠/通知/拖拽可中断；不包含此字段的旧包保持原行为。
- `WindowWalk` 将逻辑位移按显示缩放与 DPI 转为原生物理坐标，限制在当前显示器范围内。位置写入串行，异步读取用 generation 作取消检查；拖拽开始前等待最后一个在途写入。

## 验证与限制

Vitest 覆盖资源加载/引用/PNG 尺寸/朝向、到达坐下、起身中重定向、睡眠和拖拽中断、负坐标显示器边界、DPI 转换及异步取消。
拖拽补齐后的全量前端回归 293 项通过，`make test-app` 177 项通过（4 项跳过），`cargo check -p bitcat-app` 与格式检查通过。11 包逐包验证动作状态，546 帧均非空且主体未触及帧边缘。
`tools/pixel-cats-preview.html` 使用真实应用加载器与状态机，可按猫查看动作和中断行为。

### 2026-09-27：完整拖拽反馈

- 11 只猫各新增一张六姿势生成图，保存在各自的 `drag.png`，提示词在 `drag-prompts.json`。悬空姿势是真正垂下四爪的独立原画，不再使用坐姿平移。
- `pickup` 三帧、`dragging` 两种悬空变化循环、`drop` 五帧。落地末帧复用当前猫的待机帧，避免最后跳回另一只猫或另一种坐姿。
- `PetStateMachine.beginDrag/endDrag/cancelDrag` 管理物理拖拽生命周期。提起后可无限保持，通知和睡眠更新保留到放下；快速松手、落地中重新提起、取消都有独立测试。
- 桌面前端使用 `NativePetDrag`。`startDragging()` 的 Promise 完成和位置静止均不代表松手。Windows 只在拖拽期间每 100ms 查询主键是否仍按住；原生拖拽吞掉 DOM pointerup 时也能完成落地。非 Windows 返回未知，等待明确的指针松手信号。
- Win32 查询使用高位按下状态，并处理交换左右键设置，依据 [Microsoft GetAsyncKeyState 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate)。原生转移指针捕获不作为松手；再次拖动会使旧的吸附回调失效。
- 设置页和局域网预览均接入按住/松手流程。主动拖拽不受减少动态效果的静帧分支影响，设置页不再按住五秒就复位。

验证记录：真实浏览器鼠标按住黑白猫 11.2 秒仍保持悬空，松手后依次落地、待机；设置页橘猫在减少动态效果模式下按住 6.2 秒仍保持悬空。11 类猫逐类采样得到两种悬空画面、五帧落地，最终返回待机。Windows FFI 函数已按 app 的 edition、windows-sys 版本和完整 features 原样复制到最小 crate，并通过 `cargo check --target x86_64-pc-windows-msvc`。

当前 `make test-app` 177 项通过（4 项跳过），前端 293 项通过。原生 Windows 拖拽、松手与吸附仍待实机验收；最小 crate 交叉检查不等于 Windows 运行验证。

### 预览页“腿不动但在移动”修复

原预览页在 `prefers-reduced-motion: reduce` 下始终绘制第 0 帧，但状态机与位置继续推进，造成静态猫滑行。之前只验证状态/位移，漏测了这个浏览器偏好分支。
修复后，主动触发的 walk 和它的起身/坐下动作保留逐帧播放，其他自动动画继续遵循减少动态效果偏好。生产宠物页不使用原有错误分支。
新增 `pixel-preview-playback.test.js` 覆盖正常与减少动态效果下的行走、过渡和待机。
实际浏览器开启减少动态效果后，逐只点击 11 类猫的“走几步”，采样 Canvas 腿部区域：每类均捕获 16 个不同的行走帧及 16 种不同腿部画面，显示帧与状态机帧一致，位置正常推进。

`make build` 成功生成前端发行目录，Linux 原生链接被现有 `app/src/tts.rs` 的 Windows COM 符号（CoInitializeEx/CoCreateInstance/CoUninitialize）阻止。本次未更改该模块。
仍需 Windows 实机核对窗口跟随、拖拽交接、150%/200% DPI、多屏边界与气泡跟随；不把浏览器模拟验证当作原生验收。
`npm run check:typography` 仍被原有 `.p-coin { font-size: 0; }` 阻止，此声明在本次改动前即存在，未为本任务改动字体检查器或金币样式。

## 产品自检

- 时刻：陪伴与互动。
- 四个疑问：不适用，本次接入已选定宠物美术。
- 预算：选择器卡片总数不变，复用 11 个位置，其他应用入口 +0 / -0。
- 文案：新增「像素」画风说明与原版恢复提示，无工程术语进入用户区。
- 恐慌测试：不增加权限、不改变默认皮肤；保存后才切换，选择默认并保存可恢复。
