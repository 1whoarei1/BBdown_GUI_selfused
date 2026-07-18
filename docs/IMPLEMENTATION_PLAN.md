# 实施路线

## 阶段 0：文档和脚手架

目标：把项目边界钉住，不急着写业务。

- 建立 `bbdown-next` 项目目录。
- 完成功能矩阵。
- 完成技术设计。
- 确定 MVP 范围。
- 决定是否用 `pnpm create tauri-app` 初始化正式工程。

验收：

- 文档能指导开发，不依赖口头记忆。
- 旧 Python GUI 的已有能力都在矩阵中有去向。
- BBDown v1.6.3 参数都至少有记录。

## 阶段 1：正式工程骨架

目标：跑起空 Tauri 应用。

- 初始化 Tauri 2 + React + TypeScript + Vite。
- 配置 ESLint、Prettier、Vitest。
- 配置 Rust 模块结构。
- 建立前后端共享类型策略。
- 建立基础布局：侧栏、主区域、日志区域。

验收：

- `pnpm dev` 能启动桌面应用。
- Rust command 示例可被前端 invoke。
- 前端能接收一个后端 event。

## 阶段 2：配置和工具检测

目标：先把“工具路径”和“用户配置”做稳。

- 实现 `AppConfig`。
- 实现配置读写和默认配置。
- 实现 BBDown、ffmpeg、mp4box、aria2c 路径检测。
- 实现 BBDown 版本检测。
- 实现设置页。

验收：

- 用户能选择 BBDown 和 ffmpeg。
- 重启应用后配置保留。
- 找不到工具时有明确错误。
- 日志不会写到源码目录。

## 阶段 3：解析 MVP

> 当前实现已完成基于 BBDown 控制台输出的 MVP，但封面仍通过第二次
> `--cover-only` 子进程获取。后续替换方案和迁移顺序以
> [重构计划](REFACTOR_PLAN.md) 与
> [元数据调用链](METADATA_PIPELINE.md) 为准。

目标：输入 URL/BV 后能结构化展示结果。

- 实现 `parse_video` command。
- 用 `BBDown <input> -info --show-all -p ALL` 获取输出。
- 实现 UTF-8/GBK 输出解码。
- 实现错误信息提取。
- 实现标题、UP、发布时间、时长、BV、分 P、视频流、音频流解析。
- 实现封面获取和展示。
- 实现解析缓存。

验收：

- 普通视频解析可展示封面和流信息。
- 多分 P 视频可展示分 P 列表。
- 解析失败时 UI 有可读错误和原始日志。
- 解析器有 fixture 单元测试。

## 阶段 4：下载 MVP

目标：覆盖旧 Python GUI 的下载能力。

- 实现 `DownloadOptions` 到 BBDown 参数数组的转换。
- 实现开始下载、停止任务。
- 实现实时日志输出。
- 实现下载完成/失败状态。
- 实现打开输出目录。
- 支持下载内容开关：视频、音频、弹幕、字幕、封面。
- 支持画质、编码、分 P 范围、混流、AI 字幕、多线程、强制 HTTP、PCDN、MP4Box、aria2c。

验收：

- 可以下载单 P。
- 可以下载 `ALL`。
- 可以选择多个分 P。
- 可以停止正在运行的任务。
- 命令预览和实际执行参数一致。

## 阶段 5：任务队列和历史

目标：从“单任务工具”变成“下载管理器”。

- 任务队列。
- 多 URL 批量输入。
- 历史记录。
- 重试任务。
- 复制命令。
- 清理已完成任务。

验收：

- 多任务按队列运行。
- 失败任务保留日志和可重试。
- 历史记录可搜索。

## 阶段 6：高级功能补齐

目标：覆盖 BBDown 的边角能力。

- User Agent。
- aria2c 路径和参数。
- mp4box 路径。
- upos host。
- force replace host。
- save archives。
- config file。
- BiliPlus host、ep host、area。
- debug 模式。
- serve 模式。

验收：

- 功能矩阵中 P1/P2 项完成或明确延期。
- 高级参数不影响 MVP 使用体验。

## 阶段 7：打包和发布

目标：可以交付给普通用户使用。

- Windows 打包。
- 可选 sidecar 策略。
- 安装后首次引导。
- 日志导出。
- 崩溃和错误报告本地生成。
- README 和截图。

验收：

- 新机器上可安装运行。
- 首次启动能指引用户配置 BBDown/ffmpeg。
- 用户数据不写进安装目录。

## MVP 明确范围

必须包含：

- 工具路径配置。
- URL/BV/EP/SS 输入。
- 解析和封面展示。
- 分 P 展示与批量选择。
- 视频/音频流展示和标色。
- 基本下载参数。
- 开始/停止下载。
- 实时日志。
- WEB/TV/APP/INTL API 模式。
- Cookie/token 输入。
- WEB/TV 扫码登录入口。

暂不包含：

- serve 模式。
- 自动刷新 cookie。
- 自动下载和更新 BBDown。
- 多平台完整打包。
- 复杂代理和 BiliPlus 图形化向导。
