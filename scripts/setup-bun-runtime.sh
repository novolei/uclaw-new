#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# setup-bun-runtime.sh
# 下载 Bun 静态二进制到 src-tauri/bunembed/，供 Tauri build 打包成 resource。
# 运行时 uclaw_core 会 spawn 这个 bun 作为 stdio MCP 子进程驱动 gbrain。
#
# Sprint 2.0 of the gbrain integration track (Path C-2). 与 setup-python-env.sh
# 是姊妹脚本：python-env 处理 memU bridge，这个脚本处理 gbrain runtime。
# =============================================================================

# --- 颜色定义 ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
step()    { echo -e "\n${CYAN}${BOLD}▶ $*${NC}"; }

# --- 路径 ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUNEMBED_DIR="${PROJECT_DIR}/src-tauri/bunembed"
BUN_BIN="${BUNEMBED_DIR}/bun"

# --- 默认版本 ---
# Bun 的 release 节奏快，pin 一个已知稳定版作为后备。
# 实际下载会先尝试 GitHub API 拿最新；失败回退到这个值。
# 可通过 BUN_VERSION 环境变量覆盖。
DEFAULT_BUN_VERSION="${BUN_VERSION:-1.1.42}"

# --- 参数解析 ---
OPT_HELP=false
OPT_CLEAN=false
OPT_YES=false

usage() {
    local cmd
    cmd=$(basename "$0")
    cat <<EOF
${BOLD}用法:${NC} ${cmd} [选项]

下载 Bun 静态二进制到 src-tauri/bunembed/，供 Tauri 打包。

${BOLD}选项:${NC}
  --help        显示此帮助
  --clean       删除 src-tauri/bunembed/ 后退出
  --yes / -y    所有交互确认默认 yes（CI 用）

${BOLD}环境变量:${NC}
  BUN_VERSION   覆盖 Bun 版本（默认: ${DEFAULT_BUN_VERSION}）

${BOLD}产出:${NC}
  src-tauri/bunembed/bun  (~50MB 单文件可执行)

${BOLD}Tauri config:${NC}
  确认 src-tauri/tauri.conf.json 的 bundle.resources 包含:
  "bunembed/bun": "bun"
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)  OPT_HELP=true ;;
        --clean) OPT_CLEAN=true ;;
        --yes|-y) OPT_YES=true ;;
        *) error "未知选项: $1"; usage; exit 1 ;;
    esac
    shift
done

if $OPT_HELP; then usage; exit 0; fi

confirm() {
    if $OPT_YES; then return 0; fi
    local prompt="$1 [y/N] "
    read -r -p "$(echo -e "${YELLOW}${prompt}${NC}")" answer
    case "$answer" in
        [yY][eE][sS]|[yY]) return 0 ;;
        *) return 1 ;;
    esac
}

dir_size_mb() {
    if [[ -d "$1" ]]; then
        du -sm "$1" 2>/dev/null | awk '{print $1}'
    else
        echo "0"
    fi
}

do_clean() {
    step "清理 bunembed 目录"
    if [[ -d "${BUNEMBED_DIR}" ]]; then
        local size
        size=$(dir_size_mb "${BUNEMBED_DIR}")
        info "当前 bunembed 目录大小: ${size} MB"
        if confirm "确认删除 ${BUNEMBED_DIR} ?"; then
            rm -rf "${BUNEMBED_DIR}"
            success "已删除 bunembed 目录"
        else
            warn "取消清理"
        fi
    else
        info "bunembed 目录不存在，无需清理"
    fi
}

if $OPT_CLEAN; then
    do_clean
    exit 0
fi

# =============================================================================
# 主流程
# =============================================================================

step "检测平台"
OS="$(uname -s)"
ARCH="$(uname -m)"

# Bun 的 release 文件名约定:
#   bun-darwin-aarch64.zip    (macOS Apple Silicon)
#   bun-darwin-x64.zip        (macOS Intel)
#   bun-linux-aarch64.zip     (Linux arm64)
#   bun-linux-x64.zip         (Linux x86_64)
#   bun-windows-x64.zip       (Windows — 暂未支持)
case "${OS}" in
    Darwin)
        case "${ARCH}" in
            arm64)  PLATFORM_TAG="darwin-aarch64" ;;
            x86_64) PLATFORM_TAG="darwin-x64" ;;
            *) error "不支持的 macOS 架构: ${ARCH}"; exit 1 ;;
        esac
        ;;
    Linux)
        case "${ARCH}" in
            aarch64|arm64) PLATFORM_TAG="linux-aarch64" ;;
            x86_64)        PLATFORM_TAG="linux-x64" ;;
            *) error "不支持的 Linux 架构: ${ARCH}"; exit 1 ;;
        esac
        ;;
    *)
        error "不支持的操作系统: ${OS} (Windows 暂未支持)"
        exit 1
        ;;
esac

info "OS: ${OS}, ARCH: ${ARCH} → bun-${PLATFORM_TAG}"

# --- 确定版本 ---
step "确定 Bun 版本"

BUN_VERSION_RESOLVED=""
if command -v curl &>/dev/null; then
    info "查询 GitHub API 获取最新 Bun release..."
    API_RESPONSE=$(curl -sf --connect-timeout 10 --max-time 15 \
        "https://api.github.com/repos/oven-sh/bun/releases/latest" 2>/dev/null || true)
    if [[ -n "${API_RESPONSE}" ]]; then
        # tag 形如 "bun-v1.1.42"，提取 "1.1.42"
        BUN_VERSION_RESOLVED=$(echo "${API_RESPONSE}" \
            | grep -o '"tag_name":"bun-v[^"]*"' \
            | head -1 \
            | sed 's/.*"bun-v\([^"]*\)"/\1/' || true)
    fi
fi

if [[ -z "${BUN_VERSION_RESOLVED}" ]]; then
    BUN_VERSION_RESOLVED="${DEFAULT_BUN_VERSION}"
    warn "无法从 GitHub API 解析版本，使用默认: ${BUN_VERSION_RESOLVED}"
fi

info "Bun 版本: ${BUN_VERSION_RESOLVED}"

# --- 下载 URL ---
FILENAME="bun-${PLATFORM_TAG}.zip"
DOWNLOAD_URL="https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION_RESOLVED}/${FILENAME}"
TMP_FILE="/tmp/${FILENAME}"

info "下载 URL: ${DOWNLOAD_URL}"

# --- 已有安装检查 ---
SKIP_DOWNLOAD=false
if [[ -x "${BUN_BIN}" ]]; then
    local_size=$(dir_size_mb "${BUNEMBED_DIR}")
    INSTALLED_VERSION=$("${BUN_BIN}" --version 2>/dev/null || echo "unknown")
    warn "bunembed 已存在 (${local_size} MB, version: ${INSTALLED_VERSION})"
    if confirm "是否覆盖现有安装？"; then
        SKIP_DOWNLOAD=false
    else
        info "保留现有安装，退出"
        exit 0
    fi
fi

# --- 下载 ---
if ! ${SKIP_DOWNLOAD}; then
    step "下载 Bun"

    if [[ -f "${TMP_FILE}" ]]; then
        info "发现已缓存文件: ${TMP_FILE}"
        if confirm "复用？（选 N 重下载）"; then
            info "使用缓存"
        else
            rm -f "${TMP_FILE}"
        fi
    fi

    if [[ ! -f "${TMP_FILE}" ]]; then
        info "下载到 ${TMP_FILE}..."
        if ! curl -L --fail --progress-bar -o "${TMP_FILE}" "${DOWNLOAD_URL}"; then
            error "下载失败！URL: ${DOWNLOAD_URL}"
            rm -f "${TMP_FILE}"
            exit 1
        fi
        success "下载完成 ($(du -h "${TMP_FILE}" | awk '{print $1}'))"
    fi

    # --- 解压 ---
    step "解压到 ${BUNEMBED_DIR}"

    if [[ -d "${BUNEMBED_DIR}" ]]; then
        rm -rf "${BUNEMBED_DIR}"
    fi
    mkdir -p "${BUNEMBED_DIR}"

    # Bun zip 解压后形如 bun-${PLATFORM_TAG}/bun，移到 bunembed/bun
    TMP_EXTRACT="/tmp/bunembed-extract-$$"
    mkdir -p "${TMP_EXTRACT}"
    if command -v unzip &>/dev/null; then
        unzip -q "${TMP_FILE}" -d "${TMP_EXTRACT}"
    else
        error "未找到 unzip。请安装：brew install unzip 或 apt-get install unzip"
        rm -rf "${TMP_EXTRACT}"
        exit 1
    fi

    # 找解压出来的 bun 二进制
    EXTRACTED_BUN=$(find "${TMP_EXTRACT}" -name "bun" -type f | head -1)
    if [[ -z "${EXTRACTED_BUN}" ]]; then
        error "解压后未找到 bun 二进制"
        find "${TMP_EXTRACT}" | head -20
        rm -rf "${TMP_EXTRACT}"
        exit 1
    fi

    cp "${EXTRACTED_BUN}" "${BUN_BIN}"
    chmod +x "${BUN_BIN}"
    rm -rf "${TMP_EXTRACT}"
    success "Bun 已安装: ${BUN_BIN}"

    # --- 验证 ---
    BUN_ACTUAL_VERSION=$("${BUN_BIN}" --version 2>&1)
    success "Bun 版本验证: v${BUN_ACTUAL_VERSION}"
fi

# --- 提醒 tauri.conf.json ---
step "Tauri config 检查"
TAURI_CONF="${PROJECT_DIR}/src-tauri/tauri.conf.json"
if grep -q '"bunembed/bun"' "${TAURI_CONF}" 2>/dev/null; then
    success "tauri.conf.json 已包含 bunembed/bun resource"
else
    warn "tauri.conf.json 似乎未声明 bunembed/bun resource"
    warn "请确认 bundle.resources 包含:"
    echo '    "bunembed/bun": "bun"'
fi

# --- 总结 ---
step "完成"
SIZE_MB=$(dir_size_mb "${BUNEMBED_DIR}")
success "Bun runtime 安装完成 (${SIZE_MB} MB)"
info "下一步: 运行 scripts/setup-gbrain-source.sh 拉取 gbrain 源码"
