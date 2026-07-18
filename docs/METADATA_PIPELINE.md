# 视频元数据、封面与分 P 调用链

本文记录原 JiJiDown、BBDown 1.6.3 和当前 BBDown Next 的数据来源，并定义重构后的职责边界。

## 1. 原 JiJiDown 的做法

根据 `JiJiDown.exe` 的静态逆向、Tauri command 名称和实际样本验证，原程序没有依赖 BBDown 获取视频详情。

普通视频的主要流程是：

1. `check_input` 识别 URL、AV、BV 等输入。
2. `get_video_detail` 请求 B 站视频详情接口。
3. 详情响应提供标题、简介、封面 URL、UP 主信息和分 P 列表。
4. `get_cover_image_data` 获取或缓存封面，再交给 WebView 展示。
5. `get_page_download_quality` 按分 P 的 `aid/bvid/cid` 查询实际可用画质。
6. 下载时才把选中的分 P、格式和画质交给下载逻辑。

二进制中可以确认的普通视频接口包括：

```text
https://api.bilibili.com/x/web-interface/view?bvid=
https://api.bilibili.com/x/web-interface/view?aid=
https://api.bilibili.com/x/player/wbi/v2?bvid=
https://api.bilibili.com/x/player/playurl?avid=
```

`x/web-interface/view` 的关键字段对应关系：

| UI 数据 | API 字段 |
| --- | --- |
| AV/BV | `aid` / `bvid` |
| 标题 | `title` |
| 简介 | `desc` |
| 封面 | `pic` |
| UP 主 | `owner.mid/name/face` |
| 总时长 | `duration` |
| 分 P | `pages[]` |
| 分 P 编号 | `pages[].page` |
| CID | `pages[].cid` |
| 分 P 标题 | `pages[].part` |
| 分 P 时长 | `pages[].duration` |

样本 `BV1btKG6PEKU` 返回一个分 P：CID `40002191561`，标题“世 纪 辐 射【上】”，时长 7444 秒。该数据与原 JiJiDown 详情页展示完全吻合。

## 2. BBDown 1.6.3 的做法

BBDown 内部同样会访问 B 站接口，但其 CLI 默认只暴露人类可读日志。真实执行：

```powershell
BBDown.exe "https://www.bilibili.com/video/BV1btKG6PEKU" `
  -info --show-all -p ALL
```

可以从输出中获得：

- `aid`
- 标题
- 发布时间
- UP 主页
- 分 P 的 CID、标题、时长
- 每个分 P 的视频流和音频流

这次真实输出没有 UP 主名称、封面 URL和简介。因此仅依靠 CLI 文本无法稳定构建完整详情页。

BBDown 的优势仍然是：

- WEB/TV/APP/INTL 多种解析模式
- Cookie/token 鉴权
- 视频和音频流枚举
- 画质、编码、大小估算
- 实际下载、字幕、弹幕、混流和外部工具调用

## 3. 当前 BBDown Next 的问题

当前 `parse_video` 会：

1. 运行一次 BBDown `-info --show-all -p ALL`。
2. 解析控制台文本得到标题、分 P 和流信息。
3. 再运行一次 BBDown `--cover-only`。
4. 把封面写入用户下载目录。
5. BBDown 没有输出 UP 主名称时，从封面文件名反推作者。

这条路径存在以下问题：

- 一次解析启动两个 BBDown 进程，延迟和失败点翻倍。
- 封面缓存污染下载目录。
- 文件名不是可靠的数据接口，无法稳定恢复作者。
- 解析取消需要处理两个串行子进程。
- `rawOutput` 包含带时效签名的媒体 URL，不应完整返回前端。
- 控制台文案变化会同时影响标题、分 P、流信息和任务阶段。

## 4. 重构后的目标调用链

普通视频采用“双来源合并”，不重新实现 BBDown 下载协议：

```text
用户输入
  -> InputNormalizer
  -> BilibiliMetadataClient ----> VideoMetadata + PageMetadata + cover URL
  -> BBDownRunner(-info) --------> StreamCatalog + BBDown diagnostics
  -> ParseResultAssembler ------> ParseResultV2
  -> CoverCache ----------------> app cache path
  -> Tauri command -------------> frontend
```

职责划分：

- `BilibiliMetadataClient`：普通视频标题、简介、作者、封面和分 P。
- `BBDownRunner`：流信息、账号能力、实际下载和兼容模式。
- `ParseResultAssembler`：优先按 CID，退化时按分 P 编号合并两侧数据。
- `CoverCache`：下载、校验 MIME/大小、原子写入应用缓存目录。
- `RawLogStore`：原始日志落盘并脱敏，前端只接收日志引用和摘要。

番剧、课程、收藏夹和国际版先保持 BBDown 主导，并通过 provider 能力标记渐进补齐，不在第一阶段一次性重写所有 B 站 API。

## 5. 降级策略

| 场景 | 行为 |
| --- | --- |
| 元数据接口成功、BBDown 成功 | 合并完整结果 |
| 元数据接口失败、BBDown 成功 | 使用 BBDown 结果，显示无封面/作者警告 |
| 元数据接口成功、BBDown 失败 | 展示详情和分 P，但禁用流选择与下载 |
| 两者都失败 | 返回结构化错误和脱敏日志引用 |
| 封面下载失败 | 保留远程 URL 或占位图，不让解析整体失败 |

## 6. 安全约束

- 输入 URL 只保留业务参数，丢弃 `spm_id_from`、`vd_source` 等跟踪参数。
- Cookie/token 不进入前端日志、命令预览和 `rawOutput`。
- 媒体 URL 查询串不进入普通应用日志。
- 封面仅允许 `http/https`，限制响应大小和图片 MIME。
- Tauri asset protocol scope 只开放应用缓存和必要目录，不能继续使用 `**`。
- 启用明确 CSP，不继续使用 `csp: null`。

