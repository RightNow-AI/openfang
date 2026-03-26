#!/usr/bin/env bash
# Build and install telegram-bot-api from the vendored third_party source.

set -euo pipefail

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_DIR="$REPO_ROOT/third_party/telegram-bot-api"
TD_SOURCE_DIR="$SOURCE_DIR/td"
BUILD_DIR="$SOURCE_DIR/build-openfang"
INSTALL_DIR="$HOME/.openfang/bin"
OUTPUT_BIN="$INSTALL_DIR/telegram-bot-api"

echo "=== 安装 telegram-bot-api（仓库内 third_party 源码） ==="
echo ""

if [ ! -d "$SOURCE_DIR" ]; then
    echo "❌ 未找到源码目录：$SOURCE_DIR" >&2
    echo "请先执行：git submodule update --init --recursive third_party/telegram-bot-api" >&2
    exit 1
fi

if [ ! -f "$SOURCE_DIR/CMakeLists.txt" ]; then
    echo "❌ third_party/telegram-bot-api 不完整，缺少 CMakeLists.txt" >&2
    echo "请先执行：git submodule update --init --recursive third_party/telegram-bot-api" >&2
    exit 1
fi

if [ ! -f "$TD_SOURCE_DIR/CMakeLists.txt" ]; then
    if command -v git >/dev/null 2>&1 && [ -e "$SOURCE_DIR/.git" ]; then
        echo "缺少 td 子模块，尝试自动初始化..."
        git -C "$SOURCE_DIR" submodule update --init --recursive
    fi
fi

if [ ! -f "$TD_SOURCE_DIR/CMakeLists.txt" ]; then
    echo "❌ third_party/telegram-bot-api 缺少 td 子模块源码" >&2
    echo "请先执行：git submodule update --init --recursive third_party/telegram-bot-api" >&2
    exit 1
fi

if ! command -v cmake >/dev/null 2>&1; then
    echo "❌ 缺少 cmake，无法编译 telegram-bot-api" >&2
    exit 1
fi

JOBS=4
if command -v nproc >/dev/null 2>&1; then
    JOBS="$(nproc 2>/dev/null || echo 4)"
elif command -v sysctl >/dev/null 2>&1; then
    JOBS="$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"
fi

mkdir -p "$INSTALL_DIR"

echo "源码目录: $SOURCE_DIR"
echo "构建目录: $BUILD_DIR"
echo "安装目录: $OUTPUT_BIN"
echo ""

cmake -S "$SOURCE_DIR" -B "$BUILD_DIR" -DCMAKE_BUILD_TYPE=Release
cmake --build "$BUILD_DIR" --target telegram-bot-api -j "$JOBS"

BUILT_BIN="$(find "$BUILD_DIR" -type f -perm -111 -name 'telegram-bot-api' | head -1)"
if [ -z "$BUILT_BIN" ]; then
    echo "❌ 编译完成，但未找到 telegram-bot-api 可执行文件" >&2
    exit 1
fi

cp "$BUILT_BIN" "$OUTPUT_BIN"
chmod +x "$OUTPUT_BIN"

echo ""
echo "✅ 已安装到：$OUTPUT_BIN"
echo "下一步：重启 OpenFang，使 Local Bot API 自动启动配置生效。"
