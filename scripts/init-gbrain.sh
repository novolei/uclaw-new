#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# init-gbrain.sh
# 对 ~/.uclaw/gbrain/ 跑 `gbrain init --pglite --yes`，初始化 PGLite brain。
#
# Sprint 2.2 of the gbrain integration track. 三种典型用途：
#   1. Power-user 手动初始化（不想等 uClaw 自动跑）
#   2. 验证 fresh-init 是否能跑通（开发/CI）
#   3. --force reset 已存在的 brain，从零开始
#
# 前提：scripts/setup-bun-runtime.sh + scripts/setup-gbrain-source.sh
# 已经跑过（src-tauri/bunembed/bun + src-tauri/gbrain-source/ 都在）。
#
# 正常 boot 路径在 uClaw 启动时已经自动跑这个（PR #205），所以一般
# 用户不需要手动调用。
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
BUN_BIN="${PROJECT_DIR}/src-tauri/bunembed/bun"
GBRAIN_ENTRY="${PROJECT_DIR}/src-tauri/gbrain-source/src/cli.ts"
GBRAIN_HOME="${HOME}/.uclaw/gbrain"
BRAIN_DIR="${GBRAIN_HOME}/.gbrain/brain.pglite"
PG_VERSION="${BRAIN_DIR}/PG_VERSION"

# --- 参数解析 ---
OPT_HELP=false
OPT_FORCE=false
OPT_YES=false

usage() {
    local cmd
    cmd=$(basename "$0")
    cat <<EOF
${BOLD}用法:${NC} ${cmd} [选项]

初始化 ~/.uclaw/gbrain/ 下的 PGLite brain（运行 gbrain init --pglite --yes）。

${BOLD}选项:${NC}
  --help        显示此帮助
  --force       即使 brain 已初始化也重新初始化（删除 .gbrain/brain.pglite 后重建）
  --yes / -y    所有交互确认默认 yes（CI 用）

${BOLD}前提:${NC}
  ${PROJECT_DIR}/src-tauri/bunembed/bun    (运行 scripts/setup-bun-runtime.sh)
  ${PROJECT_DIR}/src-tauri/gbrain-source/  (运行 scripts/setup-gbrain-source.sh)

${BOLD}产出:${NC}
  ${BRAIN_DIR}/PG_VERSION  (PGLite 数据目录，63 migrations 跑完)
  ${GBRAIN_HOME}/.gbrain/config.json  (gbrain 自己写)

${BOLD}注:${NC} 正常情况下 uClaw 启动时已经自动跑过这个。手动调用一般是 power-user
场景（重置、调试、CI 验证）。
EOF
}

confirm() {
    if $OPT_YES; then return 0; fi
    local prompt="$1 [y/N] "
    read -r -p "$(echo -e "${YELLOW}${prompt}${NC}")" answer
    case "$answer" in
        [yY][eE][sS]|[yY]) return 0 ;;
        *) return 1 ;;
    esac
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)   OPT_HELP=true ;;
        --force)  OPT_FORCE=true ;;
        --yes|-y) OPT_YES=true ;;
        *) error "未知选项: $1"; usage; exit 1 ;;
    esac
    shift
done

if $OPT_HELP; then usage; exit 0; fi

step "Pre-flight"
if [[ ! -x "${BUN_BIN}" ]]; then
    error "找不到 bun: ${BUN_BIN}"
    error "请先跑: ${PROJECT_DIR}/scripts/setup-bun-runtime.sh"
    exit 1
fi
if [[ ! -f "${GBRAIN_ENTRY}" ]]; then
    error "找不到 gbrain CLI entry: ${GBRAIN_ENTRY}"
    error "请先跑: ${PROJECT_DIR}/scripts/setup-gbrain-source.sh"
    exit 1
fi
success "bun + gbrain entry 都在"
info "GBRAIN_HOME = ${GBRAIN_HOME}"

step "检查已有 brain"
if [[ -f "${PG_VERSION}" ]]; then
    existing_version="$(cat "${PG_VERSION}")"
    if ! $OPT_FORCE; then
        warn "brain 已初始化 (PG_VERSION=${existing_version})，跳过 init"
        info "如要重置: $(basename "$0") --force"
        exit 0
    fi
    warn "--force 启用，准备重置已有 brain"
    if ! confirm "确定删除 ${BRAIN_DIR} 重新初始化吗？"; then
        warn "取消"
        exit 0
    fi
    info "删除 ${BRAIN_DIR}"
    rm -rf "${BRAIN_DIR}"
    success "已删除"
else
    info "brain 未初始化，准备 fresh init"
fi

step "确保 GBRAIN_HOME 存在"
mkdir -p "${GBRAIN_HOME}"
success "GBRAIN_HOME ready: ${GBRAIN_HOME}"

step "运行 gbrain init --pglite --yes"
info "首次 init 会跑 ~63 PGLite migrations，约 30-60s..."
GBRAIN_HOME="${GBRAIN_HOME}" "${BUN_BIN}" "${GBRAIN_ENTRY}" init --pglite --yes
success "gbrain init 退出码 0"

step "验证 PG_VERSION 已生成"
if [[ ! -f "${PG_VERSION}" ]]; then
    error "init 退出 0 但 ${PG_VERSION} 不存在"
    error "可能 gbrain 写到了别处。检查: ls -la ${GBRAIN_HOME}/.gbrain/"
    exit 1
fi
final_version="$(cat "${PG_VERSION}")"
success "PG_VERSION = ${final_version}"

step "完成"
success "brain 已初始化在 ${BRAIN_DIR}"
info "重启 uClaw 即可正常使用 gbrain MCP（连接应在 ~2-5s 内完成）"
