# BitCat Makefile
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
        py-test all

export CMAKE_POLICY_VERSION_MINIMUM = 3.5

DEBUG_DIR   = target/debug
RELEASE_DIR = target/release
EXE_NAME    = bitcat.exe

# ══════════════════════════════════════
#  构建
# ══════════════════════════════════════

build:
	cargo run -p xtask -- prepare-frontend
	cargo build
	cargo run -p xtask -- prepare-exe --out-dir "$(DEBUG_DIR)"
	cargo run -p xtask -- copy-config --out-dir "$(DEBUG_DIR)"

release:
	cargo run -p xtask -- prepare-frontend
	cargo build --release
	cargo run -p xtask -- prepare-exe --out-dir "$(RELEASE_DIR)"
	cargo run -p xtask -- copy-config --out-dir "$(RELEASE_DIR)"

# ══════════════════════════════════════
#  打包：exe + yml → 版本化 ZIP
# ══════════════════════════════════════

VERSION   = $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
DIST_NAME = bitcat-$(VERSION)-windows-x64

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
	@cargo run -p xtask -- copy-config --out-dir core

# 完整测试：整个 workspace（core + app）。app crate 依赖 SDL2/Tauri，编译较慢。
test:
	cargo run -p xtask -- test

# 日常快速反馈：只跑 core（~20s），跳过 SDL2/Tauri 编译
test-core:
	cargo run -p xtask -- test-core

# 只跑 app 测试
test-app:
	cargo run -p xtask -- test-app

# 最快反馈：core + 跳过 proptest（proptest 默认每块 256 cases）
test-fast:
	cargo run -p xtask -- test-fast

nextest: test

# 手动触发 cargo-husky 安装 git hooks（pre-commit / pre-push）
# 原理：cargo-husky 的 build.rs 在 cargo test 时写入 .git/hooks/
# 脚本源在 .cargo-husky/hooks/。跳过安装：CARGO_HUSKY_DONT_INSTALL_HOOKS=true
install-hooks:
	@cargo test -p bitcat-core --no-run --quiet
	@echo 'Git hooks 已安装到 .git/hooks/（pre-commit + pre-push）'

check:
	cargo check

clippy:
	cargo clippy -- -W clippy::all

run:
	cargo run -p bitcat-app --bin bitcat

# ══════════════════════════════════════
#  清理
# ══════════════════════════════════════

clean:
	cargo clean
	cargo run -p xtask -- clean-dist

# ══════════════════════════════════════
#  Python（预留）
# ══════════════════════════════════════

py-test:
	uv run pytest -v

all: test build
