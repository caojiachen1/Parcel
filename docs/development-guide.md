## Parcel 本地开发指南

本文档涵盖从环境准备到编译运行 CLI 工具和安装器模板的完整流程。

---

### 1. 环境要求

开始之前请确保系统已安装以下工具：

**Rust 工具链** — 推荐通过 [rustup](https://rustup.rs) 安装。需要 1.75+ 版本（项目使用 edition 2024）。安装后确认：

```bash
rustc --version
cargo --version
```

**Node.js** — 需要 18+ 版本，用于运行 Tauri CLI。确认：

```bash
node --version
npm --version
```

**Windows 构建依赖** — Tauri 在 Windows 上编译需要：

- [Microsoft Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（安装时勾选 "使用 C++ 的桌面开发"）
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)（Windows 11 已预装；Windows 10 如未安装会提示）

---

### 2. 克隆与初始化

```bash
git clone <repo-url> Parcel
cd Parcel
```

安装 Node 依赖（主要是 `@tauri-apps/cli`）：

```bash
npm install
```

这一步会在 `node_modules/` 下安装 Tauri CLI，后续通过 `npx tauri` 或 `cargo tauri` 调用。

---

### 3. 项目结构

```
Parcel/
├── Cargo.toml                   # Workspace 根配置
├── package.json                 # Node 脚本 & Tauri CLI
│
├── crates/
│   ├── parcel-core/             # 共享类型库（parcel.json schema）
│   │   └── src/
│   │       ├── lib.rs
│   │       └── config.rs        # 所有配置结构体定义
│   │
│   └── parcel-cli/              # CLI 工具（parcel 二进制）
│       └── src/
│           ├── main.rs          # 入口：clap 子命令分发
│           ├── init.rs          # parcel init 实现
│           ├── build.rs         # parcel build 实现
│           └── preview.rs       # parcel preview 实现
│
├── src-tauri/                   # 安装器运行时（Tauri 桌面应用）
│   ├── Cargo.toml
│   ├── tauri.conf.json          # Tauri 窗口 / 打包配置
│   ├── capabilities/
│   │   └── default.json         # 权限声明
│   ├── icons/
│   │   ├── icon.ico             # Windows 图标
│   │   └── icon.png
│   └── src/
│       ├── main.rs              # 二进制入口
│       ├── lib.rs               # Tauri Builder 注册
│       ├── config.rs            # 运行时 payload 加载
│       ├── commands.rs          # Tauri IPC 命令
│       └── engine/              # 安装引擎
│           ├── mod.rs           # 安装流程编排
│           ├── files.rs         # 文件复制 / 解压
│           ├── shortcuts.rs     # 快捷方式创建
│           ├── registry.rs      # 注册表操作
│           ├── vcredist.rs      # VC++ 检测
│           ├── rollback.rs      # 失败回滚
│           └── silent.rs        # 静默安装参数解析
│
├── frontend/                    # 安装器前端（原生 HTML/CSS/JS）
│   ├── index.html               # 主入口
│   ├── preview.html             # 静态预览页
│   ├── styles/
│   │   └── main.css             # Fluent Design 暗黑主题样式
│   ├── src/
│   │   └── main.js              # 向导流程 & IPC 逻辑
│   └── assets/
│       └── logo-placeholder.svg
│
└── parcel/                      # 模板资源（用户自定义）
    ├── assets/
    │   └── logo.png
    └── eula.txt                 # 许可协议模板
```

---

### 4. 编译 CLI 工具

CLI 工具是一个独立的 Rust 二进制，不依赖 Tauri 运行时，可以单独编译：

```bash
cargo build -p parcel-cli
```

编译产物在 `target/debug/parcel.exe`（debug）或 `target/release/parcel.exe`（release）。

你可以把它加到 PATH 中以便全局使用：

```bash
# 临时（当前终端）
$env:PATH = "$(Get-Location)\target\debug;$env:PATH"

# 永久 — 复制到 Cargo bin 目录
copy target\debug\parcel.exe "$env:USERPROFILE\.cargo\bin\"
```

之后就可以在任意目录使用 `parcel` 命令了。

---

### 5. 编译安装器运行时

安装器是 Tauri 桌面应用，需要用 `cargo tauri` 编译：

**Debug 模式（开发用）：**

```bash
npx tauri dev
```

这会编译 Rust 后端并启动 WebView2 窗口加载 `frontend/index.html`。前端文件直接读取磁盘，修改 CSS/JS 后刷新窗口即可看到效果。

**Release 模式（生产构建）：**

```bash
npx tauri build
```

产物在 `src-tauri/target/release/parcel-installer.exe`。

**仅检查编译（不生成二进制）：**

```bash
cargo check --workspace
```

这会检查整个 workspace（parcel-core + parcel-cli + parcel-installer）是否能编译通过，速度最快。

---

### 6. 运行测试

```bash
# 全部 crate 的测试
cargo test --workspace

# 仅 parcel-core（配置序列化测试）
cargo test -p parcel-core
```

---

### 7. 典型开发流程

以「在你的 Tauri 项目中使用 Parcel 生成安装包」为例：

```
Step 1: 开发你的 Tauri 应用
    > cd your-tauri-app
    > cargo tauri build --no-bundle
    → 产出 target/release/your-app.exe

Step 2: 初始化 Parcel 配置
    > parcel init
    → 自动检测 tauri.conf.json，生成 parcel.json + parcel/ 目录

Step 3: 自定义安装器外观
    编辑 parcel.json:
      - appearance.colors.primary → 你的品牌色
      - appearance.logo → 你的 Logo 路径
      - eula.file → 许可协议文件路径
    将 Logo、背景图等素材放入 parcel/assets/

Step 4: 预览安装器界面
    > parcel preview
    → 启动 Tauri dev server，弹出安装器窗口
    → 可以遍历所有页面，检查样式效果

Step 5: 构建安装包
    > parcel build
    → 收集目标 exe + 资源，生成 payload manifest
    → 调用 cargo tauri build 编译安装器
    → 产出 dist/YourApp_Setup.exe
```

---

### 8. 前端预览（无需 Tauri）

`frontend/index.html` 内置了 mock IPC 层。当检测不到 Tauri 运行时（即不在 WebView2 中），会自动返回模拟数据。你可以直接在浏览器中打开 `index.html` 来预览安装向导的视觉效果：

```bash
# 用系统默认浏览器打开
start frontend\index.html

# 或用 VS Code Live Server
code frontend/index.html
```

`preview.html` 是一个纯静态页面，展示了所有安装器页面的最终状态，适合快速浏览整体布局。

---

### 9. 自定义主题

安装器的视觉风格由 `parcel.json` 的 `appearance` 段驱动，无需改代码：

```json
{
  "appearance": {
    "theme": "dark",
    "colors": {
      "primary": "#60CDFF",
      "accent": "#4DB8E8",
      "background": "#000000",
      "text": "#F5F5F5",
      "text_secondary": "#A0A0A0"
    },
    "border_radius": 6,
    "page_animation": "fade",
    "font_family": ""
  }
}
```

所有 CSS 变量定义在 `frontend/styles/main.css` 的 `:root` 块中。如需更深度的定制（如更换布局结构），直接修改 HTML 和 JS 文件即可。

---

### 10. 常见问题

**Q: 编译报错 "icons/icon.ico not found"**
确保 `src-tauri/icons/` 下有 `icon.ico` 文件。项目已包含一个占位图标。如果你替换了图标，需要同时提供 `.ico`（Windows 资源编译需要）和 `.png` 格式。

**Q: `parcel preview` 启动失败**
确认已执行 `npm install` 安装了 `@tauri-apps/cli`。如果 `cargo tauri dev` 报错，检查 Visual Studio Build Tools 是否已安装 C++ 工作负载。

**Q: 如何添加新的 Tauri IPC 命令？**
在 `src-tauri/src/commands.rs` 中添加 `#[tauri::command]` 函数，然后在 `lib.rs` 的 `invoke_handler` 中注册。前端通过 `invoke("command_name", { args })` 调用。

**Q: 如何支持亮色主题？**
CSS 当前只有暗黑主题。如需亮色模式，在 `main.css` 中添加 `.theme-light` 类覆盖 CSS 变量，然后在 JS 的 `applyConfig()` 中根据 `cfg.appearance.theme` 切换。
