#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# setup-python-env.sh
# 自动下载 python-build-standalone 并安装 memU 依赖
# =============================================================================

# --- 颜色定义 ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# --- 日志函数 ---
info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
step()  { echo -e "\n${CYAN}${BOLD}▶ $*${NC}"; }

# --- 路径计算 ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PYEMBED_DIR="${PROJECT_DIR}/src-tauri/pyembed"
PYTHON_BIN="${PYEMBED_DIR}/python/bin/python3"
MEMU_LOCAL_DIR="${HOME}/Documents/memU"

# --- 默认版本（硬编码已知稳定版，可通过 API 动态获取） ---
DEFAULT_RELEASE_TAG="20260414"
DEFAULT_PYTHON_VERSION="3.13.13"

# --- 参数解析 ---
OPT_HELP=false
OPT_CLEAN=false
OPT_OPTIMIZE=false
OPT_YES=false

usage() {
    local cmd
    cmd=$(basename "$0")
    echo -e "${BOLD}用法:${NC} ${cmd} [选项]"
    echo ""
    echo "自动下载 python-build-standalone 并安装 memU 依赖。"
    echo ""
    echo -e "${BOLD}选项:${NC}"
    echo "  --help        显示此帮助信息"
    echo "  --clean       仅清理已下载的 Python 环境（删除 pyembed 目录）"
    echo "  --optimize    安装完成后执行体积优化（删除缓存、测试文件等）"
    echo "  -y, --yes     跳过所有确认提示，自动确认"
    echo ""
    echo -e "${BOLD}示例:${NC}"
    echo "  ${cmd}                # 下载 Python + 安装 memU"
    echo "  ${cmd} --optimize     # 下载 Python + 安装 memU + 体积优化"
    echo "  ${cmd} --clean        # 仅清理 pyembed 目录"
    echo "  ${cmd} -y             # 无交互模式"
    echo ""
    echo -e "${BOLD}路径:${NC}"
    echo "  Python 安装位置: ${PYEMBED_DIR}/python/"
    echo "  脚本位置:        ${SCRIPT_DIR}/"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)     OPT_HELP=true; shift ;;
        --clean)    OPT_CLEAN=true; shift ;;
        --optimize) OPT_OPTIMIZE=true; shift ;;
        -y|--yes)   OPT_YES=true; shift ;;
        *)
            error "未知参数: $1"
            usage
            exit 1
            ;;
    esac
done

if $OPT_HELP; then
    usage
    exit 0
fi

# --- 工具函数 ---
confirm() {
    if $OPT_YES; then
        return 0
    fi
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

# --- 清理功能 ---
do_clean() {
    step "清理 pyembed 目录"
    if [[ -d "${PYEMBED_DIR}" ]]; then
        local size
        size=$(dir_size_mb "${PYEMBED_DIR}")
        info "当前 pyembed 目录大小: ${size} MB"
        if confirm "确认删除 ${PYEMBED_DIR} ?"; then
            rm -rf "${PYEMBED_DIR}"
            success "已删除 pyembed 目录"
        else
            warn "取消清理"
        fi
    else
        info "pyembed 目录不存在，无需清理"
    fi
}

if $OPT_CLEAN; then
    do_clean
    exit 0
fi

# =============================================================================
# 主流程
# =============================================================================

step "检测平台信息"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Darwin) OS_TAG="apple-darwin" ;;
    Linux)  OS_TAG="unknown-linux-gnu" ;;
    *)
        error "不支持的操作系统: ${OS}"
        exit 1
        ;;
esac

case "${ARCH}" in
    arm64|aarch64) ARCH_TAG="aarch64" ;;
    x86_64)        ARCH_TAG="x86_64" ;;
    *)
        error "不支持的 CPU 架构: ${ARCH}"
        exit 1
        ;;
esac

info "操作系统: ${OS} (${OS_TAG})"
info "CPU 架构: ${ARCH} (${ARCH_TAG})"

# --- 获取最新 release tag（带回退） ---
step "确定 python-build-standalone 版本"

RELEASE_TAG=""
PYTHON_VERSION=""

# 尝试从 GitHub API 获取最新版本
if command -v curl &>/dev/null; then
    info "正在查询 GitHub API 获取最新 release..."
    API_RESPONSE=$(curl -sf --connect-timeout 10 --max-time 15 \
        "https://api.github.com/repos/astral-sh/python-build-standalone/releases/latest" 2>/dev/null || true)

    if [[ -n "${API_RESPONSE}" ]]; then
        RELEASE_TAG=$(echo "${API_RESPONSE}" | grep -o '"tag_name":"[^"]*"' | head -1 | cut -d'"' -f4 || true)

        if [[ -n "${RELEASE_TAG}" ]]; then
            # 从 assets 中提取 3.13 版本号
            PYTHON_VERSION=$(echo "${API_RESPONSE}" | grep -o 'cpython-3\.13\.[0-9]*' | head -1 | sed 's/cpython-//' || true)
        fi
    fi
fi

# 回退到默认值
if [[ -z "${RELEASE_TAG}" ]]; then
    RELEASE_TAG="${DEFAULT_RELEASE_TAG}"
    warn "无法从 GitHub API 获取版本，使用默认值: ${RELEASE_TAG}"
fi

if [[ -z "${PYTHON_VERSION}" ]]; then
    PYTHON_VERSION="${DEFAULT_PYTHON_VERSION}"
fi

info "Release tag: ${RELEASE_TAG}"
info "Python 版本: ${PYTHON_VERSION}"

# --- 构建下载 URL ---
FILENAME="cpython-${PYTHON_VERSION}+${RELEASE_TAG}-${ARCH_TAG}-${OS_TAG}-install_only_stripped.tar.gz"
DOWNLOAD_URL="https://github.com/astral-sh/python-build-standalone/releases/download/${RELEASE_TAG}/${FILENAME}"
TMP_FILE="/tmp/${FILENAME}"

info "下载 URL: ${DOWNLOAD_URL}"

# --- 检查已有安装 ---
if [[ -d "${PYEMBED_DIR}/python" ]]; then
    warn "pyembed 目录已存在: ${PYEMBED_DIR}"
    local_size=$(dir_size_mb "${PYEMBED_DIR}")
    info "当前目录大小: ${local_size} MB"
    if ! confirm "是否覆盖现有安装？"; then
        info "跳过下载，保留现有安装"
        # 跳到安装 memU 步骤
        SKIP_DOWNLOAD=true
    else
        info "将覆盖现有安装"
        SKIP_DOWNLOAD=false
    fi
else
    SKIP_DOWNLOAD=false
fi

# --- 下载 ---
if ! ${SKIP_DOWNLOAD:-false}; then
    step "下载 python-build-standalone"

    if [[ -f "${TMP_FILE}" ]]; then
        info "发现已下载的文件: ${TMP_FILE}"
        if confirm "是否使用已下载的文件？（选 N 将重新下载）"; then
            info "使用已缓存的文件"
        else
            rm -f "${TMP_FILE}"
        fi
    fi

    if [[ ! -f "${TMP_FILE}" ]]; then
        info "正在下载到 ${TMP_FILE} ..."
        if ! curl -L --fail --progress-bar -o "${TMP_FILE}" "${DOWNLOAD_URL}"; then
            error "下载失败！请检查网络连接或 URL 是否正确"
            error "URL: ${DOWNLOAD_URL}"
            rm -f "${TMP_FILE}"
            exit 1
        fi
        success "下载完成 ($(du -h "${TMP_FILE}" | awk '{print $1}'))"
    fi

    # --- 解压 ---
    step "解压到 ${PYEMBED_DIR}"

    # 如果目录已存在，先删除
    if [[ -d "${PYEMBED_DIR}" ]]; then
        rm -rf "${PYEMBED_DIR}"
    fi

    mkdir -p "${PYEMBED_DIR}"

    info "正在解压..."
    tar xzf "${TMP_FILE}" -C "${PYEMBED_DIR}"
    success "解压完成"

    # 验证解压结果
    if [[ ! -x "${PYTHON_BIN}" ]]; then
        error "解压后未找到 Python 可执行文件: ${PYTHON_BIN}"
        error "目录内容:"
        ls -la "${PYEMBED_DIR}/" || true
        exit 1
    fi

    PYTHON_ACTUAL_VERSION=$("${PYTHON_BIN}" --version 2>&1)
    success "Python 可执行文件验证通过: ${PYTHON_ACTUAL_VERSION}"
fi

# --- 安装 memU 依赖 ---
step "安装 memU 依赖"

# 确保 Python 可用
if [[ ! -x "${PYTHON_BIN}" ]]; then
    error "Python 可执行文件不存在: ${PYTHON_BIN}"
    exit 1
fi

INSTALL_SUCCESS=false

# 优先检查本地 memU 源码
if [[ -d "${MEMU_LOCAL_DIR}" && -f "${MEMU_LOCAL_DIR}/setup.py" || -d "${MEMU_LOCAL_DIR}" && -f "${MEMU_LOCAL_DIR}/pyproject.toml" ]]; then
    info "发现本地 memU 源码: ${MEMU_LOCAL_DIR}"
    info "将从本地源码安装（开发模式）"

    if command -v uv &>/dev/null; then
        info "使用 uv 从本地安装..."
        if uv pip install --python "${PYTHON_BIN}" -e "${MEMU_LOCAL_DIR}"; then
            INSTALL_SUCCESS=true
            success "通过 uv 从本地源码安装成功"
        else
            warn "uv 安装失败，尝试回退到 pip"
        fi
    fi

    if ! $INSTALL_SUCCESS; then
        info "使用 pip 从本地安装..."
        if "${PYTHON_BIN}" -m pip install -e "${MEMU_LOCAL_DIR}"; then
            INSTALL_SUCCESS=true
            success "通过 pip 从本地源码安装成功"
        else
            warn "本地源码安装失败，将尝试从 PyPI 安装"
        fi
    fi
fi

# 如果本地安装未成功，从 PyPI 安装
if ! $INSTALL_SUCCESS; then
    if command -v uv &>/dev/null; then
        info "使用 uv 从 PyPI 安装 memu..."
        if uv pip install --python "${PYTHON_BIN}" memu; then
            INSTALL_SUCCESS=true
            success "通过 uv 从 PyPI 安装成功"
        else
            warn "uv 安装失败，尝试回退到 pip"
        fi
    fi

    if ! $INSTALL_SUCCESS; then
        info "使用 pip 从 PyPI 安装 memu..."
        if "${PYTHON_BIN}" -m pip install memu; then
            INSTALL_SUCCESS=true
            success "通过 pip 从 PyPI 安装成功"
        else
            error "memU 安装失败！"
            error "请手动检查网络连接或包名是否正确"
            exit 1
        fi
    fi
fi

# --- 安装 fastembed ---
step "安装 fastembed（本地 embedding 支持）"

FASTEMBED_INSTALLED=false

if command -v uv &>/dev/null; then
    info "使用 uv 安装 fastembed..."
    if uv pip install --python "${PYTHON_BIN}" fastembed; then
        FASTEMBED_INSTALLED=true
        success "通过 uv 安装 fastembed 成功"
    else
        warn "uv 安装 fastembed 失败，尝试回退到 pip"
    fi
fi

if ! $FASTEMBED_INSTALLED; then
    info "使用 pip 安装 fastembed..."
    if "${PYTHON_BIN}" -m pip install fastembed; then
        FASTEMBED_INSTALLED=true
        success "通过 pip 安装 fastembed 成功"
    else
        warn "fastembed 安装失败，本地 embedding 功能将不可用"
    fi
fi

# --- 验证安装 ---
step "验证 memU 安装"

if "${PYTHON_BIN}" -c "import memu; print('memU version:', memu.__version__)" 2>/dev/null; then
    success "memU 验证通过！"
else
    warn "无法导入 memu 或获取版本号，但包可能已安装"
    info "尝试基本导入测试..."
    if "${PYTHON_BIN}" -c "import memu; print('memU 导入成功')" 2>/dev/null; then
        success "memU 导入成功（无 __version__ 属性）"
    else
        error "memU 导入失败！安装可能不完整"
        exit 1
    fi
fi

# --- 体积优化 ---
if $OPT_OPTIMIZE; then
    step "执行体积优化"

    SIZE_BEFORE=$(dir_size_mb "${PYEMBED_DIR}")
    info "优化前大小: ${SIZE_BEFORE} MB"

    # 删除 __pycache__ 目录
    info "删除 __pycache__ 目录..."
    PYCACHE_COUNT=$(find "${PYEMBED_DIR}" -type d -name "__pycache__" | wc -l | tr -d ' ')
    find "${PYEMBED_DIR}" -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
    info "  已删除 ${PYCACHE_COUNT} 个 __pycache__ 目录"

    # 删除 .pyc 文件（如果有残留）
    info "删除 .pyc 文件..."
    find "${PYEMBED_DIR}" -name "*.pyc" -delete 2>/dev/null || true

    # 删除 .dist-info 中的非必要文件
    info "清理 .dist-info 目录..."
    find "${PYEMBED_DIR}" -path "*/.dist-info/*" \
        ! -name "METADATA" \
        ! -name "RECORD" \
        ! -name "INSTALLER" \
        ! -name "top_level.txt" \
        ! -name "WHEEL" \
        -type f -delete 2>/dev/null || true

    # 删除 tests/ 和 test/ 目录
    info "删除测试目录..."
    TEST_COUNT=$(find "${PYEMBED_DIR}" -type d \( -name "tests" -o -name "test" \) | wc -l | tr -d ' ')
    find "${PYEMBED_DIR}" -type d \( -name "tests" -o -name "test" \) -exec rm -rf {} + 2>/dev/null || true
    info "  已删除 ${TEST_COUNT} 个测试目录"

    # 删除 .pdb 文件（Windows 调试符号，不应存在但以防万一）
    find "${PYEMBED_DIR}" -name "*.pdb" -delete 2>/dev/null || true

    # 删除 idle 和 turtle（通常不需要）
    for dir in idlelib turtledemo turtle; do
        if [[ -d "${PYEMBED_DIR}/python/lib/python${PYTHON_VERSION%.*}/${dir}" ]]; then
            rm -rf "${PYEMBED_DIR}/python/lib/python${PYTHON_VERSION%.*}/${dir}"
            info "  已删除 ${dir}/"
        fi
    done

    SIZE_AFTER=$(dir_size_mb "${PYEMBED_DIR}")
    SAVED=$((SIZE_BEFORE - SIZE_AFTER))

    echo ""
    info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    info "优化前: ${SIZE_BEFORE} MB"
    info "优化后: ${SIZE_AFTER} MB"
    info "节省:   ${SAVED} MB"
    info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    success "体积优化完成！"
fi

# --- 完成 ---
echo ""
echo -e "${GREEN}${BOLD}════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}  Python 环境准备完成！${NC}"
echo -e "${GREEN}${BOLD}════════════════════════════════════════${NC}"
echo ""
info "Python 路径:  ${PYTHON_BIN}"
info "Python 版本:  $("${PYTHON_BIN}" --version 2>&1)"
info "pyembed 大小: $(dir_size_mb "${PYEMBED_DIR}") MB"
echo ""
if $FASTEMBED_INSTALLED; then
    info "fastembed:  已安装"
else
    warn "fastembed:  未安装"
fi
info "使用示例:"
echo "  ${PYTHON_BIN} -c \"import memu; print(memu.__version__)\""
echo ""
