# 宠物资源包发布计划

> 状态：活跃  
> 更新日期：2026-06-08

当前宠物渲染已经进入 v2-only 资源包模式。内置默认不再走 `sprite.js` 硬编码 fallback，而是加载 bundled v2 pack；配置了资源包时加载失败会直接暴露错误，避免旧版本兼容路径掩盖问题。

## 当前基线

最终软件只打包 15 个内置猫咪资源包，目录位于 `app/frontend/__fixtures__/pets/`：

| 资源包 | 类型 | 尺寸策略 | 备注 |
|------|------|----------|------|
| `cat-tabby` | 192x208 WebP v2 sheet | 74x80 显示 | 当前默认内置狸花/橘色虎斑小猫 |
| `cat-black` / `cat-blue-gray` / `cat-calico` / `cat-cow` / `cat-cream` | 192x208 WebP v2 sheet | 74x80 显示 | 内置猫咪品种 |
| `cat-ginger` / `cat-gray` / `cat-lilac` / `cat-ragdoll` / `cat-siamese` | 192x208 WebP v2 sheet | 74x80 显示 | 内置猫咪品种 |
| `cat-snowshoe` / `cat-tortie` / `cat-tuxedo` / `cat-white` | 192x208 WebP v2 sheet | 74x80 显示 | 内置猫咪品种 |

代码入口：

- `app/frontend/js/sprite-loader.js`：v2 manifest loader，默认 URL 为 `/__fixtures__/pets/cat-tabby`。
- `app/frontend/js/settings.js`：设置页资源包 preset 列表。
- `app/frontend/__tests__/pet-catalog-fixtures.test.js`：确保 bundled catalog 可加载。
- `app/frontend/__tests__/pet-fixture.test.js`：确保默认内置资源覆盖基础状态。

## 已完成

- v1 schema 与硬编码 fallback 已清理。
- `cat-tabby` 成为默认 v2 内置资源包。
- 内置 catalog 收敛为 15 个猫咪品种，不再随最终软件打包 `piggy`、终端状态风或其他角色资源。
- catalog 资源包统一使用 manifest 加载，并补 `metadata.qualityTier` / `assetClass` / `releaseTier`。
- manifest `actions` 已支持 timeline；15 个猫咪资源包均提供语义动作，用于截图、输入和拖拽反馈。
- 配置了外部资源时加载失败直接失败，不回退内置宠物。
- 设置页已经提供 bundled preset 选择和自定义地址入口。

## 待决策

1. **发布包体积预算**
   - 固定内置 15 个猫咪资源包。
   - 其他实验资源不放入 `app/frontend/__fixtures__/pets/`，避免进入最终软件包。

2. **外部资源包目录**
   - 推荐正式入口：`~/.bitcat/pets/<id>/manifest.json`。
   - 当前 project-local `/__fixtures__/pets/<id>` 适合作为开发与 bundled 资源入口。

3. **设置页预览与诊断**
   - 保存前校验 manifest。
   - 显示资源包 id、schemaVersion、显示尺寸、状态覆盖和图片加载错误。
   - 自定义地址失败时给出 UI toast，而不是只依赖 console。

4. **资源包分层**
   - 必选内置：15 个 `cat-*` 品种。
   - 非内置资源：后续若恢复，应放到用户目录或外部下载，不放入最终软件包。

## 下一步

1. 在打包前统计每个 bundled pack 的实际字节数，写入 release checklist。
2. 给设置页增加“测试资源包”按钮，复用 `loadPetAssetPack()` 做 manifest/image 校验。
3. 支持 `~/.bitcat/pets/<id>` 用户目录扫描，并把结果合并到 preset 列表。
4. 根据发布包大小决定是否把大 WebP 资源包移到外部下载或开发 fixtures。
