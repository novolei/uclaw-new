#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# setup-gbrain-source.sh
# Clone gbrain 源码到 src-tauri/gbrain-source/，跑 bun install 拉依赖，
# 整个目录会被 Tauri build 打包成 resource。
#
# 运行时 uclaw_core 会 spawn:
#   bunembed/bun gbrain-source/<entry.ts> --stdio
# 作为 stdio MCP 子进程。具体 entry point 由 gbrain 的 release 决定。
#
# Sprint 2.0 of the gbrain integration track (Path C-2).
# =============================================================================

# --- 颜色 ---
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
GBRAIN_DIR="${PROJECT_DIR}/src-tauri/gbrain-source"
BUNEMBED_BIN="${PROJECT_DIR}/src-tauri/bunembed/bun"

# --- 默认配置（可被环境变量覆盖）---
# 仓库默认值：sandbox 没法验证实际仓库的存在/位置 + 是否需要镜像
# 默认指向 garrytan/gbrain（Engines spec line 10 引用的 inspiration）
# Mac 端验证时如果实际仓库地址不同请用 GBRAIN_REPO 覆盖。
DEFAULT_GBRAIN_REPO="${GBRAIN_REPO:-https://github.com/garrytan/gbrain.git}"

# Git ref：garrytan/gbrain 使用 master（不是 main）。
# 用 GBRAIN_REF 环境变量覆盖（e.g. GBRAIN_REF=v0.3.2 或 GBRAIN_REF=main）。
DEFAULT_GBRAIN_REF="${GBRAIN_REF:-master}"

# 是否裁剪 node_modules 中 dev-only 依赖（默认 ON 减小 bundle 体积）。
DEFAULT_PRUNE_DEV="${PRUNE_DEV:-1}"

# --- 参数 ---
OPT_HELP=false
OPT_CLEAN=false
OPT_YES=false
OPT_NO_INSTALL=false

usage() {
    local cmd
    cmd=$(basename "$0")
    cat <<EOF
${BOLD}用法:${NC} ${cmd} [选项]

Clone gbrain 源码 + 用 bun 装依赖。打包进 Tauri resource。

${BOLD}选项:${NC}
  --help          显示此帮助
  --clean         删除 src-tauri/gbrain-source/ 后退出
  --yes / -y      所有交互确认默认 yes（CI 用）
  --no-install    只 clone 源码，不跑 bun install（调试用）

${BOLD}环境变量:${NC}
  GBRAIN_REPO    gbrain 仓库 URL  (默认: ${DEFAULT_GBRAIN_REPO})
  GBRAIN_REF     git ref / branch / tag (默认: ${DEFAULT_GBRAIN_REF})
  PRUNE_DEV      非空时跑 bun install --production (默认: ${DEFAULT_PRUNE_DEV})

${BOLD}前置依赖:${NC}
  - git
  - src-tauri/bunembed/bun  (先跑 scripts/setup-bun-runtime.sh)

${BOLD}产出:${NC}
  src-tauri/gbrain-source/  (~70MB clone + 依赖，去 dev-only 后)

${BOLD}Tauri config:${NC}
  确认 src-tauri/tauri.conf.json 的 bundle.resources 包含:
  "gbrain-source": "gbrain"
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)        OPT_HELP=true ;;
        --clean)       OPT_CLEAN=true ;;
        --yes|-y)      OPT_YES=true ;;
        --no-install)  OPT_NO_INSTALL=true ;;
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
    step "清理 gbrain-source 目录"
    if [[ -d "${GBRAIN_DIR}" ]]; then
        local size
        size=$(dir_size_mb "${GBRAIN_DIR}")
        info "当前 gbrain-source 目录大小: ${size} MB"
        if confirm "确认删除 ${GBRAIN_DIR} ?"; then
            rm -rf "${GBRAIN_DIR}"
            success "已删除"
        else
            warn "取消"
        fi
    else
        info "gbrain-source 目录不存在，无需清理"
    fi
}

if $OPT_CLEAN; then
    do_clean
    exit 0
fi

# =============================================================================
# 主流程
# =============================================================================

step "前置依赖检查"

# git
if ! command -v git &>/dev/null; then
    error "未找到 git"
    exit 1
fi
info "git: $(git --version)"

# bun（来自 bunembed/）
if [[ ! -x "${BUNEMBED_BIN}" ]]; then
    error "未找到 bunembed/bun: ${BUNEMBED_BIN}"
    error "请先运行: scripts/setup-bun-runtime.sh"
    exit 1
fi
BUN_VERSION=$("${BUNEMBED_BIN}" --version 2>&1)
info "Bun: v${BUN_VERSION} (来自 bunembed/)"

# --- 已有 clone 检查 ---
step "检查已有 clone"
SKIP_CLONE=false
if [[ -d "${GBRAIN_DIR}/.git" ]]; then
    CURRENT_REMOTE=$(cd "${GBRAIN_DIR}" && git config --get remote.origin.url 2>/dev/null || echo "?")
    CURRENT_REF=$(cd "${GBRAIN_DIR}" && git rev-parse --short HEAD 2>/dev/null || echo "?")
    local_size=$(dir_size_mb "${GBRAIN_DIR}")
    warn "gbrain-source 已存在 (${local_size} MB)"
    info "  remote: ${CURRENT_REMOTE}"
    info "  HEAD:   ${CURRENT_REF}"
    info "  目标 repo: ${DEFAULT_GBRAIN_REPO}"
    info "  目标 ref:  ${DEFAULT_GBRAIN_REF}"
    if confirm "覆盖现有 clone？"; then
        rm -rf "${GBRAIN_DIR}"
    else
        info "复用现有 clone"
        SKIP_CLONE=true
    fi
fi

# --- Clone ---
if ! ${SKIP_CLONE}; then
    step "Clone gbrain"
    info "Repo: ${DEFAULT_GBRAIN_REPO}"
    info "Ref:  ${DEFAULT_GBRAIN_REF}"

    # --depth 1 减体积；--single-branch 避免拉所有分支。
    if ! git clone --depth 1 --single-branch --branch "${DEFAULT_GBRAIN_REF}" \
            "${DEFAULT_GBRAIN_REPO}" "${GBRAIN_DIR}" 2>&1; then
        # branch 不存在时回退：先 clone main 然后 checkout 指定 ref
        # （对 tag 比较有用 — git clone --branch 也支持 tag，但 --single-branch
        # 跟 detached HEAD 偶尔行为差异）
        warn "shallow clone with --branch=${DEFAULT_GBRAIN_REF} 失败，回退到 full-clone-then-checkout"
        rm -rf "${GBRAIN_DIR}"
        git clone --depth 1 "${DEFAULT_GBRAIN_REPO}" "${GBRAIN_DIR}"
        (cd "${GBRAIN_DIR}" && git checkout "${DEFAULT_GBRAIN_REF}")
    fi

    # 去 .git 减体积（运行时不需要）
    rm -rf "${GBRAIN_DIR}/.git"
    success "Clone 完成: $(dir_size_mb "${GBRAIN_DIR}") MB"
fi

# --- 安装依赖 ---
if $OPT_NO_INSTALL; then
    warn "--no-install 指定，跳过 bun install"
else
    step "bun install"
    cd "${GBRAIN_DIR}"

    if [[ ! -f "package.json" ]]; then
        error "${GBRAIN_DIR}/package.json 不存在 — clone 出来的不像 Bun 项目？"
        error "目录内容:"
        ls -la "${GBRAIN_DIR}" | head -20
        exit 1
    fi

    # Bun 1.1.x 不认识 lockfileVersion 1（Bun 1.2+ 格式）。
    # 删掉 lockfile 让 bun 直接从 package.json 解析，避免 InvalidLockfileVersion。
    if [[ -f "bun.lock" ]]; then
        lock_ver=$(head -3 bun.lock 2>/dev/null | grep -o '"lockfileVersion":[[:space:]]*[0-9]*' | grep -o '[0-9]*$' || echo "0")
        if [[ "$lock_ver" -ge 1 ]]; then
            warn "bun.lock lockfileVersion=${lock_ver} 超过 Bun 1.1.x 支持范围，已删除（将从 package.json 重新解析）"
            rm -f bun.lock
        fi
    fi
    rm -f bun.lockb  # 二进制格式同理

    if [[ -n "${DEFAULT_PRUNE_DEV}" && "${DEFAULT_PRUNE_DEV}" != "0" ]]; then
        info "运行 bun install --production (跳过 devDependencies)"
        if ! "${BUNEMBED_BIN}" install --production --no-summary; then
            error "bun install --production 失败"
            cd - >/dev/null
            exit 1
        fi
    else
        info "运行 bun install (含 devDependencies)"
        if ! "${BUNEMBED_BIN}" install --no-summary; then
            error "bun install 失败"
            cd - >/dev/null
            exit 1
        fi
    fi
    cd - >/dev/null
    success "依赖安装完成: $(dir_size_mb "${GBRAIN_DIR}") MB"
fi

# --- PGlite WASM 探测 ---
# PGlite (gbrain 的存储后端) 是 PostgreSQL 编译成的 WASM。
# Sprint 0 验证 Path C-1 时,bun build --compile 因为找不到 WASM 文件而崩。
# Path C-2 不 compile,WASM 跟 node_modules 一起被 Tauri resource 拷进去,
# 路径在 node_modules/@electric-sql/pglite/dist/*.wasm。
#
# 这里做一个 sanity check:
#  - 如果 gbrain 真依赖 PGlite,它应该在 node_modules 里
#  - 找到 WASM 文件,记下相对路径,后续 Sprint 2.1 spawn 时确认 cwd 设对
step "PGlite WASM 检查"
PGLITE_DIR="${GBRAIN_DIR}/node_modules/@electric-sql/pglite"
if [[ -d "${PGLITE_DIR}" ]]; then
    PGLITE_VERSION=$(grep -o '"version":"[^"]*"' "${PGLITE_DIR}/package.json" 2>/dev/null | head -1 | cut -d'"' -f4 || echo "?")
    info "PGlite 版本: ${PGLITE_VERSION}"
    WASM_FILES=$(find "${PGLITE_DIR}" -name "*.wasm" -type f 2>/dev/null)
    if [[ -n "${WASM_FILES}" ]]; then
        info "WASM blobs:"
        while IFS= read -r wasm; do
            rel="${wasm#${GBRAIN_DIR}/}"
            size=$(du -h "${wasm}" 2>/dev/null | awk '{print $1}')
            info "  ${rel} (${size})"
        done <<< "${WASM_FILES}"
        success "PGlite WASM 就位 — Tauri resource 会跟着 gbrain-source 一起打包"
    else
        warn "PGlite 目录存在但未找到 *.wasm — 检查 dist/ 目录:"
        ls "${PGLITE_DIR}/dist" 2>/dev/null | head -10 || true
        warn "gbrain 启动时可能 ENOENT — Sprint 2.1 前必须搞清楚"
    fi
else
    info "未找到 @electric-sql/pglite 依赖 — gbrain 可能不用 PGlite 或换了存储"
    info "(如果你确认 gbrain 应该用 PGlite,检查 ${GBRAIN_DIR}/package.json 的 dependencies)"
fi

# --- Entry-point 探测 ---
step "Entry point 探测"
ENTRY_CANDIDATES=(
    "src/index.ts"
    "src/main.ts"
    "src/cli.ts"
    "src/mcp.ts"
    "src/server.ts"
    "index.ts"
    "main.ts"
)
FOUND_ENTRY=""
for ep in "${ENTRY_CANDIDATES[@]}"; do
    if [[ -f "${GBRAIN_DIR}/${ep}" ]]; then
        FOUND_ENTRY="${ep}"
        break
    fi
done
if [[ -n "${FOUND_ENTRY}" ]]; then
    info "找到可能的 entry: gbrain-source/${FOUND_ENTRY}"
    info "Sprint 2.1 在 mcp_servers.json 默认配置里需要用到这个路径"
else
    warn "未在常见位置找到 entry point — Sprint 2.1 时需要手动确定"
    warn "目录顶层文件:"
    ls "${GBRAIN_DIR}" | head -20
fi

# --- Tauri config 检查 ---
step "Tauri config 检查"
TAURI_CONF="${PROJECT_DIR}/src-tauri/tauri.conf.json"
if grep -q '"gbrain-source"' "${TAURI_CONF}" 2>/dev/null; then
    success "tauri.conf.json 已声明 gbrain-source resource"
else
    warn "tauri.conf.json 似乎未声明 gbrain-source resource"
    warn "请确认 bundle.resources 包含:"
    echo '    "gbrain-source": "gbrain"'
fi

# --- 总结 ---
step "完成"
FINAL_SIZE=$(dir_size_mb "${GBRAIN_DIR}")
success "gbrain 源码已就位 (${FINAL_SIZE} MB)"
info ""
info "下一步:"
info "  1. cargo tauri dev   — 验证 bundle 时能找到 resources"
info "  2. Sprint 2.1 — 在 mcp_servers.json 里加 gbrain stdio MCP 默认配置"
