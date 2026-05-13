# 8Bit Cat — ai-pad Makefile
#
# 用法:
#   make build      开发构建
#   make release    Release 构建（opt-level=z, LTO, strip）
#   make dist       打包为 ZIP（含 exe + yml 配置）
#   make dist-upx   打包 + UPX 压缩（体积减 40~60%）
#   make test       运行测试（nextest → 回退 cargo test）
#   make run        启动程序
#
# 环境要求: cmake, $env:CMAKE_POLICY_VERSION_MINIMUM="3.5"（SDL2 编译）

.PHONY: build release dist dist-upx test test-core test-app test-fast nextest run check clippy clean \
        install-hooks \
        py-read py-ctl py-test all

export CMAKE_POLICY_VERSION_MINIMUM = 3.5

DEBUG_DIR   = target/debug
RELEASE_DIR = target/release
EXE_NAME    = ai-pad-app.exe

# ══════════════════════════════════════
#  构建
# ══════════════════════════════════════

build:
	cargo build && mkdir -p $(DEBUG_DIR)/config && cp config/*.yml $(DEBUG_DIR)/config/

release:
	cargo build --release && mkdir -p $(RELEASE_DIR)/config && cp config/*.yml $(RELEASE_DIR)/config/

# ══════════════════════════════════════
#  打包：exe + yml → 版本化 ZIP
# ══════════════════════════════════════

VERSION   = $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
DIST_NAME = ai-pad-$(VERSION)-windows-x64

dist: release
	cargo run -p xtask -- package-portable --version "$(VERSION)" --release-dir "$(RELEASE_DIR)" --out-dir "."

dist-upx:
	$(MAKE) release
	cargo run -p xtask -- package-portable --version "$(VERSION)" --release-dir "$(RELEASE_DIR)" --out-dir "." --upx

# ══════════════════════════════════════
#  测试 & 检查
# ══════════════════════════════════════

# 拷贝配置文件到 core/（测试需要这些 yml）
_copy-fixtures:
	@mkdir -p core/config && cp config/*.yml core/config/

# 完整测试：整个 workspace（core + app）。app crate 依赖 SDL2/Tauri，编译较慢。
test: _copy-fixtures
	cargo nextest run --workspace

# 日常快速反馈：只跑 core（~20s），跳过 SDL2/Tauri 编译
test-core: _copy-fixtures
	cargo nextest run -p ai-pad-core

# 只跑 app 测试
test-app: _copy-fixtures
	cargo nextest run -p ai-pad-app

# 最快反馈：core + 跳过 proptest（proptest 默认每块 256 cases）
test-fast: _copy-fixtures
	PROPTEST_CASES=32 cargo nextest run -p ai-pad-core -E 'not test(/prop_/)'

nextest: test

# 手动触发 cargo-husky 安装 git hooks（pre-commit / pre-push）
# 原理：cargo-husky 的 build.rs 在 cargo test 时写入 .git/hooks/
# 脚本源在 .cargo-husky/hooks/。跳过安装：CARGO_HUSKY_DONT_INSTALL_HOOKS=true
install-hooks:
	@cargo test -p ai-pad-core --no-run --quiet
	@echo 'Git hooks 已安装到 .git/hooks/（pre-commit + pre-push）'

check:
	cargo check

clippy:
	cargo clippy -- -W clippy::all

run:
	cargo run

# ══════════════════════════════════════
#  清理
# ══════════════════════════════════════

clean:
	cargo clean
	rm -f ai-pad-*.zip

# ══════════════════════════════════════
#  Python（预留）
# ══════════════════════════════════════

py-read:
	uv run python -m ai_pad.reader

py-ctl:
	uv run python -m ai_pad.ctl

py-test:
	uv run pytest -v

all: test build
