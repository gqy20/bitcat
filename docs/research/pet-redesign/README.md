# 黑白猫造型探索

用户选定 B 黑白猫，原图保存在 `tuxedo-selected-concept.png`。
`tuxedo-idle-v1.png` 是使用内置 image_gen 生成的透明背景简化稿。
保留琥珀眼、不对称白斑、白胸白袜和钩形尾巴。

当前为造型参考，尚未完成严格 48×48 网格、有限色板与边缘清理验收，不能直接视为正式动画资源。现有应用资源未替换。

## 待机动画初版

- `preview.html`：双击可打开的独立预览，支持深浅背景、80/96/144/288px 显示、暂停、逐帧查看及图集下载。
- `tuxedo-idle-preview.gif`：可直接查看的循环动画，高度 192px。
- `tuxedo-idle-sheet-v1.png`：内置 image_gen 生成的四帧原图；提示词见 `animation-prompt.md`。
- `tuxedo-idle-stabilized.png`：预览页面导出的透明横向四帧图集，每帧 450×612。
- `idle-animation.json`：待机时间轴，9930ms 一轮。此文件是独立预览规格，并非完整应用 v2 manifest。
- `idle-preview.js`：对生成图的非均匀外边距作裁切定位，以第一帧身体为基准，只替换眼睛和尾巴局部区域，避免整只猫漂移。

动画包含睁眼、闭眼、尾尖向内、尾尖向外四个姿势，眨眼保持 130–140ms。GIF 导出以编码器时间单位取整，精确时长以 JSON 和网页预览为准。系统开启减少动态效果时，网页默认暂停。

### 验证

浏览器核对页面地址、标题、六个 Canvas 及加载状态；连续播放覆盖全部四帧。检查暂停、逐帧和 80px 尺寸切换。逐像素检查确认眼睛/尾巴区域以外相对基础帧零变化。深浅背景下完成截图检查。

### 产品自检

- 时刻：陪伴。
- 四个疑问：不适用，本次验证常驻角色的视觉和动作。
- 预算：应用控件/入口 +0 / -0，预览仅在研究目录。
- 文案：未变更应用文案。
- 恐慌测试：不涉及权限和默认值变更。

## 简化稿提示词

Use case: style-transfer. Edit the supplied BitCat black-and-white cat concept into a simplified production-oriented idle sprite master. Preserve this specific cat's identity: tall triangular ears, relaxed narrow amber eyes, asymmetric ivory blaze up the nose, ivory muzzle and chest bib, white front socks, charcoal black coat, slender seated body, upright hook-shaped tail on viewer's right with ivory tip. Preserve the pose, proportions, calm slightly aloof personality and facing direction. Simplify ONLY the rendering and fine detail to strict retro pixel art suitable for a 48x48 logical sprite enlarged with nearest neighbor: large clean flat pixel clusters, uniformly sized square pixels, approximately 10-12 discrete colors, no gradients, no textures, no antialiasing or smooth lighting, no tiny fur strands. Simplify chest fur into a few clear stepped clusters and whiskers to at most two short lines per side. Keep eyes readable at small size. Single cat, full body including tail and ears, centered with transparent padding. GENUINELY TRANSPARENT alpha background, no gray backdrop, no checkerboard drawn into image, no floor, no shadows, no text, no UI, no additional views or animation frames. A clean enlarged pixel sprite, not pixelated detailed illustration. This is the static master for a desktop companion.
