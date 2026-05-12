# Third-Party Licenses

本文档列出 `ai-pad`（8Bit Cat）所有第三方依赖的开源许可证及商用合规说明。

**生成日期**: 2026-05-13
**依赖总数**: 634 个 crate（含间接依赖）
**分析工具**: cargo-license v0.7.0

---

## 商用合规结论

> **本项目可以商用。** 所有依赖的许可证组合在 Windows 桌面应用分发场景下均不构成商用障碍。
>
> 核心直接依赖（tauri、tokio、serde、reqwest、sdl2、rig-core、tracing、windows-sys、image 等）全部使用 MIT / Apache-2.0 等宽松协议，无任何限制。

以下是需要注意的例外情况。

---

## 1. MPL-2.0 弱 Copyleft 依赖（5 个）

MPL-2.0 是**文件级弱 Copyleft**：修改了这些 crate 的源码必须开源修改部分，但可以与闭源代码一起编译/分发。作为下游用户**原样链接使用不触发开源义务**。

| Crate | 版本 | 引入路径 | 说明 |
|-------|------|----------|------|
| `cssparser` | 0.36.0 | tauri → wry → Servo | CSS 解析器 |
| `cssparser-macros` | 0.6.1 | 同上 | cssparser 的 proc macro |
| `dtoa-short` | 0.3.5 | 同上 | 浮点数转字符串 |
| `option-ext` | 0.2.0 | tauri → dirs → ... | Option 扩展方法 |
| `selectors` | 0.36.1 | tauri → wry → Servo | CSS 选择器引擎 |

### 合规要求

- **允许**：原样使用、闭源分发、商业用途、静态/动态链接
- **禁止**：修改上述 crate 的源码后，仅以二进制形式分发而不公开修改部分的源码
- **实际影响**：本项目的引入方式为通过 tauri/wry 间接依赖，未直接修改源码，因此 **MPL-2.0 条件不构成实质障碍**

---

## 2. Unicode-3.0 许可依赖（18 个）

通过 ICU4X（Unicode 国际化组件）引入的数据与工具库：

```
icu_collections, icu_locale_core, icu_normalizer, icu_normalizer_data,
icu_properties, icu_properties_data, icu_provider,
litemap, potential_utf, tinystr, writeable,
yoke, yoke-derive, zerofrom, zerofrom-derive, zerotrie, zerovec, zerovec-derive
```

### Unicode-3.0 许可关键条款

- 允许免费使用、复制、修改、合并、发布、分发、再授权、商业化
- **唯一限制**：不得将 Unicode 数据和软件用于创建或推广**竞争性的字符编码标准**
- 必须保留版权声明和许可声明

### 合规要求

- 对于桌面应用场景，此限制**不会触发**
- 正常的商业软件分发完全不受影响
- 只要你不是在做"替代 Unicode 的编码标准"即可安全使用

---

## 3. LGPL-2.1-or-later 依赖（2 个）

| Crate | 版本 | 协议选项 | 说明 |
|-------|------|----------|------|
| `r-efi` | 5.3.0 | Apache-2.0 OR LGPL-2.1+ OR MIT | UEFI 固件接口绑定 |
| `r-efi` | 6.0.0 | 同上 | 同上 |

### 合规要求

- 该 crate 是 UEFI 固件接口绑定，**仅在非 Windows 目标平台可能被实际编译链接**
- 本项目为 Windows 桌面应用，该 crate 在 Windows 构建中**不会被链接到最终产物**
- 即使被链接，LGPL-2.1 对动态链接不要求开源自身代码
- 且该 crate 提供 **MIT / Apache-2.0 双重授权选项**，可选用宽松协议

---

## 4. 仅开发期依赖（不影响最终产物）

| Crate | 协议 | 原因 |
|-------|------|------|
| `cargo-husky` | Custom License File (MIT-like) | Git hooks 工具，仅开发时运行，不进入二进制产物 |

---

## 5. 完全无限制的协议分类

以下协议均允许自由商用、修改、闭源分发，无需额外义务：

| 协议 | 代表性依赖 | 数量级 |
|------|-----------|--------|
| **MIT** | tokio, tracing, serde, thiserror, sdl2, rig-core, reqwest, image, hyper, bytes, wiremock, insta, rstest, mockall | ~200+ |
| **Apache-2.0** | windows-sys, tauri (全家桶), futures, chrono, dirs, rand, regex, proptest, cargo-metadata | ~200+ |
| **BSD-3-Clause** | alloc-no-stdlib, alloc-stdlib, brotli-decompressor, subtle | ~10 |
| **Zlib / Zlib-like** | foldhash, miniz_oxide, bytemuck, tinyvec, zune-jpeg, lru-slab | ~15 |
| **ISC** | libloading, untrusted, rustls-webpki | ~3 |
| **Unlicense** | aho-corasick, byteorder-lite, memchr, same-file, walkdir | ~5 |
| **CC0-1.0** | dunce | ~1 |
| **BSL-1.0** | ryu | ~1 |
| **CDLA-Permissive-2.0** | webpki-root-certs, webpki-roots | ~3 |
| **Apache-2.0 WITH LLVM-exception** | linux-raw-sys, wasi, wasm-encoder, wit-bindgen 系列 | ~12 |

> 注：多数 crate 提供多协议选择（如 `Apache-2.0 OR MIT`），可任选其一满足合规要求。

---

## 分发建议

1. **保留 LICENSE 文件**：在安装目录中附带本仓库根目录的 `LICENSE` 文件
2. **附带第三方声明**：推荐在安装目录或"关于"界面中提供指向本文档的入口
3. **动态链接优先**：如未来引入新的 LGPL 依赖库，优先采用动态链接方式以降低合规复杂度
4. **外部 API 服务条款独立于依赖许可**：rig-core 调用的 AI API（Anthropic Claude 等）受各服务商自身服务条款约束，与本文档无关

---

## 更新方法

依赖变更后重新生成完整列表：

```bash
cargo install cargo-license
cargo license --authors --do-not-bundle > LICENSES.third-party.new.md
```

然后对比差异并更新本文档的合规分析部分。
