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

.PHONY: build release dist dist-upx test nextest run read check clippy clean \
        py-read py-ctl py-test all

export CMAKE_POLICY_VERSION_MINIMUM = 3.5

DEBUG_DIR   = target/debug
RELEASE_DIR = target/release
EXE_NAME    = ai-pad-app.exe

# ══════════════════════════════════════
#  构建
# ══════════════════════════════════════

build:
	cargo build && cp buttons.yml actions.yml prompts.yml $(DEBUG_DIR)/

release:
	cargo build --release && cp buttons.yml actions.yml prompts.yml $(RELEASE_DIR)/

# ══════════════════════════════════════
#  打包：exe + yml → 版本化 ZIP
# ══════════════════════════════════════

VERSION   = $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
DIST_NAME = ai-pad-$(VERSION)-windows-x64

dist: release
	@rm -f $(DIST_NAME).zip && mkdir -p $(DIST_NAME) \
	  && cp $(RELEASE_DIR)/$(EXE_NAME) buttons.yml actions.yml prompts.yml $(DIST_NAME)/ \
	  && powershell -c "Compress-Archive -Path $(DIST_NAME)/* -DestinationPath $(DIST_NAME).zip" \
	  && echo "Done: $$(du -sh $(DIST_NAME).zip | cut -f1)" \
	  && rm -rf $(DIST_NAME)

dist-upx:
	$(MAKE) release
	@which upx > /dev/null || (echo "UPX not found: winget install UPX.UPX" && false)
	upx --best --lzma $(RELEASE_DIR)/$(EXE_NAME)
	@rm -f $(DIST_NAME).zip && mkdir -p $(DIST_NAME) \
	  && cp $(RELEASE_DIR)/$(EXE_NAME) buttons.yml actions.yml prompts.yml $(DIST_NAME)/ \
	  && powershell -c "Compress-Archive -Path $(DIST_NAME)/* -DestinationPath $(DIST_NAME).zip" \
	  && echo "Done: $$(du -sh $(DIST_NAME).zip | cut -f1)" \
	  && rm -rf $(DIST_NAME)

# ══════════════════════════════════════
#  测试 & 检查
# ══════════════════════════════════════

test:
	cp buttons.yml actions.yml prompts.yml core/ && cargo nextest run --workspace || cargo test --workspace

nextest:
	cp buttons.yml actions.yml prompts.yml core/ && cargo nextest run --workspace

check:
	cargo check

clippy:
	cargo clippy -- -W clippy::all

run read:
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
