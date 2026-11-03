# 同备 Synbackup

把 USB 移动存储设备插上电脑，同备就帮你把指定文件夹同步好——**差异先列出来给你确认，同步才动手**。

`0.3.0` · Windows / macOS · Tauri 2 + Vue 3 + TypeScript + Rust

## 功能特性

- **自绘标题栏**：不使用系统标题栏，窗口顶部由前端绘制一条 40px 标题栏，承载应用标识与窗口按钮。整条都是拖拽区，双击可最大化 / 还原；macOS 保留系统红绿灯并把左侧空间让给它，Windows 自绘最小化 / 最大化 / 关闭按钮，关闭键悬停按 Windows 语义变红。
- **后台常驻**：关闭窗口后 WebView 进程完全退出，仅保留 Rust 后端进程在系统托盘运行；插入设备、扫描差异、通知提醒都在后台完成，随时从托盘唤回界面。
- **设备监控**：自动检测新插入的移动存储设备（U 盘、移动硬盘、读卡器等）。检测由系统事件驱动——macOS 监听 NSWorkspace 卷挂载通知，Windows 接收 WM_DEVICECHANGE 设备广播，空闲时零扫描开销；事件合并去抖后走快照对比，每次变化把「设备 + 关联任务」的完整快照推给界面，插入设备后界面立即刷新，无需手动操作。另保留 60 秒一次的低频兜底复查，应对事件遗漏。
- **同步任务**：将一个本地文件夹与一台设备上的文件夹配对；设备通过配对时写入的标记文件识别，**卷标改名、盘符变化后依然有效**。
- **差异检查**：基于上次同步清单做三方对比，区分"本地新增 / 设备新增 / 本地已修改 / 设备已修改 / 两侧均修改（冲突）/ 某侧已删除 / 类型冲突"。
- **差异处理**：扫描结果以列表呈现，逐项或批量选择处理方式（复制到设备 / 复制到本地 / 删除某侧 / 跳过），确认后才执行。
- **自动同步（可选）**：开启后，无冲突的差异按建议方向自动执行；**冲突与删除永远需要人工确认**。开始与完成都会发系统通知，完成后说明同步了多少文件、失败多少。
- **系统通知适配**：通知统一由 Rust 后端发送，窗口关闭、仅托盘常驻时照常弹出。macOS 使用现代 `UNUserNotificationCenter`（启动时请求授权，前台也显示横幅）；Windows 按环境自动选择通知身份——NSIS 安装版用应用自身（显示应用名与图标），便携版 / 开发环境落到系统 PowerShell AUMID，保证 toast 始终能弹出。
- **同步历史**：每次执行（自动 / 手动）都会记一条，标明触发方式，并可展开查看逐项明细——哪个文件、执行了什么动作、成功还是失败、失败原因。单条最多保留 200 项，超出部分只给计数。
- **中英双语界面**、开机自启、插入设备时系统通知。
- **个性化**：五档主题色、浅色 / 深色 / 跟随系统外观、界面字体、字体与界面大小（− / + 步进调节）、启动动画开关；外观偏好即时生效，首帧不闪变。
- **更新渠道**：稳定通道对应正式版（Release），测试通道对应预发布（Pre-Release）。

## 界面

界面按一套明确的规则收敛，而不是逐个页面调样式：

- **窗口装饰自绘**：`src-tauri/tauri.conf.json` 关闭系统边框（`decorations: false`），`src-tauri/tauri.macos.conf.json` 单独把 macOS 覆写为「透明叠加标题栏 + 隐藏标题文本」（`titleBarStyle: Overlay`、`hiddenTitle: true`），因此三个红绿灯仍由系统绘制，其余部分全部由 `src/components/TitleBar.vue` 决定。
- **品牌配色**：主题色 `#9B62CF` 用于"实"（主按钮、选中态填充、强调文字与图标、焦点环、进度条），辅助色 `#BE93E3` 用于"虚"（浅底、悬停描边、次级图标）。选中/悬停浅底由辅助色派生，悬停实色取主题色深一档 `#8A4FC0`，保证白字对比度不下降。全部集中在 `src/styles/tokens.css`，改一层即可换肤。
- **标识即矢量**：`src/components/Logo.vue` 内联 SVG（不经过 `<img>`、不使用位图或 data URI）。标题栏放横向长方形文字 logo（16px 高，宽度按 1144:206 原始比例推算）；关于页把同一文字标居中独立展示（32px 高）。两处为同一组件、同一设计稿（`Synbackup Logo with text.svg`），任何缩放下都不失真、不模糊。
- **材质分层**：标题栏与侧栏同为结构层，使用同一层半透明材质；内容区为实色。不在浅色材质上再叠浅色材质。
- **交互即时**：窗口按钮悬停 / 按下均有即时反馈，`prefers-reduced-motion` 下退化为无位移的颜色变化；`prefers-reduced-transparency` 下侧栏与标题栏转为实色。
- **平台惯例优先**：macOS 关闭按钮在左上、Windows 在右上且悬停变红；按钮顺序与图标状态（最大化 / 还原）跟随窗口真实状态同步。

已知限制：macOS 的透明叠加标题栏由系统实现，窗口失焦时拖拽无效（Tauri 上游限制，`tauri-apps/tauri#4316`）。

## 架构

前后端完全分离：

```
┌─────────────────────────┐        ┌──────────────────────────────┐
│  前端  Vue 3 + TS (WebView) │ ←IPC→  │  后端  Rust (backend 核心)      │
│  纯视图层，可随时销毁        │  事件   │  配置 / 监控 / 扫描 / 执行 / 历史 │
└─────────────────────────┘        └──────────────────────────────┘
                                            ↑ 托盘常驻，独立运行
```

- 所有业务逻辑都在 `src-tauri/src/backend/` 中，**不依赖任何 Tauri / WebView 类型**；`lib.rs` 与 `commands.rs` 只是薄封装。
- 关闭主窗口时 WebView 被彻底销毁（Windows 上 WebView2 进程随之退出），后端进程与监控循环继续运行。
- 前端不持有任何状态权威：配置、清单、待处理差异全部存在后端，窗口重开后恢复如初。

### 同步模型

每个任务维护一份同步清单（记录上次同步时每个文件在两侧的大小与修改时间）：

| 状态 | 含义 | 建议操作 |
| --- | --- | --- |
| 本地新增 / 设备新增 | 只在一侧存在 | 复制到另一侧 |
| 本地已修改 / 设备已修改 | 仅一侧相对上次同步有变化 | 以修改侧为准 |
| 两侧均修改 | 双向冲突 | 由用户裁决（默认跳过） |
| 某侧已删除 | 上次同步后一侧文件被移除 | 默认跳过，绝不自动删 |
| 类型冲突 | 同名文件与目录并存 | 由用户裁决 |

安全设计：复制保留修改时间；忽略 `.DS_Store`、`Thumbs.db`、`System Volume Information` 等系统文件；删除操作永远只作用于配对目录内部；配置与清单原子写入。

## 开发

环境要求：Node.js ≥ 18、Rust ≥ 1.77（Windows 需 MSVC 与 WebView2，macOS 需 Xcode Command Line Tools）。

```bash
npm install
npm run tauri dev     # 开发运行
npm run tauri build   # 打包（macOS 生成 .app/.dmg，Windows 生成 .msi/.exe）
```

仅校验：

```bash
npm run build         # vue-tsc 类型检查 + vite 构建
cd src-tauri && cargo check
```

## CI

每次推送都会通过 GitHub Actions 自动构建（`.github/workflows/ci.yml`），产物在 Actions 页面的 Artifacts 中下载：

- `Synbackup-<版本>-windows-x64` — Windows x64 便携版（单文件 exe，目标机器需已安装 WebView2 Runtime，Win10/11 默认内置）
- `Synbackup-<版本>-macos-arm64` — macOS Apple Silicon DMG（未签名，首次打开需右键 → 打开）

版本号取自 `src-tauri/tauri.conf.json`。

窗口装饰按平台分文件配置：`tauri.conf.json` 面向 Windows（无边框 + 自绘按钮），`tauri.macos.conf.json` 覆写 macOS 窗口（透明叠加标题栏 + 原生红绿灯）。平台配置按 JSON Merge Patch 合并，**数组整体替换**，所以 macOS 里把整个窗口对象重写了一遍，新增窗口字段时两处都要改。

应用图标取自设计稿 `Synbackup Logo.svg`，按 macOS 的图标网格绘制：1024×1024 画布、白色圆角方块（squircle，圆角约 22% 画布宽）铺满画布、标识居中占 60% 宽、边缘一道极淡描边。导出 1024 母版后执行 `npx tauri icon <母版.png>`，一次产出 `src-tauri/icons/` 下的 .icns / .ico / 全尺寸 PNG（含 iOS、Android 变体）。

macOS 菜单栏另用 `src-tauri/icons/tray-template.png`（纯黑剪影、透明底），配合 `icon_as_template` 让系统按 alpha 取形并随明暗外观自动反色；Windows / Linux 托盘仍用彩色 `32x32.png`。

## 数据位置

配置、同步清单与历史记录存放在用户数据目录（界面"设置 → 关于"中可查看）：

- macOS：`~/Library/Application Support/com.mogrowang.synbackup/`
- Windows：`%APPDATA%\com.mogrowang.synbackup\`

配对时会在设备侧目录写入一个 `.synbackup-device` 标记文件（含随机 UUID），用于跨卷标识别设备；不需要时删除该文件即可。

## 目录结构

```
├── index.html
├── src/                    # 前端（Vue 3 + TypeScript）
│   ├── api.ts              #   IPC 封装
│   ├── store.ts            #   全局状态
│   ├── i18n.ts             #   中英文案
│   ├── styles/             #   设计令牌与基础样式
│   ├── components/         #   自绘标题栏、差异对话框、任务编辑器等
│   └── views/              #   总览 / 任务 / 设备 / 历史 / 设置
├── src-tauri/
│   ├── tauri.conf.json     #   窗口 / 打包配置（关闭系统边框）
│   ├── tauri.macos.conf.json  # macOS 覆盖：透明叠加标题栏，保留系统红绿灯
│   └── src/
│       ├── backend/        # 后端核心（无 Tauri 依赖）
│       │   ├── devices.rs  #   设备枚举与识别
│       │   ├── scan.rs     #   差异扫描
│       │   ├── manifest.rs #   同步清单
│       │   ├── execute.rs  #   同步执行
│       │   └── ...
│       ├── commands.rs     # IPC 命令（薄封装）
│       └── lib.rs          # 托盘 / 窗口生命周期 / 监控循环
```

## 路线图

- [x] 界面规范：自绘标题栏、主题色 / 辅助色、材质分层
- [x] 品牌标识：矢量 logo（界面内联）+ 全平台应用图标
- [ ] 空目录同步、符号链接策略
- [ ] 计划任务与定时备份
- [ ] 冲突"保留两份"策略
- [ ] 多语言托盘菜单即时生效

## 文档

- [CHANGELOG](CHANGELOG.md)

---

版权所有 © 2026 MogroWang Studio。保留所有权利。
