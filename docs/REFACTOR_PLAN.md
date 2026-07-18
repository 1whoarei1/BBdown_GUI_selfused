# BBDown Next 重构计划

## 1. 目标

在保留现有 BBDown 1.6.3 下载能力的前提下，把 BBDown Next 从“可运行原型”重构为边界清楚、可测试、可持续迭代的桌面下载管理器。

核心目标：

- 解析详情达到原 JiJiDown 的信息完整度和交互质量。
- BBDown 只负责它擅长的流解析、鉴权兼容和下载。
- 封面与元数据不再依赖第二次 `--cover-only` 子进程。
- 前端从单个大组件拆为按业务组织的功能模块。
- 任务、设置和日志具备稳定的数据契约与迁移机制。
- 安全边界满足桌面 WebView 应用的基本要求。

## 2. 非目标

- 不重写 BBDown 的下载协议、混流器或登录实现。
- 不把 `BBDown serve` 作为主架构；本地子进程仍是唯一执行入口。
- 不一次性实现所有 B 站内容 API。
- 不照搬 JiJiDown 的品牌、素材、文案和深色视觉。
- 不在重构期间顺手加入无关高级功能。

## 3. 当前基线

2026-07-18 验证结果：

- `npm run build`：通过。
- `cargo test`：33/33 通过。
- Rust 警告：`DownloadingPart` 和 `TaskLogStream::System` 尚未使用。
- BBDown：1.6.3，SHA-256 `EB8B985AF07C4757FA695204283208AEE879BF79F6462A1D161E3A55B5A19CB1`。
- 普通视频真实 `-info` 解析成功。

重构期间每个阶段都必须保持上述构建和测试基线不退化。

## 4. 目标目录边界

后端逐步调整为：

```text
src-tauri/src/
  application/
    parse_service.rs
    download_service.rs
  bilibili/
    input.rs
    metadata.rs
    models.rs
  bbdown/
    command_builder.rs
    parser.rs
    runner.rs
  media/
    cover_cache.rs
  tasks/
    manager.rs
    events.rs
    models.rs
  config/
    store.rs
    migration.rs
    secrets.rs
  commands/
    parse.rs
    download.rs
    config.rs
```

前端逐步调整为：

```text
src/
  app/
    AppShell.tsx
    routes.ts
  features/
    parse/
    downloads/
    tasks/
    settings/
    logs/
  components/
    controls/
    layout/
    feedback/
  stores/
    settings.ts
    tasks.ts
    workspace.ts
  lib/
    api.ts
    format.ts
  types/
```

目录结构是目标边界，不要求一次移动所有文件。每次移动必须由实际功能改动驱动。

## 5. 核心接口契约

### 5.1 元数据模型

```rust
struct VideoMetadata {
    aid: u64,
    bvid: String,
    title: String,
    description: String,
    cover_url: String,
    owner: OwnerMetadata,
    duration_seconds: u64,
    pages: Vec<PageMetadata>,
}

struct PageMetadata {
    page_number: u32,
    cid: u64,
    title: String,
    duration_seconds: u64,
}
```

### 5.2 解析结果

`ParseResultV2` 应明确区分：

- `metadata`：稳定详情字段。
- `parts`：按 CID 合并后的分 P 与流列表。
- `cover`：远程 URL、缓存状态、本地路径。
- `capabilities`：能否下载、登录限制、API 模式。
- `diagnostics`：警告和脱敏日志引用。

不再把完整 BBDown 原始输出作为常规前端响应字段。

### 5.3 错误模型

错误至少包含：

```text
code
stage
message
retryable
task_id
log_ref
```

前端不通过解析中文错误字符串判断状态。

## 6. 分阶段实施

### 阶段 0：冻结行为和补齐样本

改动：

- 保存当前配置、命令构建和解析契约。
- 加入本次真实普通视频输出 fixture，并删除时效媒体 URL。
- 增加多 P、番剧、错误、无 UP 主名称等 fixture。
- 为 `AppConfig` 增加 `schemaVersion` 设计，但暂不改变用户配置。

验收：

- 现有 18 个 Rust 测试继续通过。
- 每类输出至少一个 fixture 测试。
- fixture 不包含 Cookie、token 或媒体签名 URL。

### 阶段 1：普通视频元数据适配器

改动：

- 引入受限配置的 HTTP client。
- 实现输入标准化和 BV/AV 提取。
- 实现 `x/web-interface/view` 响应模型。
- 实现元数据与 BBDown 分 P 按 CID 合并。
- 保留 BBDown-only 降级路径。

验收：

- `BV1btKG6PEKU` 返回正确标题、UP 主、封面和 CID。
- 带跟踪参数和纯 BV 输入得到相同业务结果。
- 元数据接口失败时仍能展示 BBDown 流信息。
- HTTP 逻辑通过固定 JSON fixture 测试，不依赖在线网络跑单测。

### 阶段 2：封面缓存替换

改动：

- 新增应用缓存目录下的 `covers/`。
- 以 BV/AV + URL 哈希命名缓存文件。
- 校验协议、响应大小、MIME 和扩展名。
- 使用临时文件 + rename 原子写入。
- 删除解析阶段的第二次 `--cover-only` 调用和文件名反推 UP 主逻辑。

验收：

- 一次普通视频解析只启动一个 BBDown 进程。
- 下载目录不再出现解析封面副作用。
- 缓存命中不重复请求。
- 封面失败不影响元数据和流信息展示。

### 阶段 3：后端应用服务与任务生命周期

改动：

- 把 Tauri command 变成薄适配层。
- `ParseService` 统一编排元数据、BBDown、合并和封面缓存。
- `BBDownRunner` 统一子进程启动、输出读取、取消和脱敏。
- 修正任务阶段，真正使用下载阶段或删除无效枚举。
- 原始日志落应用日志目录，前端只接收有限行事件。

验收：

- 任一时刻一个任务只有一个可停止的活动子进程句柄。
- 取消发生在元数据、BBDown 或封面阶段都能收敛到 `canceled`。
- 终态不可被后续事件覆盖。
- 日志中不出现 Cookie、token 或完整媒体签名 URL。

### 阶段 4：前端工作台重组

改动：

- 拆分当前 `App.tsx`，不同时重写全部样式。
- 首页保持一个主解析入口。
- 结果采用“摘要 + 分 P 下载单元 + 拆分动作”。
- 下载页集中统计、限速、完成后动作和批量操作。
- 设置页按常规、工具、账号、下载、网络和外观分组。
- Zustand 只管理跨页面状态；局部弹窗状态继续使用 React state。

验收：

- `App.tsx` 只负责壳和路由，不持有领域流程。
- 普通视频从输入到创建下载任务不超过三个主要决策步骤。
- 分 P 支持搜索、全选、清空和范围摘要。
- 窗口在 1000x720 与 1440x900 下无重叠和不可达控件。
- 前端关键 store/formatter 有 Vitest 测试。

### 阶段 5：设置、账号和安全收口

改动：

- 配置增加 schema 版本和迁移。
- Cookie/token 从普通 `AppConfig` 响应中分离。
- Windows 使用 DPAPI 或 Credential Manager 保存敏感信息。
- UI 只获得登录状态和掩码摘要。
- 配置明确 CSP，并收窄 asset protocol scope。

验收：

- 升级旧配置不丢下载目录和工具路径。
- 前端状态、日志和命令预览不包含明文凭据。
- 非缓存目录无法通过 asset protocol 任意读取。
- CSP 下主工作流、封面和二维码均正常。

### 阶段 6：队列、历史和发布

改动：

- 有限并发任务队列。
- 任务历史、重试、清理和打开目录。
- BBDown/ffmpeg 首次启动检测与引导。
- sidecar/便携包策略和签名发布流程。

验收：

- 并发数和队列顺序可配置且重启后保留。
- 失败任务保留可读错误和日志引用。
- 新 Windows 用户无需手动定位 BBDown 即可完成首次引导。
- 发布包和内部可执行文件均有版本、哈希与签名记录。

## 7. 测试策略

### Rust

- 输入标准化单元测试。
- B 站 JSON fixture 反序列化测试。
- BBDown 文本 fixture 解析测试。
- CID 合并与降级测试。
- CoverCache 临时目录测试。
- TaskManager 状态机和取消竞态测试。
- 命令构建与凭据脱敏测试。

### 前端

- 设置迁移和 store 测试。
- 分 P 选择、范围格式化测试。
- 下载任务状态 reducer 测试。
- 解析结果摘要和错误状态组件测试。

### 集成

- 使用本地 `bin/BBDown.exe` 的手动冒烟测试。
- 普通单 P、普通多 P、番剧 EP、番剧 SS 四条固定样本。
- 在线测试不进入默认 CI，避免网络和账号导致不稳定。

## 8. 风险与决策

| 风险 | 决策 |
| --- | --- |
| B 站接口变化 | provider 隔离 + JSON fixture + BBDown 降级 |
| BBDown 控制台文案变化 | parser fixture + 原始日志引用 + 版本检测 |
| 两侧分 P 不一致 | CID 优先，页码退化，冲突产生 warning |
| 登录内容权限差异 | BBDown 结果决定可下载能力，元数据只负责展示 |
| 重构范围过大 | 每阶段可独立合并，禁止前后端同时整体改写 |
| 用户配置损坏 | schema 迁移前备份并支持回滚 |

## 9. 当前实施状态

2026-07-18 已完成：

1. `parse_video_v2` 已成为主界面解析入口，保留旧 command 作为兼容对照。
2. 首页改为突出解析入口；每次解析创建独立、可切换、可关闭的页签。
3. 解析详情展示封面、作者头像、简介、视频与作者跳转、分 P、CID 和音视频流。
4. 下载入口收敛为解析后的“下载视频”，同时保留画质、编码和音频优先级。
5. 下载任务页区分当前任务与已完成历史；设置页增加打开下载目录按钮。
6. 账号入口统一为扫码或 Cookie 登录；右上角持续显示登录状态、头像、UID 和会员信息。
7. 后端可从手工 Cookie 或 BBDown 扫码生成的 `BBDown.data` 获取账号公开资料，日志不输出凭据。
8. MP4Box 和 aria2c 已从设置界面移除，旧配置字段暂时保留用于向后兼容。
9. `App.tsx` 已拆出账号弹窗和工作台组件；前端生产构建、Rust 格式检查和 33 个测试均通过。
10. `BV1btKG6PEKU` 在线冒烟通过：账号识别有效，BBDown 返回 aid、CID、视频流和音频流，未启动下载。
11. 账号弹窗支持本机退出：清空 Cookie/token，并删除 BBDown 扫码凭据和二维码。

下一批工作聚焦阶段 2 和阶段 5：封面缓存、敏感配置独立存储、配置 schema 迁移以及前端关键状态测试。旧配置兼容字段在迁移机制落地前不删除。
