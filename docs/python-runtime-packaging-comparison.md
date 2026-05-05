# 方案对比详细矩阵

## 核心指标对比

### 包体积
```
PyInstaller (A):
  ├─ memu_bridge exe: 80-120 MB
  └─ 依赖库: 50-80 MB
  ─────────────────
  合计: 150-250 MB ✓ 最小

python-build-standalone (B):
  ├─ Python 3.13: 60-80 MB
  ├─ site-packages: 150-200 MB
  └─ 应用本身: <1 MB
  ─────────────────
  合计: 250-350 MB (推荐平衡)

PyO3 (C):
  ├─ libpython: 10-15 MB
  ├─ 应用二进制: 30-50 MB
  └─ site-packages: 150-200 MB
  ─────────────────
  合计: 200-300 MB (但集成复杂)

uv (D):
  ├─ uv 工具: 20-30 MB
  └─ 首次下载 Python: 60-80 MB (一次性)
  ─────────────────
  合计: 20-30 MB (分布式下载)

混合 (E):
  ├─ PyInstaller Release: 150-200 MB
  └─ 系统 Python (Dev): 0 MB
  ─────────────────
  合计: 150-200 MB
```

### 启动时间
```
PyInstaller (A):       2-5s (解包耗时)
python-build-standalone (B): 1-2s ✓ 最快
PyO3 (C):              <1s (无子进程) ✓ 理论最快
uv (D):                首次 30-60s，后续 <2s (有缓存)
混合 (E):              2-5s (Release)
```

### 开发复杂度
```
简单   中等        复杂      非常复杂
  |      |          |            |
  D      B          A       C + E
  |      |          |            |
  
详细：
D (uv):        简单 - 仅需配置下载 URL
B (standalone): 中等 - 下载、配置资源、修改路径
A (PyInstaller): 中等 - 构建脚本、多平台编译
C (PyO3):      复杂 - 需要 FFI 包装、libpython 编译
E (混合):      非常复杂 - 两套构建流程
```

### 跨平台支持
```
平台          PyInstaller  Standalone  PyO3   uv   混合
─────────────────────────────────────────────────────
macOS x86        ✓            ✓        ✓      ✓    ✓
macOS ARM64      ✓            ✓        ✓      ✓    ✓
Windows x86      ✓            ✓        ✓      ✓    ✓
Linux x86        ✓            ✓        ✓      ✓    ✓
Linux ARM        ✗            ✓        ✓      ✓    ✗

编译位置：
- PyInstaller: 需在目标平台编译
- Standalone: 预构建二进制可用
- PyO3: 需在目标平台编译
- uv: 远程下载对应平台版本
- 混合: 需在目标平台编译
```

### 维护成本
```
低维护成本：
  uv (D) - 官方维护 Python 版本
  Standalone (B) - 官方维护 Python-build-standalone

中等维护成本：
  PyInstaller (A) - 需要维护 spec 配置、处理依赖变化
  混合 (E) - 维护两套构建流程

高维护成本：
  PyO3 (C) - 需要维护 Rust FFI 层、处理 libpython 版本升级
```

---

## 实施工作量估算

### python-build-standalone (推荐方案)

```
任务                        工作量      难度
──────────────────────────────────────────
1. 下载和提取 Python       30 分钟     ★☆☆☆☆
2. 使用 uv 安装 memU      20 分钟     ★☆☆☆☆
3. 修改 Rust 代码          2 小时      ★★☆☆☆
   - 更新 find_python()
   - 修改路径解析逻辑
   - 添加资源定位
4. 配置 tauri.conf.json    30 分钟     ★☆☆☆☆
5. 修改 build.rs           30 分钟     ★★☆☆☆
6. 测试 (dev + release)    2 小时      ★★☆☆☆
   - 验证路径检测
   - 验证 memU 初始化
   - 验证跨平台
7. CI/CD 集成 (可选)       2 小时      ★★★☆☆
   - GitHub Actions 脚本

总计：9-10 小时（1-2 天工作）
```

### PyInstaller 方案 (备选)

```
任务                        工作量      难度
──────────────────────────────────────────
1. 创建 PyInstaller spec   1 小时      ★★☆☆☆
2. 测试打包                1 小时      ★★☆☆☆
3. 处理依赖问题            2 小时      ★★★☆☆
4. 配置 Tauri externalBin  1 小时      ★★☆☆☆
5. 多平台编译脚本          2 小时      ★★★☆☆
6. 测试 (3 个平台)         3 小时      ★★★☆☆
7. 签名和代码验证          1 小时      ★★☆☆☆

总计：11-12 小时（1.5-2 天工作）
```

---

## 性能对标

### 子进程通信开销（JSON-RPC over stdio）

```
当前架构开销：
  - 进程启动: 0.5-1s (一次性)
  - 请求序列化: <1ms (JSON)
  - 进程间通信: 1-5ms (stdio)
  - 响应反序列化: <1ms

PyO3 嵌入式开销：
  - 无进程启动
  - Python FFI 调用: 10-50ns
  - Rust->Python 转换: 100-500ns
  - Python->Rust 转换: 100-500ns

性能差异：
  - memU 初始化: PyO3 快 5-10 倍
  - 单次请求: PyO3 快 100-1000 倍（微观）
  - 整体应用感知: 无显著差异（大多数时间花在 LLM API 上）
```

---

## 风险评分矩阵

```
风险类别        PyInstaller  Standalone  PyO3   uv   混合
─────────────────────────────────────────────────────
包体积过大         ★★★         ★★         ★★☆   ★      ★★
启动延迟          ★★☆         ★          ★☆    ★★★☆  ★★☆
跨平台兼容         ★★★         ★          ★★    ★      ★★★
依赖冲突          ★★          ★☆         ★★★   ★☆     ★★
代码签名问题       ★★          ★★         ★★    ★      ★★
维护复杂度         ★★          ★          ★★★   ★      ★★★★

风险评分说明：
★☆☆☆☆ = 风险低
★★☆☆☆ = 风险中低
★★★☆☆ = 风险中
★★★★☆ = 风险中高
★★★★★ = 风险高
```

---

## 示例实施路径

### 推荐路径 (python-build-standalone)

```
Week 1:
  Day 1-2:  下载配置 + Rust 代码修改
  Day 3-4:  Tauri 配置 + 初步测试
  Day 5:    macOS 完整测试

Week 2:
  Day 1-2:  跨平台编译脚本
  Day 3-4:  Windows/Linux 测试
  Day 5:    文档和 CI/CD

Go-live: Week 2 结束
```

### 快速路径 (uv 工具链)

```
Day 1:
  - 集成 uv 下载逻辑
  - 验证功能

Day 2:
  - 测试多平台
  - 处理首次下载 UX

Go-live: Day 2 结束 (风险: 首次启动体验)
```

---

## 技术栈兼容性

### 与当前项目的兼容性评分

```
current_stack = {
  "runtime": "Tauri 2.10.3",
  "backend": "Rust (Tokio)",
  "frontend": "TypeScript + Svelte",
  "python_integration": "subprocess JSON-RPC"
}

python-build-standalone (B):
  Tauri 2.10.3 兼容性: ✓✓✓ (完美)
  Rust 集成难度: ✓✓ (中等)
  需要修改文件: 3-4 个

PyInstaller (A):
  Tauri 2.10.3 兼容性: ✓✓ (良好)
  Rust 集成难度: ✓✓ (中等)
  需要修改文件: 4-5 个
  编译脚本复杂度: 中等

PyO3 (C):
  Tauri 2.10.3 兼容性: ✓ (需要改造)
  Rust 集成难度: ✗✗ (复杂)
  需要修改文件: 8-10 个
  学习曲线: 陡峭
```

