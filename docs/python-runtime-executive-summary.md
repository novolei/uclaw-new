# uClaw Python Runtime 打包方案 - 执行总结

## 核心问题
uClaw Tauri 应用当前依赖系统 Python 和手动安装的 memU 包，终端用户需要完整开发环境才能使用。目标是在 Release 版本中完整打包 Python runtime，使用户开箱即用。

## 调研结论

### 当前架构特征
- **通信模式**: JSON-RPC over stdio (子进程)
- **Python 版本**: 3.13+ (硬性要求)
- **包依赖**: ~10 个直接依赖 + 大量传递依赖
- **总体积**: 完整 site-packages 约 250-400 MB

### 发现的核心事实
1. memU 依赖树较重 (httpx, numpy, openai, pydantic, langchain 等)
2. Tauri 2.10.3 已支持资源打包 (resources + externalBin)
3. PyTauri 等项目已验证 python-build-standalone 方案可行
4. 子进程 JSON-RPC 架构已成熟稳定，无需大幅改造

---

## 五大方案对比总结

| 方案 | 体积 | 启动 | 复杂度 | 推荐度 | 适用场景 |
|------|------|------|--------|--------|----------|
| **A. PyInstaller** | 150-250 MB | 2-5s | 中 | ⭐⭐⭐ | 体积优先 |
| **B. python-build-standalone** | 250-350 MB | 1-2s | 中 | ⭐⭐⭐⭐⭐ | **首选** |
| **C. PyO3 嵌入式** | 200-300 MB | <1s | 高 | ⭐⭐ | 性能关键 |
| **D. uv 工具链** | 20-30 MB | 首次 30-60s | 低 | ⭐⭐⭐ | SaaS 应用 |
| **E. 混合方案** | 150-200 MB | 2-5s | 高 | ⭐⭐⭐⭐ | 演进式迁移 |

---

## 推荐方案：python-build-standalone

### 为什么选择这个方案？

**排名权重：**
1. **成熟度** (30%) - PyTauri 已验证，官方维护 ✓✓✓
2. **用户体验** (25%) - 启动快、无需额外操作 ✓✓✓
3. **开发效率** (20%) - 9-10 小时实施 ✓✓✓
4. **跨平台** (15%) - 预构建二进制，无需平台特定编译 ✓✓✓
5. **体积合理** (10%) - 250-350 MB 可接受 ✓✓

**得分：95/100** (对标 Electron 应用 150-300 MB 基准)

### 核心优势
- ✓ 启动速度最快 (预编译、不需解包)
- ✓ 开发成本最低 (仅需配置，无复杂编译)
- ✓ 跨平台最简单 (官方提供预构建二进制)
- ✓ 维护成本最低 (依赖官方维护)
- ✓ 用户体验最好 (完全透明，无感知)

### 实施时间
- **总工作量**: 9-10 小时 (1-2 天)
- **关键路径**:
  1. 下载配置 (1-2 小时)
  2. Rust 代码改造 (2 小时)
  3. Tauri 配置 (1 小时)
  4. 测试验证 (3-4 小时)

### 包体积分解
```
最终应用包体积：280-320 MB

组成：
├─ Python 3.13 runtime:     60-80 MB   (系统库)
├─ site-packages:          150-200 MB  (memU + 依赖)
├─ 应用二进制 (Rust):        <1 MB     (uClaw)
├─ 前端资源 (UI):           10-20 MB   (TypeScript/Svelte)
└─ 其他资源:                20-30 MB   (图标、配置等)
────────────────────────────
合计:                      280-320 MB
```

---

## 实施路线图

### Phase 1: 准备 (Day 1 上午)
- [ ] 下载 python-build-standalone (3 个平台)
- [ ] 提取到 `src-tauri/pyembed/python/`
- [ ] 使用 uv 安装 memU 依赖

### Phase 2: 核心改造 (Day 1 下午 - Day 2 上午)
- [ ] 修改 `app.rs` 中的 `find_python()` 和 `try_init_memu()`
- [ ] 更新路径解析逻辑，支持嵌入式 Python
- [ ] 修改 `build.rs`，复制 `memu_bridge.py` 到资源目录

### Phase 3: 配置 (Day 2 上午)
- [ ] 修改 `tauri.conf.json` 配置资源打包
- [ ] 更新 `.taurignore` 避免开发冲突

### Phase 4: 测试 (Day 2 下午)
- [ ] 开发模式测试 (`cargo tauri dev`)
- [ ] Release 构建测试 (`cargo tauri build`)
- [ ] macOS/Windows/Linux 验证

### Phase 5: 文档和 CI/CD (可选，Day 3)
- [ ] 编写构建文档
- [ ] 配置 GitHub Actions 自动化
- [ ] 发布首个 Release 版本

---

## 关键修改点

### 文件清单
```
需要修改：
├─ src-tauri/src/app.rs          (app.rs - 路径检测逻辑)
├─ src-tauri/Cargo.toml          (build.rs 配置)
├─ src-tauri/build.rs            (资源复制)
├─ src-tauri/tauri.conf.json     (资源打包)
└─ src-tauri/.taurignore         (开发时排除)

新增：
├─ src-tauri/pyembed/python/     (嵌入式 Python)
└─ src-tauri/build_scripts/      (构建脚本)
```

### 关键代码示例

**app.rs - 双路径 Python 检测**
```rust
// 优先使用嵌入式 Python (Release)
let embedded_candidates = [
    "Resources/python/bin/python3",
    "Resources/python/python.exe",
];

// 回退到系统 Python (开发)
let system_candidates = ["python3.13", "python3", "python"];
```

**tauri.conf.json - 资源配置**
```json
"bundle": {
  "resources": {
    "pyembed/python": "Resources/python",
    "src/memu/memu_bridge.py": "Resources/memu_bridge.py"
  }
}
```

---

## 风险评估和缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| 包体积过大 | 低 | 中 | 优化依赖、删除不必要文件 |
| macOS 签名失败 | 中 | 高 | 预签名 Python 目录 |
| Windows/Linux 兼容 | 低 | 中 | CI/CD 多平台测试 |
| Python 路径错误 | 低 | 中 | 详细日志、多候选路径 |
| 首次启动延迟 | 低 | 低 | 显示初始化进度 |

---

## 与其他项目的参考

### PyTauri 项目的经验教训
✓ python-build-standalone 方案已被验证可行  
✓ 需要注意 macOS dylib 的 install_name 修复  
✓ Tauri resource_dir 打包方式清晰  
✓ 首次安装应显示进度提示  

### 其他 Tauri + Python 项目
✓ Kivy 集成方案使用 PyInstaller，体积较大  
✓ FastAPI 后端方案使用子进程通信，架构与 uClaw 相似  
✓ Streamlit 集成方案需要特殊处理，复杂度高  

---

## 后续优化建议

### 短期 (发布后 1-2 个版本)
1. 实现 memU 初始化进度显示
2. 添加详细的启动日志
3. 实现 Python 环境自动检测和更新

### 中期 (3-6 个月)
1. CI/CD 自动化 (多平台编译验证)
2. Python 环境增量更新 (仅下载变化部分)
3. 支持可选模块 (用户可选安装/卸载)

### 长期 (6+ 个月)
1. 性能优化 (考虑 PyO3，若 memU 请求频繁)
2. WebAssembly 方案探索 (完全移除 Python)
3. 环境优化 (自动删除未使用的库)

---

## 决策建议

### 立即行动
**推荐开始实施 python-build-standalone 方案**

理由：
- 方案稳定可靠（已验证）
- 开发成本合理（1-2 天）
- 用户体验优秀（启动快、无感知）
- 维护成本低（依赖官方维护）
- 收益明确（完全解决用户依赖问题）

### 预期成果
实施完成后，用户将能够：
- ✓ 下载 uClaw.dmg (macOS) / uClaw.exe (Windows) / uClaw.AppImage (Linux)
- ✓ 直接安装，无需额外操作
- ✓ 启动后 1-2 秒内可用
- ✓ 完整的 memU 功能可用
- ✓ 无需系统 Python，完全隔离

### 成功指标
- [ ] Release 体积 < 400 MB
- [ ] 冷启动时间 < 2 秒
- [ ] 跨平台测试全部通过
- [ ] memU 功能 100% 可用
- [ ] 无用户环境依赖

---

## 附录：资源链接

**官方文档**
- Tauri 2 资源打包: https://v2.tauri.app/develop/sidecar/
- python-build-standalone: https://github.com/indygreg/python-build-standalone
- PyTauri 实现参考: https://pytauri.github.io/pytauri/0.8/usage/tutorial/build-standalone/

**下载地址**
- python-build-standalone: https://github.com/indygreg/python-build-standalone/releases (选择 3.13 版本)

**相关工具**
- uv: https://github.com/astral-sh/uv (Python 包管理器)

