# ai-pad Makefile
# 需要: cmake (pip install cmake), $env:CMAKE_POLICY_VERSION_MINIMUM="3.5"
#
# 常用命令:
#   make read     - 按键测试
#   make ctl      - 启动控制器
#   make test     - 运行测试
#   make build    - 构建
#   make clean    - 清理

export CMAKE_POLICY_VERSION_MINIMUM = 3.5

.PHONY: build test read ctl release check clippy clean \
        py-read py-ctl py-test all

# ---- Rust ----

build:
	cargo build
	cp buttons.yml target/debug/ 2>/dev/null || true
	cp actions.yml target/debug/ 2>/dev/null || true

release:
	cargo build --release
	cp buttons.yml target/release/
	cp actions.yml target/release/

test:
	cp buttons.yml actions.yml core/ 2>/dev/null || true
	cargo test

read:
	cargo run

ctl:
	cargo run

check:
	cargo check

clippy:
	cargo clippy -- -W clippy::all

clean:
	cargo clean

# ---- Python ----

py-read:
	uv run python -m ai_pad.reader

py-ctl:
	uv run python -m ai_pad.ctl

py-test:
	uv run pytest -v

# ---- 组合 ----

all: test build
