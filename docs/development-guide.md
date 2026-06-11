## Parcel 使用与编译指南

Parcel 是一个 **Tauri 安装程序生成器**，帮助你为自己的 Tauri 桌面应用快速生成漂亮的安装向导（类似 Inno Setup / NSIS，但更现代）。

本文档涵盖环境准备、编译方法、CLI 使用说明和配置参考。

---

### 1. 环境要求

| 工具 | 最低版本 | 说明 |
|------|---------|------|
| **Rust** | 1.75+（edition 2024） | 通过 [rustup](https://rustup.rs) 安装 |
| **Node.js** | 18+ | 用于 Tauri CLI 和前端构建 |
| **VS C++ Build Tools** | — | [下载地址](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，安装时勾选「使用 C++ 的桌面开发」 |
| **WebView2** | — | Windows 11 已预装；Windows 10 如未安装会自动提示 |

安装后验证：

```powershell
rustc --version
cargo --version
node --version
npm --version
```

---

### 2. 项目结构概览

```
Parcel/
├── Cargo.toml               # Rust Workspace 根配置
├── package.json             # Node 依赖 & Tauri CLI 脚本
│
├── parcel-core/             # 📦 共享类型库（parcel.json 配置 schema）
│   └── src/
│       ├── lib.rs
│       ├── config.rs        # 所有配置结构体定义
│       └── safety.rs        # 路径安全校验
│
├── parcel-cli/              # 🔧 CLI 工具（parcel 命令行）
│   └── src/
│       ├── main.rs          # 入口：clap 子命令分发
│       ├── init.rs          # parcel init
│       ├── build.rs         # parcel build
│       ├── preview.rs       # parcel preview
│       └── clean.rs         # parcel clean
│
├── parcel-uninstaller/      # 🗑️ 卸载程序
│   └── src/main.rs
│
├── src-tauri/               # 🖥️ 安装器运行时（Tauri 桌面应用）
│   ├── tauri.conf.json      # Tauri 窗口 / 打包配置
│   └── src/
│       ├── main.rs / lib.rs
│       ├── commands.rs      # Tauri IPC 命令
│       ├── config.rs        # 运行时 payload 加载
│       └── engine/          # 安装引擎核心
│           ├── mod.rs       # 安装流程编排
│           ├── files.rs     # 文件复制 / 解压
│           ├── shortcuts.rs # 快捷方式创建
│           ├── registry.rs  # 注册表操作
│           ├── vcredist.rs  # VC++ 运行时检测
│           ├── rollback.rs  # 失败回滚
│           └── silent.rs    # 静默安装
│
├── parcel-setup/            # 安装器前端模板（Vite + Tauri）
│   ├── src-tauri/           # 独立的 Tauri 后端
│   ├── src/                 # 前端源码
│   └── index.html
│
├── src/                     # 安装器前端（主项目）
│   ├── main.js
│   ├── styles/main.css
│   └── assets/
│
├── index.html               # 安装器入口页面
├── vite.config.js           # Vite 构建配置
│
└── tests/test-app/          # 测试用示例项目
```

---

### 3. 初始化项目

```powershell
cd d:\Codebase\Parcel
npm install
```

这一步会安装 `@tauri-apps/cli` 等 Node 依赖，后续编译时自动调用。

---

### 4. 编译

#### 4.1 编译 CLI 工具（`parcel` 命令）

CLI 是独立的 Rust 二进制，不依赖 Tauri 运行时，编译最快：

```powershell
# Debug 模式
cargo build -p parcel-cli

# Release 模式（推荐分发用）
cargo build -p parcel-cli --release
```

编译产物：
- Debug: `target/debug/parcel.exe`
- Release: `target/release/parcel.exe`

**将 CLI 加入 PATH（可选）：**

```powershell
# 临时（仅当前终端）
$env:PATH = "$(Get-Location)\target\debug;$env:PATH"

# 永久 — 复制到 Cargo bin 目录
copy target\debug\parcel.exe "$env:USERPROFILE\.cargo\bin\"
```

#### 4.2 编译安装器运行时（Tauri 桌面应用）

```powershell
# 开发模式（热重载，前端修改刷新即可）
npx tauri dev

# 生产构建（Release）
npx tauri build
```

产物路径：`src-tauri/target/release/parcel-installer.exe`

#### 4.3 编译 parcel-setup（安装器模板应用）

parcel-setup 是一个独立的 Tauri 子项目：

```powershell
cd parcel-setup
npm install
npx tauri dev      # 开发
npx tauri build    # 生产构建
```

#### 4.4 编译卸载程序

```powershell
cargo build -p parcel-uninstaller
cargo build -p parcel-uninstaller --release
```

产物：`target/release/uninstall.exe`

#### 4.5 仅检查编译（最快验证）

```powershell
# 检查整个 workspace 是否能编译通过，不生成二进制
cargo check --workspace
```

#### 4.6 运行测试

```powershell
# 全部 crate
cargo test --workspace

# 仅 parcel-core（配置序列化测试）
cargo test -p parcel-core
```

---

### 5. CLI 使用指南

CLI 工具提供 4 个子命令：`init`、`build`、`preview`、`clean`。

#### 5.1 `parcel init` — 初始化配置

在你的 Tauri 项目根目录运行：

```powershell
parcel init
```

**作用：** 自动检测 `tauri.conf.json`，生成 `parcel.json` 配置文件和 `parcel/` 资源目录。

#### 5.2 `parcel build` — 构建安装包

```powershell
parcel build
```

**执行流程：**
1. 读取 `parcel.json` 配置
2. 收集目标 exe + 资源文件
3. 生成 payload 清单
4. 调用 `cargo tauri build` 编译安装器
5. 输出安装包到 `dist/` 目录（如 `dist/YourApp_Setup.exe`）

#### 5.3 `parcel preview` — 预览安装器界面

```powershell
parcel preview
```

启动 Tauri dev server，弹出安装器窗口，可以遍历所有页面检查样式效果，**不会执行实际安装**。

#### 5.4 `parcel clean` — 清理构建产物

```powershell
# 仅清理构建缓存（.parcel-build/）
parcel clean

# 同时清理 dist/ 输出目录
parcel clean --dist

# 完全重置（包括 parcel.json 和 parcel/）
parcel clean --all

# 预览模式：只显示会删除什么，不实际操作
parcel clean --dry-run
parcel clean --all --dry-run
```

---

### 6. 典型使用流程

以「为你的 Tauri 应用生成安装包」为例：

```
第一步：先编译你的 Tauri 应用
    > cd your-tauri-app
    > cargo tauri build --no-bundle
    → 产出 target/release/your-app.exe

第二步：初始化 Parcel 配置
    > parcel init
    → 自动检测 tauri.conf.json
    → 生成 parcel.json + parcel/ 目录

第三步：自定义安装器外观
    编辑 parcel.json（详见下方配置参考）
    将 Logo、背景图等素材放入 parcel/assets/

第四步：预览安装器界面
    > parcel preview
    → 弹出窗口，检查样式和交互

第五步：构建安装包
    > parcel build
    → 产出 dist/YourApp_Setup.exe
```

---

### 7. `parcel.json` 配置参考

所有字段都有默认值，你只需要覆盖关心的部分。以下是完整配置示例：

```json
{
  "app": {
    "name": "MyApp",
    "version": "1.0.0",
    "identifier": "com.example.myapp",
    "publisher": "My Company",
    "publisher_url": "https://example.com"
  },

  "paths": {
    "target_exe": "src-tauri/target/release/app.exe",
    "resources": [],
    "icon": "icons/icon.png",
    "output_dir": "dist"
  },

  "install": {
    "default_dir": "{localappdata}\\Programs\\{name}",
    "allow_custom_dir": true,
    "require_admin": false,
    "shortcuts": {
      "desktop_default": true,
      "desktop_optional": true,
      "start_menu_default": true,
      "start_menu_optional": true
    },
    "file_associations": [],
    "auto_start": {
      "enabled": true,
      "default_value": false
    }
  },

  "appearance": {
    "theme": "dark",
    "colors": {
      "primary": "#60CDFF",
      "accent": "#4DB8E8",
      "background": "#202020",
      "text": "#F5F5F5",
      "text_secondary": "#A0A0A0"
    },
    "logo": "parcel/assets/logo.png",
    "font_family": "",
    "welcome_background": null,
    "finish_background": null,
    "border_radius": 6,
    "page_animation": "fade",
    "strings": {
      "welcome_title": "欢迎使用 {name} 安装程序",
      "welcome_subtitle": "版本 {version}",
      "btn_next": "下一步",
      "btn_back": "上一步",
      "btn_install": "安装",
      "btn_finish": "完成",
      "btn_cancel": "取消"
    }
  },

  "eula": {
    "file": null
  },

  "signing": {
    "enabled": false,
    "certificate": null,
    "password": null
  }
}
```

#### 配置要点

| 配置项 | 说明 |
|--------|------|
| `app.name` | 安装器中显示的应用名称 |
| `app.identifier` | 反向域名格式的唯一标识 |
| `paths.target_exe` | 你的应用可执行文件路径（相对于项目根目录） |
| `paths.resources` | 额外资源文件的 glob 模式列表 |
| `install.default_dir` | 默认安装目录，支持 `{localappdata}`、`{programfiles}`、`{name}` 占位符 |
| `install.require_admin` | 是否需要管理员权限 |
| `appearance.theme` | 主题：`"dark"` 或 `"light"` |
| `appearance.colors.primary` | 品牌主色（十六进制） |
| `appearance.logo` | Logo 图片路径 |
| `appearance.strings` | 所有界面文案均可自定义（支持 `{name}`、`{version}` 占位符） |
| `eula.file` | 许可协议文件路径（纯文本或 HTML），设为 `null` 则跳过许可协议页 |
| `signing` | 代码签名配置（证书路径 + 密码） |

---

### 8. 自定义安装器外观

无需改代码，通过 `parcel.json` 的 `appearance` 段即可定制：

```json
{
  "appearance": {
    "theme": "dark",
    "colors": {
      "primary": "#3B82F6",
      "accent": "#2563EB",
      "background": "#0F172A",
      "text": "#F8FAFC",
      "text_secondary": "#94A3B8"
    },
    "border_radius": 8,
    "page_animation": "slide"
  }
}
```

如需更深层的布局定制，直接修改 `src/styles/main.css` 和 `index.html`。

---

### 9. 前端预览（无需 Tauri）

安装器前端内置了 mock IPC 层，在普通浏览器中打开时会自动返回模拟数据：

```powershell
# 用系统默认浏览器打开
start index.html
```

适合快速浏览安装向导的视觉效果，无需编译 Rust。

---

### 10. 常见问题

**Q: 编译报错 `icons/icon.ico not found`**
确保 `src-tauri/icons/` 下有 `icon.ico` 文件。替换图标时需同时提供 `.ico` 和 `.png` 格式。

**Q: `parcel preview` 启动失败**
确认已执行 `npm install`。如果 `cargo tauri dev` 报错，检查 Visual Studio Build Tools 是否已安装 C++ 工作负载。

**Q: PowerShell 中 `&&` 不能用**
PowerShell 不支持 `&&` 连接命令，请用分号 `;` 代替。

**Q: 如何添加新的 Tauri IPC 命令？**
在 `src-tauri/src/commands.rs` 添加 `#[tauri::command]` 函数，在 `lib.rs` 的 `invoke_handler` 中注册，前端通过 `invoke("command_name", { args })` 调用。

**Q: 如何支持亮色主题？**
在 `main.css` 中添加 `.theme-light` 类覆盖 CSS 变量，然后在 JS 的 `applyConfig()` 中根据 `cfg.appearance.theme` 切换。
