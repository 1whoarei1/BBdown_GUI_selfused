# BBDown CLI 能力参考

本文件基于三类来源整理：

- 本仓库旧版 `BBdown说明文档.md`。
- 本地 `bin/BBDown.exe --help` 输出。
- BBDown GitHub Release v1.6.3 页面。

本地检测结果：

- `bin/BBDown.exe --help` 输出版本：`BBDown version 1.6.3, Bilibili Downloader.`
- `bin/BBDown.exe --version` 在当前二进制上会因为缺少 URL 报错，所以版本检测应优先解析 `--help` 第一行。

## 1. 基本用法

```text
BBDown <url> [command] [options]
```

`<url>` 可以是：

- 视频地址
- `av`
- `bv`/`BV`
- `ep`
- `ss`

## 2. API 模式

| 参数 | 含义 | 新版 UI |
| --- | --- | --- |
| `-tv`, `--use-tv-api` | TV 端解析模式 | API 模式选择 |
| `-app`, `--use-app-api` | APP 端解析模式 | API 模式选择 |
| `-intl`, `--use-intl-api` | 国际版解析模式 | API 模式选择 |

默认不传参数即 WEB。

## 3. 解析和显示

| 参数 | 含义 |
| --- | --- |
| `-info`, `--only-show-info` | 仅解析而不下载 |
| `--show-all` | 展示所有分 P 标题 |
| `-ia`, `--interactive` | 交互式选择清晰度 |
| `-hs`, `--hide-streams` | 不显示所有可用音视频流 |
| `--debug` | 输出调试日志 |

GUI 应优先使用非交互式模式，不使用 `-ia`。

## 4. 下载内容

| 参数 | 含义 |
| --- | --- |
| `--video-only` | 仅下载视频 |
| `--audio-only` | 仅下载音频 |
| `--danmaku-only` | 仅下载弹幕 |
| `--sub-only` | 仅下载字幕 |
| `--cover-only` | 仅下载封面 |
| `-dd`, `--download-danmaku` | 下载弹幕 |
| `--skip-mux` | 跳过混流步骤 |
| `--skip-subtitle` | 跳过字幕下载 |
| `--skip-cover` | 跳过封面下载 |
| `--skip-ai` | 跳过 AI 字幕下载，默认开启 |

## 5. 画质和编码

| 参数 | 含义 | 示例 |
| --- | --- | --- |
| `-e`, `--encoding-priority` | 视频编码优先级 | `hevc,av1,avc` |
| `-q`, `--dfn-priority` | 画质优先级 | `8K 超高清, 1080P 高码率, HDR 真彩, 杜比视界` |
| `--video-ascending` | 视频体积升序，最小体积优先 | 无值 |
| `--audio-ascending` | 音频体积升序，最小体积优先 | 无值 |

BBDown 支持 AVC/HEVC/AV1，支持 8K/HDR/杜比视界/杜比全景声下载，但实际可用性取决于账号、内容和接口。

## 6. 分 P 选择

| 参数 | 含义 | 示例 |
| --- | --- | --- |
| `-p`, `--select-page` | 选择指定分 P 或范围 | `8`、`1,2`、`3-5`、`ALL`、`LAST`、`3,5,LATEST` |

新版 UI 需要支持：

- 单选。
- 多选。
- 范围。
- 全选。
- 反选。
- 手动输入高级表达式。

## 7. 文件命名和输出

| 参数 | 含义 |
| --- | --- |
| `-F`, `--file-pattern` | 单 P 存储文件名 |
| `-M`, `--multi-file-pattern` | 多 P 存储文件名 |
| `--language` | 混流的音频语言代码，例如 `chi`、`jpn` |
| `--work-dir` | 工作目录 |
| `--save-archives-to-file` | 将下载过的视频记录到本地文件，用于跳过重复下载 |
| `--config-file` | 读取指定 BBDown 配置文件 |

可用模板变量：

- `<videoTitle>`
- `<pageNumber>`
- `<pageNumberWithZero>`
- `<pageTitle>`
- `<bvid>`
- `<aid>`
- `<cid>`
- `<dfn>`
- `<res>`
- `<fps>`
- `<videoCodecs>`
- `<videoBandwidth>`
- `<audioCodecs>`
- `<audioBandwidth>`
- `<ownerName>`
- `<ownerMid>`
- `<publishDate>`
- `<videoDate>`
- `<apiType>`

## 8. 账号和鉴权

| 参数/命令 | 含义 |
| --- | --- |
| `-c`, `--cookie` | 设置字符串 cookie，用于 WEB 接口会员内容 |
| `-token`, `--access-token` | 设置 access_token，用于 TV/APP 接口会员内容 |
| `-ua`, `--user-agent` | 指定 user-agent |
| `login` | 通过 APP 扫码登录 WEB 账号 |
| `logintv` | 通过 APP 扫码登录 TV 账号 |

隐私要求：

- UI 默认隐藏 cookie/token。
- 日志打码。
- 配置文件权限尽量收紧。

## 9. 外部工具

| 参数 | 含义 |
| --- | --- |
| `--ffmpeg-path` | 设置 ffmpeg 路径 |
| `--mp4box-path` | 设置 mp4box 路径 |
| `--aria2c-path` | 设置 aria2c 路径 |
| `--use-mp4box` | 使用 MP4Box 混流 |
| `-aria2`, `--use-aria2c` | 调用 aria2c 下载 |
| `--aria2c-args` | aria2c 附加参数 |
| `-mt`, `--multi-thread` | 多线程下载，默认开启 |

普通视频混流需要 ffmpeg 或 mp4box；杜比视界需要 ffmpeg 5.0 以上或新版 mp4box。

## 10. 网络和地区

| 参数 | 含义 |
| --- | --- |
| `--force-http` | 下载音视频时强制使用 HTTP 替换 HTTPS，默认开启 |
| `--allow-pcdn` | 不替换 PCDN 域名，仅在其他情况无法下载时使用 |
| `--upos-host` | 自定义 upos 服务器 |
| `--force-replace-host` | 强制替换下载服务器 host，默认开启 |
| `--delay-per-page` | 合集分 P 下载间隔，单位秒 |
| `--host` | 指定 BiliPlus host |
| `--ep-host` | 指定 BiliPlus EP host |
| `--area` | BiliPlus area：`hk`、`tw`、`th` |

## 11. 子命令

| 命令 | 含义 | 新版计划 |
| --- | --- | --- |
| `login` | WEB 扫码登录 | MVP |
| `logintv` | TV 扫码登录 | MVP |
| `serve` | API 服务器模式 | P2 |

`serve` 参数：

| 参数 | 含义 |
| --- | --- |
| `-l`, `--listen` | 服务器监听 URL，例如 `http://0.0.0.0:12450` |

## 12. Release 备注

BBDown v1.6.3 Release 页面列出的变化包括：

- 解决扫码登录后 Cookie 转义问题。
- 修复 TV parser。
- 支持交互/互动视频下载。
- 优化课程和番剧 EP/SS ID 解析。

该仓库在 2026-05-14 被归档，后续需要把 BBDown 本身视为外部兼容层依赖。

