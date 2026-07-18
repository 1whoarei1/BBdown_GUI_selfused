# BBDown Next 技术设计总览

## 1. 项目目标

BBDown Next 是一个面向桌面端的哔哩哔哩下载管理工具。它不重新实现 Bilibili 解析和下载协议，而是作为 BBDown 的图形化控制层，负责解析请求、下载配置、任务队列、实时日志、结果展示、账号配置和工具链管理。

新版的核心目标：

- 覆盖 BBDown v1.6.3 的主要命令行能力。
- 完整继承旧 Python GUI 已有体验：解析、封面预览、分 P 选择、批量下载、画质/编码优先级、下载内容开关、扫码登录入口、流信息标色。
- 避免旧版架构问题：UI、配置、命令拼接、子进程、解析器、日志不再互相缠绕。
- 使用可维护的类型模型，让前后端契约可测试、可演进。
- 默认不把配置、日志、下载结果写进源码仓库。

## 2. 技术栈

### 2.1 推荐栈

- Tauri 2：桌面壳、打包、权限和原生能力。
- React：构建复杂状态 UI。
- TypeScript：前端类型约束。
- Rust：管理本地文件、配置、进程、任务队列和解析器。
- Zustand 或 TanStack Store：前端轻量状态管理。
- TanStack Query：处理 invoke 请求、缓存解析结果和任务查询。
- Zod：前端运行时数据校验，和 Rust 端 serde 类型形成双保险。
- Vitest：前端单元测试。
- Rust `cargo test`：后端命令构建、解析器、配置迁移测试。

### 2.2 Tauri 能力使用原则

前端只通过 Tauri command 调 Rust，不直接拼接和执行系统命令。Rust 后端拥有唯一的 BBDown 执行权。

可选能力：

- 用户自选外部 `BBDown.exe`、`ffmpeg.exe`、`mp4box.exe`、`aria2c.exe` 路径。
- 发布版可把 BBDown 或辅助工具作为 sidecar 打包，但 MVP 优先使用用户本地路径，减少授权和分发复杂度。
- 文件选择、打开目录、读取封面等功能由 Rust command 或受限 Tauri 插件提供。

参考依据：

- Tauri command 支持前端调用 Rust 函数，并支持参数、返回值、错误和异步命令。
- Tauri shell plugin 支持生成子进程，但默认危险命令受权限限制。
- Tauri sidecar 支持把外部二进制放入打包产物。
- Tauri 文件系统插件和 Rust `std::fs`/`tokio::fs` 可用于配置、日志、文件检测。

## 3. 总体架构

```text
bbdown-next/
  src/                         前端 React
    app/
    pages/
    components/
    features/
    stores/
    lib/
    types/
  src-tauri/
    src/
      commands/                暴露给前端的 Tauri commands
      core/                    领域模型、错误、事件
      bbdown/                  BBDown 命令构建、执行、输出解析
      task/                    任务队列、进程句柄、事件流
      config/                  配置读写、迁移、路径检测
      tools/                   ffmpeg/mp4box/aria2c/BBDown 检测
      storage/                 历史记录、缓存、日志索引
  docs/
```

## 4. 关键边界

### 4.1 前端职责

- 展示解析结果、任务队列、日志和设置。
- 维护临时 UI 状态，例如当前选中的分 P、当前筛选器、展开折叠状态。
- 调用 Rust command，不拼命令行字符串。
- 根据后端事件更新进度、状态和日志。

### 4.2 Rust 后端职责

- 读取和保存配置。
- 检测 BBDown、ffmpeg、mp4box、aria2c。
- 根据结构化选项构造 BBDown 参数数组。
- 启动、停止、追踪子进程。
- 解析 BBDown 输出，转换成结构化数据。
- 维护任务队列和历史记录。
- 过滤敏感日志，避免泄露 cookie/token。

### 4.3 BBDown 兼容层职责

`bbdown` 模块是新版的核心适配层。它必须做到：

- 所有命令参数从结构化配置生成，禁止字符串拼接 shell 命令。
- 输出解析器独立于 UI，可单元测试。
- 对 BBDown 版本做检测和能力标记。
- 对无法结构化解析的原始输出保留原文日志，便于排障。

## 5. 领域模型草案

```ts
type ApiMode = "WEB" | "TV" | "APP" | "INTL";
type TaskStatus = "queued" | "parsing" | "running" | "stopping" | "completed" | "failed" | "canceled";
type MediaKind = "video" | "audio" | "danmaku" | "subtitle" | "cover";
type StreamKind = "video" | "audio";
type StreamBadge = "8k" | "dolby" | "hdr" | "4k" | "high1080p" | "normal";
```

```ts
interface ToolPaths {
  bbdownPath: string;
  ffmpegPath?: string;
  mp4boxPath?: string;
  aria2cPath?: string;
}

interface AuthConfig {
  apiMode: ApiMode;
  cookie?: string;
  accessToken?: string;
  userAgent?: string;
}

interface AppConfig {
  tools: ToolPaths;
  workDir: string;
  auth: AuthConfig;
  defaultOptions: DownloadOptions;
  advanced: AdvancedOptions;
}

interface ParseRequest {
  input: string;
  apiMode: ApiMode;
  auth?: AuthConfig;
  showAllParts: boolean;
  useCache: boolean;
}

interface ParseResult {
  id: string;
  input: string;
  bvid?: string;
  aid?: string;
  title: string;
  ownerName?: string;
  publishTime?: string;
  duration?: string;
  coverPath?: string;
  savePath?: string;
  parts: PartInfo[];
  rawOutputRef: string;
  warnings: string[];
}

interface PartInfo {
  pageNumber: number;
  title: string;
  cid?: string;
  videoStreams: StreamInfo[];
  audioStreams: StreamInfo[];
}

interface StreamInfo {
  kind: StreamKind;
  rawText: string;
  qualityLabel?: string;
  resolution?: string;
  fps?: string;
  codec?: string;
  bandwidth?: string;
  badges: StreamBadge[];
}

interface DownloadOptions {
  media: {
    video: boolean;
    audio: boolean;
    danmaku: boolean;
    subtitle: boolean;
    cover: boolean;
  };
  pageSelection: string;
  codecPriority: string[];
  dfnPriority: string[];
  mux: boolean;
  skipAiSubtitle: boolean;
  multiThread: boolean;
}

interface AdvancedOptions {
  forceHttp: boolean;
  useAria2c: boolean;
  aria2cArgs?: string;
  useMp4box: boolean;
  allowPcdn: boolean;
  videoAscending: boolean;
  audioAscending: boolean;
  filePattern?: string;
  multiFilePattern?: string;
  language?: string;
  delayPerPage?: number;
  uposHost?: string;
  forceReplaceHost?: boolean;
  saveArchivesToFile?: boolean;
  biliPlus?: {
    host?: string;
    epHost?: string;
    area?: "hk" | "tw" | "th";
  };
}
```

## 6. 后端 Command API 草案

### 配置与工具

- `get_config() -> AppConfig`
- `save_config(config: AppConfig) -> AppConfig`
- `detect_tools() -> ToolDetectionResult`
- `validate_tools(paths: ToolPaths) -> ToolValidationResult`
- `get_bbdown_version(path: string) -> VersionInfo`
- `open_path(path: string) -> Result`

### 解析

- `parse_video(request: ParseRequest) -> ParseResult`
- `cancel_parse(parse_id: string) -> Result`
- `get_parse_cache() -> ParseResult[]`
- `clear_parse_cache() -> Result`

### 下载任务

- `build_preview_command(request: DownloadRequest) -> CommandPreview`
- `enqueue_download(request: DownloadRequest) -> DownloadTask`
- `start_task(task_id: string) -> Result`
- `stop_task(task_id: string) -> Result`
- `remove_task(task_id: string) -> Result`
- `list_tasks() -> DownloadTask[]`
- `clear_finished_tasks() -> Result`

### 账号与登录

- `run_login_web() -> TaskId`
- `run_login_tv() -> TaskId`
- `save_auth(auth: AuthConfig) -> Result`
- `clear_auth(kind: "cookie" | "token" | "all") -> Result`

### 历史和日志

- `list_history(filter: HistoryFilter) -> HistoryItem[]`
- `remove_history(id: string) -> Result`
- `export_logs(task_id?: string) -> string`

## 7. 事件设计

Rust 后端通过 Tauri event 向前端推送实时状态。

```ts
type BackendEvent =
  | { type: "task-log"; taskId: string; line: string; level: "debug" | "info" | "warn" | "error" }
  | { type: "task-status"; taskId: string; status: TaskStatus; exitCode?: number }
  | { type: "task-progress"; taskId: string; current?: number; total?: number; message?: string }
  | { type: "parse-complete"; parseId: string; result: ParseResult }
  | { type: "tool-detected"; result: ToolDetectionResult };
```

日志规则：

- 原始 BBDown 输出进入任务日志。
- cookie、token、authorization 等敏感内容必须打码。
- UI 只展示最近 N 行，完整日志落盘。

## 8. UI 信息架构

### 8.1 主页面：解析与下载

目标是成为第一屏，不做营销式首页。

区域：

- 顶部输入栏：URL/BV/AV/EP/SS 输入，解析按钮，停止按钮。
- 左侧摘要：封面、标题、UP、BV/AV、发布时间、时长、保存路径。
- 中央流信息：按分 P 展开，视频流和音频流分组。
- 右侧或底部下载参数：下载内容、分 P 范围、画质优先级、编码优先级、开始下载。
- 底部实时日志：解析/下载输出，支持复制和清空。

旧版能力继承：

- 封面预览。
- 分 P 树形展示。
- 视频流标色。
- 解析缓存。
- 默认分 P 为 `1`，可选 `ALL`。

### 8.2 任务页面

- 当前队列。
- 任务状态：排队、运行、停止中、完成、失败、取消。
- 每个任务可查看命令预览、日志、输出目录。
- 支持停止、重试、复制命令、打开目录。

### 8.3 批量页面

批量能力分两层：

- 分 P 批量：对一个视频/番剧/合集选择多个分 P。
- 输入批量：一次输入多个 URL/BV/EP/SS，生成多个任务。

MVP 先做分 P 批量，P1 做多输入批量。

### 8.4 设置页面

- 工具路径：BBDown、ffmpeg、mp4box、aria2c。
- 默认下载目录。
- API 模式：WEB/TV/APP/INTL。
- Cookie、Access Token、User Agent。
- 默认下载行为：多线程、HTTP、PCDN、aria2c、mp4box、AI 字幕、混流。
- 文件命名模板。
- 高级网络：upos host、BiliPlus host、ep host、area。

### 8.5 日志页面

- 应用日志。
- 任务日志。
- BBDown 原始输出。
- 错误报告导出。

## 9. 流信息标色规范

旧版 Python GUI 根据流文本给 Treeview 加 tag。新版应改成 badge + 颜色双表达，不只靠颜色。

| 条件 | badge | 建议颜色 | 说明 |
| --- | --- | --- | --- |
| 包含 `8K` | `8K` | 金色 | 最高分辨率突出 |
| 包含 `杜比`、`Dolby`、`Atmos` | `Dolby` | 蓝色 | 杜比视界/全景声 |
| 包含 `HDR` | `HDR` | 珊瑚红 | HDR 真彩 |
| 包含 `4K` | `4K` | 洋红 | 4K 超清 |
| 包含 `1080P 高码率` 或 `60.0` | `高码率/高帧率` | 紫色 | 高质量 1080P |
| 普通流 | 无或 `普通` | 中性灰 | 默认 |
| 错误 | `错误` | 红色 | 解析/下载失败 |

## 10. 命令构建原则

禁止：

```text
BBDown "{url}" --work-dir "{dir}" ...
```

应该：

```rust
Command::new(bbdown_path)
  .arg(input)
  .arg("--work-dir")
  .arg(work_dir)
```

每个 BBDown 参数由一个独立字段控制，最终输出 `Vec<String>`。前端的“复制命令”功能可以由后端把参数安全格式化为只读预览，不作为执行入口。

## 11. 配置和数据目录

建议：

- 配置：系统应用配置目录，例如 `AppConfig/BBDownNext/config.json`。
- 日志：系统应用日志目录。
- 历史：应用数据目录，可先用 JSON，后续迁移 SQLite。
- 下载：用户自选目录，默认使用系统下载目录下的 `BBDownNext`。
- sidecar：只读资源目录，不写运行数据。

## 12. 兼容性和风险

### 12.1 BBDown 已归档

BBDown 官方仓库在 2026-05-14 被归档。最新 Release 页面显示 v1.6.3 发布于 2024-08-14。新版应将 BBDown 视为外部、可能停止演进的依赖。

措施：

- 启动时检测 BBDown 版本。
- 文档和 UI 明确支持目标版本为 v1.6.3。
- 输出解析器必须有 fixture 测试。
- 用户可替换 BBDown 可执行文件。

### 12.2 输出解析不稳定

BBDown `-info --show-all` 输出是人类可读文本，不是稳定 JSON API。解析器必须：

- 对字段缺失宽容。
- 保留原始输出。
- 对不同编码尝试 UTF-8、GBK。
- 对错误输出提取 `message`。

### 12.3 权限和隐私

- Cookie/token 默认不显示明文。
- 日志打码。
- 不自动上传日志。
- 不在仓库或安装目录保存用户数据。

