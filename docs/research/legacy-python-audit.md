# 旧版 Python GUI 审计

## 1. 旧版项目现状

旧项目是一个 Tkinter + ttkbootstrap GUI。它已经能承担自用下载工具的核心流程，但代码组织更像一个功能原型。

主要文件：

- `main_to_start.py`：启动入口、DPI、主题、主窗口。
- `gui.py`：主控制器，包含页面切换、解析、下载、命令拼接、进程管理、封面加载、弹窗逻辑。
- `page_home.py`：链接输入、解析结果、封面、分 P、下载参数。
- `page_adv.py`：路径、画质编码、下载行为、命名、网络工具。
- `page_net.py`：Cookie、Access Token、API 模式。
- `config.py`：配置加载、保存、路径自动检测。
- `logger.py`：日志。
- `utils.py`：文件名清理、窗口适配、滚轮绑定。

## 2. 已实现的产品能力

### 首页能力

- 输入视频链接或 BV 号。
- 解析按钮。
- 停止当前任务按钮。
- 封面预览。
- 展示标题、BV 号、UP 主、时长、发布时间。
- 展示保存路径。
- Treeview 展示分 P、视频流、音频流。
- 分 P 范围输入。
- 分 P 选择弹窗，支持全选、全不选、反选。
- 下载内容开关：视频、音频、弹幕、字幕、封面。
- 高级处理开关：混流&命名、跳过 AI 字幕。
- 开始下载。

### 高级设置能力

- BBDown 路径。
- ffmpeg 路径。
- 保存目录。
- 编码优先级：`avc,hevc,av1`。
- 画质优先级：4K、1080P 高码率、1080P、杜比、8K、HDR、720P、480P、360P。
- 多线程。
- 强制 HTTP。
- 视频体积升序。
- 音频体积升序。
- 单 P 文件名模板。
- 多 P 文件名模板。
- 混流语言代码。
- aria2c 开关。
- MP4Box 开关。
- 允许 PCDN。
- 合集间隔。

### 账号能力

- Cookie 输入。
- Access Token 输入。
- API 模式：WEB、TV、APP、INTL。
- WEB 扫码登录入口。
- TV 扫码登录入口。

### 解析与展示能力

- 调用 `BBDown -info --show-all -p ALL`。
- UTF-8 失败后尝试 GBK。
- 正则提取标题、UP、发布时间、时长、BV。
- 解析分 P 列表。
- 解析视频流和音频流。
- 下载封面并展示。
- 内存解析缓存。

### 标色能力

旧版根据流文本给 Treeview tag：

- `8K`：金色。
- `杜比`、`Dolby`：蓝色。
- `HDR`：珊瑚红。
- `4K`：洋红。
- `1080P 高码率` 或 `60.0`：紫色。
- 普通流：灰色。
- 错误：红色。

新版保留这个信息层，但改成 badge + 色彩，避免只靠颜色表达。

## 3. 旧版主要问题

### 3.1 控制器过大

`gui.py` 同时负责：

- UI 页面初始化和切换。
- 子窗口。
- 配置保存。
- 命令拼接。
- 子进程执行。
- 解析 BBDown 输出。
- 封面获取。
- 图片加载。
- 下载任务状态。

这导致任何功能变化都可能影响整条链路。

### 3.2 命令执行方式不安全

旧版通过字符串拼接命令，并使用 `shell=True`。路径、URL、cookie、文件名模板都可能包含特殊字符，容易出现转义错误，也有命令注入风险。

新版必须使用参数数组执行：

```rust
Command::new(bbdown_path)
  .arg(input)
  .arg("--work-dir")
  .arg(work_dir);
```

### 3.3 配置和运行产物污染项目目录

旧版默认在项目目录下创建：

- `config/`
- `download/`
- `logs/`
- `__pycache__/`
- `bin/*.data`

新版必须使用系统应用目录和用户选择的下载目录。

### 3.4 输出解析不稳定

旧版直接在 GUI 控制器里正则解析 BBDown 人类可读输出。新版必须把解析器独立出来，使用 fixture 测试覆盖普通视频、多分 P、番剧、错误输出、不同编码。

### 3.5 UI 与数据耦合

旧版 `save_all_ui()` 直接读取每个页面控件值。新版需要用结构化状态和表单模型，UI 控件不应成为配置源本身。

## 4. 迁移策略

保留：

- 功能设计。
- 常用默认值。
- 分 P 选择体验。
- 画质/编码顺序选择。
- 封面预览。
- 流信息标色。
- WEB/TV 登录入口。

不迁移：

- Tkinter 页面结构。
- `shell=True` 命令执行。
- GUI 控制器内解析输出。
- 项目目录内保存用户数据。
- 过度萌化文案作为默认工具风格。

## 5. 新版对应模块

| 旧版位置 | 新版模块 |
| --- | --- |
| `gui.py do_parse` | `src-tauri/src/bbdown/parser.rs` + `commands/parse.rs` |
| `gui.py do_download` | `src-tauri/src/bbdown/command_builder.rs` + `task/manager.rs` |
| `gui.py run_process` | `task/process.rs` |
| `config.py` | `config/store.rs` |
| `page_home.py` | `src/pages/WorkspacePage.tsx` |
| `page_adv.py` | `src/pages/SettingsPage.tsx` |
| `page_net.py` | `src/pages/AuthPage.tsx` 或 Settings 子页 |
| Treeview tag | `StreamBadge` 组件 |
| `logger.py` | Rust tracing + task log storage |

