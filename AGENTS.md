# AGENTS.md

给在这个仓库里干活的 AI 代理与协作者的约定。动手前请先读完，尤其第一节。

## 项目

**Synbackup（同备）** —— 把 USB 移动存储设备插上电脑，就把指定文件夹同步好；差异先列出来给你确认，同步才动手。Windows / macOS，Tauri 2 + Vue 3 + TypeScript + Rust。

前端是纯视图层（WebView，关闭窗口即销毁），配置、设备监控、扫描、执行、历史全部由 Rust 后端负责，并常驻系统托盘运行。

---

## 一、更新日志：只写功能，不写实现

这是本仓库最重要的一条写作约定。

`CHANGELOG.md` 的每个版本段会被 `release.yml` **原样截取为 GitHub Release 的发布说明**。读者是准备下载软件的人，不是读代码的人。他们只关心一件事：这一版我能多做什么。

### 硬性要求

1. **第一条主句必须讲用户能干什么。** 新增了什么功能、界面上多了什么、原来做不到的事现在能做到。
2. **不写实现。** 文件名、函数名、组件名、类型名、IPC 命令名、crate 名、CSS 变量 / z-index / 选择器、算法取舍过程、调试中踩过的坑，一律不进 CHANGELOG。这些属于 README 的「架构」一节或提交信息。
3. **一条 1–3 句，约 120 字以内。** 讲清：新增了什么 → 用起来是什么样 → 有什么边界或默认值。详细不等于清楚。
4. **一条只讲一件事。** 一次提交做了三个功能，就写三条。
5. **用界面上的措辞。** 「待处理差异」「同步前询问」「品牌 logo」就是界面上的字，不要另造同义词。
6. **不写过程说明。** 「文案覆盖全部 11 种界面语言」「版本号升至 x.y.z」「旧配置向后兼容」这类话不进条目——语言覆盖是默认要求，版本号由版本段本身表达。

### 对照

反例（摘自本仓库历史条目，问题一眼可见）：

> 总览显示项新增三张可选统计卡：CPU 占用、GPU 占用、同步项目大小。CPU 取本应用进程……sysinfo 的进程占用是两次刷新之间的增量，因此按设置的频率采样、再按核数折成"占整机算力的百分比"……macOS 读 `ioreg` 里 IOAccelerator 的 `Device Utilization %`，Windows 查 GPU 引擎性能计数器……

正例：

> 总览新增 **CPU 占用 / GPU 占用 / 同步项目大小** 三张可选统计卡，在总览右上角「编辑」中逐项开关，每项可单独设刷新频率（实时指标 5–60 秒，项目大小 1–15 分钟）。CPU 与内存统计的是本应用自身的占用，GPU 是整机占用；读不到的指标显示占位符，不会假装是 0。

差距不在信息量，在取舍：**保留用户需要知道的口径与边界，删掉"这是怎么做到的"。**

### 修复类

写「什么场景下会出什么问题」+「现在怎样了」，不写根因链。

- ✗ 滑条元素漏了 `ref="hueRef"` 绑定，取基准矩形时拿到 `null` 直接返回，色相永远停住……
- ✓ 修复自定义调色板的色相滑条拖不动的问题——此前只有颜色面板可以拖，现在滑条与面板都 1:1 跟随指针。

### 变更类

写用户能感知的行为差异。会打断用户的改动（需要重新授权通知、数据目录迁移、旧设置失效）必须单独点明，不要藏在一段技术说明里。

纯内部重构（拆模块、换调度方式、重写实现）不占条目；只有当它改变了外部行为或兼容性，才以结果的口吻写一句。

### 格式

遵循 Keep a Changelog，版本段 `## [x.y.z] - YYYY-MM-DD`。`## [` 必须顶格——发布流水线靠它定位段落。分类只用 `### 新增 / 变更 / 修复 / 移除`，不新建分类。未发布的内容先写在 `## [Unreleased]`。每条以 `- ` 开头，一条一段，不嵌套多层列表。

---

## 二、版本与发布

版本号六处同步，缺一处就会出现"关于页版本和更新包对不上"：

- `src-tauri/tauri.conf.json` → `version`（发布流水线只读这一处，其余是它的镜像）
- `package.json` → `version`
- `src-tauri/Cargo.toml` → `version`（及 `Cargo.lock` 里 `synbackup` 那条）
- `src/views/SettingsView.vue` 的关于页版本回退值
- `README.md` 顶部版本徽标

发版两步：改上面六处版本号 + 在 `CHANGELOG.md` 补 `## [新版本号]` 段，推送到 `main`。流水线发现该版本 Release 已存在就跳过；版本是新的才打 tag、双平台构建并发布。含 `-` 的版本号发 Pre-Release，纯 `x.y.z` 发正式版并标记 latest。

**产物文件名不得改动**：`Synbackup_windows-x64.zip` / `Synbackup_macos-arm64.zip` 是应用内自动更新的下载依据，改名会让更新直接失效。名字里不带版本号（版本由 tag 表达），包内文件名固定：Windows 是 `Synbackup.exe`，macOS 是 `Synbackup.app`。装回时也不改名——用户手里的程序文件名与升级次数无关。

---

## 三、架构约束

- 业务逻辑全部放在 `src-tauri/src/backend/`，**不得依赖任何 Tauri / WebView 类型**。`lib.rs`、`commands.rs` 只是薄封装，不放业务规则。
- 前端不持有状态权威。配置、同步清单、待处理差异都存在后端；窗口销毁重建后必须能原样恢复。
- 关闭窗口 = 销毁 WebView，后端继续在托盘跑。任何功能都要考虑「窗口不在」的场景：通知、后台扫描、自动同步必须由 Rust 侧独立完成，不能依赖前端存活。
- 删除操作永远只作用于配对目录内部。配置与清单原子写入。复制保留修改时间。
- 忽略规则（`.DS_Store`、`Thumbs.db`、`System Volume Information`、符号链接等）只在一处定义，别让扫描、统计、执行各写一套。

### 平台配置

`tauri.conf.json` 面向 Windows（无边框 + 自绘按钮），`tauri.macos.conf.json` 覆写 macOS（透明叠加标题栏 + 原生红绿灯）。两者按 JSON Merge Patch 合并，**数组整体替换**——所以 macOS 那份把整个 `window` 对象重写了一遍。新增窗口字段时两处都要改，漏一处就是"Windows 好使、macOS 崩"。

平台差异按 `cfg` 分文件/分模块，不要把 if-else 撒进主流程。

### 前端约定

- 层级用 token，不写魔法数字：`--z-modal` 100 < `--z-popup` 120 < `--z-titlebar` 150 < `--z-toast` 200。
- 颜色只从 `src/styles/tokens.css` 取（主题色 `--accent`、辅助色 `--accent-2`），不硬编码品牌紫。
- 所有动效都要在 `prefers-reduced-motion: reduce` 下退化；材质在 `prefers-reduced-transparency: reduce` 下退回实色。
- 外观偏好首帧前就要生效（localStorage 存一份），避免启动闪变。

### 多语言

界面文案 11 种语言。新增键写在 `src/i18n/zh.ts`，其余 10 个文件同步补齐——`Dict = typeof zh` 会在 `npm run build` 的 `vue-tsc` 阶段强制键对齐，缺键或多键直接编译失败。不要绕过这个检查（比如改成 `any` 或可选键）。

---

## 四、开发

环境：Node.js ≥ 18、Rust ≥ 1.77（Windows 需 MSVC + WebView2，macOS 需 Xcode CLT）。

```bash
npm install
npm run tauri dev      # 开发运行
npm run tauri build    # 打包
```

提交前至少过一遍：

```bash
npm run build                                  # vue-tsc 类型检查 + vite 构建
cargo check --manifest-path src-tauri/Cargo.toml
```

`npm run build` 同时是 i18n 键对齐与 TS 类型守门人，别跳过。

---

## 五、提交信息

Conventional Commits，描述用中文，必要时在末尾括注版本号：

```
feat(overview): 总览新增 CPU / GPU / 项目大小三项统计（1.0.0）
fix(ui): 调色板色相滑条无法拖动
ci(release): 推送 main 自动打 tag 并发布
```

type 用 `feat` / `fix` / `ui` / `docs` / `ci` / `chore`；scope 取模块名（`ui`、`sync`、`notify`、`update`、`release`…）。提交正文可以写技术细节——**技术细节写在这里，不写进 CHANGELOG**。

---

## 六、不要做

- 不手改 `dist/`（构建产物，已在 `.gitignore`）。
- 不提交 `node_modules/`、`src-tauri/target/`、`.zcode/`。
- 不删除 `.workbuddy/`（项目记忆与约定存放处）。
- 不改自动更新依赖的产物命名与目录结构（`data/updates/`、`location.txt` 指针语义）。
- 不在 CHANGELOG 里贴代码、堆实现细节或复述调试过程。
- 不为一次性场景引入新依赖；确需新增 Rust crate 时，先确认没有现成模块能覆盖。
