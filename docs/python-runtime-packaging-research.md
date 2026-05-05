# uClaw Tauri 应用 Python Runtime 打包调研

## 1. 当前架构总结

### 集成现状
- **通信模式**: JSON-RPC over stdio，子进程通信
- **Python 版本**: 3.13+ (硬性要求)
- **启动方式**: 动态查找系统 Python 解释器 (python3.13 > python3 > python)
- **依赖检查**: memu_bridge.py 在启动时检查 memu 包是否可导入
- **降级模式**: 若 Python 或 memU 不可用，应用以降级模式运行，memU 功能禁用

### 文件结构
```
src-tauri/src/memu/
├── bridge.rs       # 子进程生命周期管理、JSON-RPC 通信
├── client.rs       # 高级 Rust 客户端 API
├── memu_bridge.py  # Python 侧 JSON-RPC 服务器
└── mod.rs          # 模块导出
```

### 初始化流程 (app.rs)
1. AppState::new() 调用 try_init_memu()
2. 定位 memu_bridge.py（开发时在 src/memu，发布时在 ~/.uclaw）
3. 查找系统 Python
4. 创建 MemUBridge 和 MemUClient，存入 AppState

### 当前问题
- **用户依赖**: 终端用户需要手动安装 Python 3.13 和 memU 包
- **环境复杂性**: 跨平台 Python 配置不统一，容易失败
- **发布体验**: 用户首次启动需要完整的开发环境

---

## 2. memU 依赖树分析

### 核心依赖 (pyproject.toml)
```
defusedxml           >= 0.7.1    (XML 安全解析)
httpx                >= 0.28.1   (异步 HTTP 客户端) ✓ 核心
numpy                >= 2.3.4    (数值计算) ✓ 核心
openai               >= 2.8.0    (OpenAI SDK) ✓ 核心
pydantic             >= 2.12.4   (数据验证) ✓ 核心
sqlmodel             >= 0.0.27   (SQL 模型) ✓ 核心
alembic              >= 1.14.0   (数据库迁移)
pendulum             >= 3.1.0    (时间处理)
langchain-core       >= 1.2.7    (LLM 框架) ✓ 核心
lazyllm              >= 0.7.3    (LLM 管道)
```

### 传递依赖估算
- **httpx** 依赖: sniffio, certifi, idna, rfc3986, anyio...
- **numpy** 依赖: 无（预编译二进制）
- **openai** 依赖: requests, pydantic, tqdm, distro...
- **pydantic** 依赖: annotated-types, pydantic-core...
- **sqlmodel** 依赖: SQLAlchemy, pydantic...
- **langchain-core** 依赖: PyYAML, SQLAlchemy, jsonpatch...
- **lazyllm** 依赖: openai, pydantic, 其他...

### 体积估算
| 类别 | 估算大小 | 说明 |
|------|--------|------|
| Python 3.13 runtime | 50-80 MB | 解释器 + 标准库 |
| numpy (预编译) | 40-60 MB | macOS/Linux/Windows 分别编译 |
| memU 源代码 | 2-5 MB | Python 模块代码 |
| 所有第三方库 (site-packages) | 150-250 MB | 包括所有传递依赖 |
| **总计** | **250-400 MB** | 平台无关部分 |

---

## 3. 各方案横向对比

| 方案 | 体积 | 启动时间 | 开发复杂度 | 维护成本 | 跨平台 | 推荐度 |
|------|------|--------|---------|--------|------|--------|
| **A. PyInstaller** | 150-250 MB | 快 (2-5s) | 中 | 中 | ✓ | ⭐⭐⭐ |
| **B. python-build-standalone** | 250-400 MB | 快 (1-2s) | 中 | 低 | ✓ | ⭐⭐⭐⭐ |
| **C. PyO3 嵌入式** | 50-100 MB | 很快 (<1s) | 高 | 高 | ✓ | ⭐⭐ |
| **D. uv 工具链** | 30-50 MB (uv本身) | 中 (首次 30-60s) | 低 | 低 | ✓ | ⭐⭐⭐ |
| **E. 混合方案** | 80-150 MB | 快 (2-5s) | 高 | 中 | ✓ | ⭐⭐⭐⭐ |

### 详细分析

#### **A. PyInstaller 打包为独立可执行文件**

**优点**
- 单个可执行文件，开箱即用
- 用户无需安装 Python
- 打包流程成熟，文档丰富
- 可隐藏源代码（轻度加密）

**缺点**
- 打包体积大（150-250 MB）
- 启动时间较长（解包过程 2-5s）
- 跨平台编译复杂（需在不同 OS 上编译）
- 反编译风险（非真正的编译）

**集成难点**
- memu_bridge.py 需转换为独立 exe
- Tauri 需配置 externalBin 来调用打包的可执行文件
- 每个平台需单独编译

**文件大小**
- PyInstaller 基础包：30-50 MB
- memU + 依赖：100-150 MB
- 总计：150-250 MB

---

#### **B. python-build-standalone（推荐）**

**优点** ✓ 推荐
- 官方维护的便携 Python 发行版
- 启动速度最快（不需解包）
- 可直接嵌入 Tauri resources
- 跨平台编译简单（预构建二进制）
- PyTauri 等项目已成功验证
- 依赖无需重新编译

**缺点**
- 打包体积最大（250-400 MB）
- 需要手动下载和配置
- 首次启动需要 site-packages 初始化

**实施方案**
```
1. 下载 python-build-standalone (3.13, 精简版)
   - x86_64-unknown-linux-gnu-install_only_stripped.tar.gz
   - aarch64-apple-darwin-install_only_stripped.tar.gz
   - x86_64-pc-windows-msvc-install_only_stripped.tar.gz

2. 提取到 src-tauri/pyembed/python/

3. 使用 uv 安装 memU:
   uv pip install --python ./src-tauri/pyembed/python/bin/python3 memu

4. Tauri 配置:
   {
     "bundle": {
       "resources": {
         "pyembed/python": "./"
       }
     }
   }

5. Rust 代码修改:
   bridge.rs 中 python_path = app_path/Resources/python/bin/python3
```

**文件大小**
- Python 3.13 (install_only_stripped)：60-80 MB
- memU + 依赖：150-200 MB
- 总计：250-350 MB

---

#### **C. PyO3 直接嵌入 Python 解释器**

**优点**
- 启动速度最快（无子进程开销）
- 体积最小（仅需 libpython）
- 可获得最佳性能

**缺点**
- 开发复杂度最高
- 需要构建 libpython（平台特定）
- memU 代码需要 PyO3 包装
- 对 Rust 开发者要求高
- 维护成本高

**实施难点**
- memU 是纯 Python，需要全量包装
- site-packages 仍需打包到资源
- 跨平台编译配置复杂

**不推荐原因**: 收益不足以抵消复杂度

---

#### **D. uv 工具链（运行时下载）**

**优点**
- 无需预打包 Python
- 初始应用包体积小（仅 5-20 MB）
- 用户获得正确版本的依赖
- uv 本身速度极快（10-100x 比 pip）

**缺点**
- 首次启动耗时（30-60s 下载 + 安装）
- 需要网络连接
- 用户体验不佳（冷启动延迟）
- 离线场景无法使用

**适用场景**
- SaaS 应用（可接受首次延迟）
- 开发工具（可接受冷启动）
- **不适合桌面应用**（用户期望立即可用）

---

#### **E. 混合方案：PyInstaller (Release) + 系统 Python (Dev)**

**实施方案**
- 开发模式（cargo tauri dev）：使用系统 Python
- Release 构建（cargo tauri build）：PyInstaller 生成可执行文件

**优点**
- 开发快速迭代
- Release 用户体验好
- 开发依赖最小化

**缺点**
- 构建过程复杂
- 需要平台特定的编译脚本
- CI/CD 配置复杂

**文件大小**: 80-150 MB (Release)

---

## 4. 推荐方案及理由

### 首选：**B. python-build-standalone + Tauri Resources**

**选择理由**
1. **最稳定**: PyTauri 等项目已成功验证该方案
2. **维护性最优**: 无需处理 Python 编译，官方维护的二进制
3. **用户体验好**: 启动速度快（预编译、不需解包）
4. **跨平台简单**: 预构建二进制，无需在多平台编译
5. **成本合理**: 250-350 MB 可接受（对标 Electron 应用 150-300 MB）

### 备选：**A. PyInstaller (若需更小体积)**

**场景**: 若应用体积要求严格 (<150 MB)
- 体积：150-200 MB
- 启动时间：2-5s（稍慢）

---

## 5. 推荐方案实施步骤

### 第一阶段：准备工作

#### 1.1 下载 python-build-standalone

从 https://github.com/indygreg/python-build-standalone/releases 下载：

```bash
# macOS x86_64
wget https://github.com/indygreg/python-build-standalone/releases/download/20250415/cpython-3.13.13-x86_64-apple-darwin-install_only_stripped.tar.gz

# macOS ARM64
wget https://github.com/indygreg/python-build-standalone/releases/download/20250415/cpython-3.13.13-aarch64-apple-darwin-install_only_stripped.tar.gz

# Linux x86_64
wget https://github.com/indygreg/python-build-standalone/releases/download/20250415/cpython-3.13.13-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz

# Windows x86_64
wget https://github.com/indygreg/python-build-standalone/releases/download/20250415/cpython-3.13.13-x86_64-pc-windows-msvc-install_only_stripped.tar.gz
```

#### 1.2 提取到 Tauri 项目

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri

# 创建目录
mkdir -p pyembed

# 为当前平台提取（以 macOS x86_64 为例）
tar -xzf cpython-3.13.13-x86_64-apple-darwin-install_only_stripped.tar.gz -C pyembed/

# 目录结构
# pyembed/
# ├── python/
# │   ├── bin/
# │   ├── include/
# │   ├── lib/
# │   └── share/
```

#### 1.3 配置 .taurignore

在 `src-tauri/.taurignore` 添加：
```
/pyembed/
```

这防止 `tauri dev` 时重新复制 pyembed。

### 第二阶段：依赖安装

#### 2.1 使用 uv 安装 memU 到嵌入式 Python

```bash
# 安装 uv (如未安装)
curl -LsSf https://astral.sh/uv/install.sh | sh

# 安装 memU 到嵌入式 Python
cd /Users/ryanliu/Documents/uclaw/src-tauri

export EMBEDDED_PYTHON="./pyembed/python/bin/python3"

uv pip install \
  --python "$EMBEDDED_PYTHON" \
  --break-system-packages \
  memu

# 验证
$EMBEDDED_PYTHON -c "import memu; print(memu.__version__)"
```

### 第三阶段：修改 Rust 代码

#### 3.1 更新 app.rs 中的 Python 路径解析

```rust
fn find_python() -> Option<String> {
    // 首先尝试使用嵌入式 Python (Release)
    let embedded_candidates = [
        #[cfg(target_os = "macos")]
        "Resources/python/bin/python3",
        #[cfg(target_os = "linux")]
        "Resources/python/bin/python3",
        #[cfg(target_os = "windows")]
        "Resources/python/python.exe",
    ];
    
    for candidate in &embedded_candidates {
        if let Ok(output) = std::process::Command::new(candidate)
            .arg("--version")
            .output()
        {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                tracing::debug!("Found embedded Python: {} -> {}", candidate, version.trim());
                return Some(candidate.to_string());
            }
        }
    }
    
    // 回退到系统 Python (开发模式)
    let system_candidates = ["python3.13", "python3", "python"];
    for candidate in &system_candidates {
        if let Ok(output) = std::process::Command::new(candidate)
            .arg("--version")
            .output()
        {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                tracing::debug!("Found system Python: {} -> {}", candidate, version.trim());
                return Some(candidate.to_string());
            }
        }
    }
    
    None
}
```

#### 3.2 更新路径计算逻辑

```rust
fn try_init_memu(data_dir: &std::path::Path, app_path: &std::path::Path) -> Option<Arc<MemUClient>> {
    // 定位 memu_bridge.py
    let script_path = {
        // 首先检查开发时位置
        let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("memu")
            .join("memu_bridge.py");
        if dev_path.exists() {
            dev_path
        } else {
            // 检查嵌入式位置
            #[cfg(target_os = "macos")]
            let embedded_path = app_path.join("Contents/Resources/memu_bridge.py");
            
            #[cfg(not(target_os = "macos"))]
            let embedded_path = app_path.join("memu_bridge.py");
            
            if embedded_path.exists() {
                embedded_path
            } else {
                // 最后检查数据目录
                let data_script = data_dir.join("memu_bridge.py");
                if data_script.exists() {
                    data_script
                } else {
                    return None;
                }
            }
        }
    };

    let python_path = Self::find_python();
    let python_path = python_path?;

    let bridge = Arc::new(MemUBridge::new(python_path, script_path, data_dir.to_path_buf()));
    let client = Arc::new(MemUClient::new(bridge));
    Some(client)
}
```

### 第四阶段：复制资源文件

#### 4.1 将 memu_bridge.py 复制到资源目录

```bash
# 创建构建脚本 src-tauri/build_resources.sh
#!/bin/bash

# 复制 memu_bridge.py 到资源目录
mkdir -p target/release/resources/memu
cp src/memu/memu_bridge.py target/release/resources/memu_bridge.py
```

#### 4.2 修改 Cargo.toml 构建流程

在 `src-tauri/Cargo.toml` 中添加：

```toml
[package]
build = "build.rs"

[build]
# ... 现有配置
```

修改 `build.rs`：

```rust
fn main() {
    tauri_build::build();
    
    // 复制 memu_bridge.py
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let src = "src/memu/memu_bridge.py";
    let dst = format!("{}/memu_bridge.py", out_dir);
    std::fs::copy(src, dst).ok();
}
```

### 第五阶段：配置 Tauri 打包

#### 5.1 修改 tauri.conf.json

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "uClaw",
  "version": "0.1.0",
  "identifier": "ai.uclaw.desktop",
  "build": {
    "beforeDevCommand": "cd ../ui && npm run build",
    "beforeBuildCommand": "cd ../ui && npm run build",
    "frontendDist": "../static"
  },
  "app": {
    "macOSPrivateApi": true,
    "windows": [{"title": "uClaw", "width": 1280, "height": 820}]
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/icon.png"],
    "resources": {
      "pyembed/python": "Resources/python",
      "src/memu/memu_bridge.py": "Resources/memu_bridge.py"
    }
  }
}
```

### 第六阶段：测试和验证

#### 6.1 开发模式测试

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri
cargo tauri dev
```

验证：
- [ ] 应用启动成功
- [ ] 日志显示 "Found Python" 和 "memU bridge initialized"
- [ ] memU 功能正常（health check 返回 "ok"）

#### 6.2 Release 构建测试

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri
cargo tauri build
```

验证生成的应用：
- [ ] 包含 `pyembed/python` 目录
- [ ] 包含 `memu_bridge.py`
- [ ] 启动后能正常使用 memU
- [ ] 运行 `./app.app/Contents/Resources/python/bin/python3 --version` 显示 Python 3.13

---

## 6. 风险与缓解措施

| 风险 | 影响 | 缓解措施 |
|------|------|--------|
| **包体积过大** | Release 体积 350+ MB | 使用"精简"版本 (install_only_stripped) |
| **首次安装卡顿** | 用户体验差 | 实现进度条 UI，显示初始化状态 |
| **跨平台兼容性** | Windows/Linux 编译失败 | CI/CD 在各平台构建验证 |
| **Python 路径不对** | 子进程启动失败 | 详细的启动日志，多个候选路径 |
| **macOS 代码签名** | 发布失败 | 签名 pyembed/python 目录 |
| **依赖冲突** | memU 版本不兼容 | 锁定 memU 依赖版本，定期更新 |

---

## 7. 后续优化建议

### 7.1 短期（1-2 个版本）
1. 实现 memU 的异步初始化，显示启动进度
2. 添加日志记录，便于排查问题
3. 实现 Python runtime 的自动检测和更新

### 7.2 中期（3-6 个月）
1. 集成 CI/CD，自动在多平台测试打包
2. 实现应用更新时的 Python 环境同步
3. 支持可选的 memU 模块（用户可选安装）

### 7.3 长期（6+ 个月）
1. 考虑 PyO3 方案（若性能成为瓶颈）
2. 探索 WebAssembly 方案（完全移除 Python 依赖）
3. 实现 Python 环境的自动优化（删除未使用的库）

