# 宠物资源包发布计划

> 状态：活跃  
> 更新日期：2026-05-17

当前宠物渲染已经进入 v2-only 资源包模式。内置默认不再走 `sprite.js` 硬编码 fallback，而是加载 bundled v2 pack；配置了资源包时加载失败会直接暴露错误，避免旧版本兼容路径掩盖问题。

## 当前基线

Bundled 资源包位于 `app/frontend/__fixtures__/pets/`：

| 资源包 | 类型 | 尺寸策略 | 备注 |
|------|------|----------|------|
| `piggy` | 192x208 PNG v2 sheet | 74x80 显示 | 当前默认内置小猪，高分辨率终端状态风资源 |
| `cat` | 16x16 PNG v2 pack | 128x128 显示 | 旧小猫资源包，保留为可选 |
| `status` | 大尺寸 WebP v2 sheet | 69x75 显示 | 状态化终端风资源 |
| `core` | 大尺寸 WebP v2 sheet | 69x75 显示 | 状态化资源 |
| `dewey` / `fireball` / `rocky` / `seedy` / `stacky` / `bsod` / `null-signal` | 大尺寸 WebP v2 sheet | 69x75 显示 | 可选资源目录 |

代码入口：

- `app/frontend/js/sprite-loader.js`：v2 manifest loader，默认 URL 为 `/__fixtures__/pets/piggy`。
- `app/frontend/js/settings.js`：设置页资源包 preset 列表。
- `app/frontend/__tests__/pet-catalog-fixtures.test.js`：确保 bundled catalog 可加载。
- `app/frontend/__tests__/piggy-fixture.test.js` / `pet-fixture.test.js`：确保轻量内置资源覆盖基础状态。

## 已完成

- v1 schema 与硬编码 fallback 已清理。
- `piggy` 成为默认 v2 内置资源包。
- `cat` 去掉 `default-` 命名，仅作为普通可选资源包。
- catalog 资源包统一使用 manifest 加载。
- 配置了外部资源时加载失败直接失败，不回退内置宠物。
- 设置页已经提供 bundled preset 选择和自定义地址入口。

## 待决策

1. **发布包体积预算**
   - `piggy` / `cat` 很小，可以长期内置。
   - 大尺寸 WebP 资源包会让发布包增加数 MB，需要决定哪些进入正式 bundle。

2. **外部资源包目录**
   - 推荐正式入口：`~/.ai-pad/pets/<id>/manifest.json`。
   - 当前 project-local `/__fixtures__/pets/<id>` 适合作为开发与 bundled 资源入口。

3. **设置页预览与诊断**
   - 保存前校验 manifest。
   - 显示资源包 id、schemaVersion、显示尺寸、状态覆盖和图片加载错误。
   - 自定义地址失败时给出 UI toast，而不是只依赖 console。

4. **资源包分层**
   - 必选内置：`piggy`、`cat`。
   - 候选内置：`status`、`core`。
   - 候选外置/下载：`dewey`、`fireball`、`rocky`、`seedy`、`stacky`、`bsod`、`null-signal`。

## 下一步

1. 在打包前统计每个 bundled pack 的实际字节数，写入 release checklist。
2. 给设置页增加“测试资源包”按钮，复用 `loadPetAssetPack()` 做 manifest/image 校验。
3. 支持 `~/.ai-pad/pets/<id>` 用户目录扫描，并把结果合并到 preset 列表。
4. 根据发布包大小决定是否把大 WebP 资源包移到外部下载或开发 fixtures。
