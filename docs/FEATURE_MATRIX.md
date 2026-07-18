# 功能矩阵

本文档把 BBDown v1.6.3 能力、旧 Python GUI 已做能力和新版计划放在同一张地图里，避免开发时漏功能。

状态说明：

- `MVP`：第一版必须做。
- `P1`：第一版后优先补齐。
- `P2`：高级功能，待核心稳定后做。
- `Manual`：先提供配置入口或命令预览，不急着做完整交互。

## 1. 内容类型

| 能力 | BBDown 支持 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| 普通视频 | 支持 WEB/TV/APP | 基于 URL/BV 输入 | MVP |
| 番剧 | 支持 WEB/TV/APP，EP/SS | 可通过 URL，分 P/全集依赖 `-p` | MVP |
| 课程 | 支持 WEB | 未做专门 UI | P1 |
| 合集/列表/收藏夹/个人空间 | 支持解析 | README 提到视频合集不能完善批选 | P1 |
| 互动视频 | v1.6.3 Release 提到支持 | 未做专门 UI | P1/P2 |
| 国际版内容 | `--use-intl-api` | API 模式有 INTL | MVP |

## 2. 解析能力

| 能力 | BBDown 参数 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| 仅解析不下载 | `-info`/`--only-show-info` | 已用 | MVP |
| 展示所有分 P | `--show-all` | 已用 | MVP |
| 隐藏流信息 | `-hs`/`--hide-streams` | 未做 | P2 |
| 交互式选择清晰度 | `-ia`/`--interactive` | 未做 | 不推荐，GUI 自己选择 |
| 错误信息提取 | 输出 `message` | 简单正则 | MVP，结构化错误 |
| 解析缓存 | 无 BBDown 参数 | 内存 `parse_cache` | MVP，内存缓存；P1 持久缓存 |
| 封面预览 | `--cover-only` | 已做，下载封面后读取 | MVP |

## 3. 下载内容

| 能力 | BBDown 参数 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| 下载视频+音频 | 默认 | 已做 | MVP |
| 仅视频 | `--video-only` | 已做 | MVP |
| 仅音频 | `--audio-only` | 已做 | MVP |
| 仅弹幕 | `--danmaku-only` | 已做 | MVP |
| 仅字幕 | `--sub-only` | 已做 | MVP |
| 仅封面 | `--cover-only` | 已做 | MVP |
| 同时下载弹幕 | `-dd`/`--download-danmaku` | 已做 | MVP |
| 跳过字幕 | `--skip-subtitle` | 通过字幕开关反向生成 | MVP |
| 跳过封面 | `--skip-cover` | 通过封面开关反向生成 | MVP |
| 跳过混流 | `--skip-mux` | 通过“混流&命名”开关反向生成 | MVP |
| 跳过 AI 字幕 | `--skip-ai` | 已做 | MVP |

## 4. 分 P 和批量

| 能力 | BBDown 参数 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| 单 P 下载 | `-p 1` | 默认 `1` | MVP |
| 多 P 下载 | `-p 1,2,10` | 可手输/弹窗选择 | MVP |
| 范围下载 | `-p 3-5` | `smart_range_format` 已做 | MVP |
| 全部分 P | `-p ALL` | 全选时生成 `ALL` | MVP |
| 最后一 P | `-p LAST` | 未做 UI | P1 |
| 最新一 P | `-p LATEST` | 未做 UI | P1 |
| 混合选择 | `-p 3,5,LATEST` | 未做特殊 UI | P1 |
| 批量输入多个链接 | 多任务队列 | 未做 | P1 |
| 批量任务队列 | 应用层能力 | 未做 | P1 |

## 5. 画质、编码和流标识

| 能力 | BBDown 参数 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| 编码优先级 | `-e`/`--encoding-priority` | 已做，有顺序选择弹窗 | MVP |
| 画质优先级 | `-q`/`--dfn-priority` | 已做，有顺序选择弹窗 | MVP |
| 视频体积升序 | `--video-ascending` | 已做 | MVP |
| 音频体积升序 | `--audio-ascending` | 已做 | MVP |
| 8K | 通过画质优先级和账号能力 | Treeview 金色 | MVP，badge+标色 |
| HDR | 通过画质优先级和账号能力 | Treeview 珊瑚红 | MVP，badge+标色 |
| 杜比视界 | 通过画质优先级和账号能力 | Treeview 蓝色 | MVP，badge+标色 |
| 杜比全景声 | 音频流能力 | 音频流蓝色 | MVP，badge+标色 |
| 4K | 通过画质优先级 | Treeview 洋红 | MVP，badge+标色 |
| 1080P 高码率/60fps | 通过画质优先级 | Treeview 紫色 | MVP，badge+标色 |

## 6. API 和账号

| 能力 | BBDown 参数/命令 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| WEB API | 默认 | 已做 | MVP |
| TV API | `-tv`/`--use-tv-api` | 已做 API 模式 | MVP |
| APP API | `-app`/`--use-app-api` | 已做 API 模式 | MVP |
| 国际版 API | `-intl`/`--use-intl-api` | 已做 API 模式 | MVP |
| Cookie | `-c`/`--cookie` | 已做输入框 | MVP |
| Access Token | `-token`/`--access-token` | 已做输入框 | MVP |
| User Agent | `-ua`/`--user-agent` | 配置里有，UI 未做 | P1 |
| WEB 扫码登录 | `login` | 已做入口 | MVP |
| TV 扫码登录 | `logintv` | 已做入口 | MVP |
| 自动刷新 cookie | BBDown TODO | 未做 | 不承诺，P2 研究 |

## 7. 外部工具和混流

| 能力 | BBDown 参数 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| ffmpeg 路径 | `--ffmpeg-path` | 已做 | MVP |
| mp4box 路径 | `--mp4box-path` | 只有使用开关，无路径 UI | P1 |
| 使用 MP4Box | `--use-mp4box` | 已做开关 | MVP |
| aria2c 路径 | `--aria2c-path` | 没有路径 UI | P1 |
| 使用 aria2c | `-aria2`/`--use-aria2c` | 已做开关 | MVP |
| aria2c 参数 | `--aria2c-args` | 配置有，UI 未做 | P1 |
| 多线程下载 | `-mt`/`--multi-thread` | 已做 | MVP |

## 8. 网络和地区

| 能力 | BBDown 参数 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| 强制 HTTP | `--force-http` | 已做 | MVP |
| upos host | `--upos-host` | 未做 | P1 |
| 强制替换 host | `--force-replace-host` | 未做 | P1 |
| 允许 PCDN | `--allow-pcdn` | 已做 | MVP |
| BiliPlus host | `--host` | 未做 | Manual/P2 |
| BiliPlus EP host | `--ep-host` | 未做 | Manual/P2 |
| BiliPlus area | `--area hk/tw/th` | 未做 | Manual/P2 |

## 9. 输出和文件命名

| 能力 | BBDown 参数 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| 工作目录 | `--work-dir` | 已做 | MVP |
| 单 P 文件名模板 | `-F`/`--file-pattern` | 已做 | MVP |
| 多 P 文件名模板 | `-M`/`--multi-file-pattern` | 已做 | MVP |
| 混流语言 | `--language` | 已做 | MVP |
| 下载归档去重 | `--save-archives-to-file` | 未做 | P1 |
| BBDown 配置文件 | `--config-file` | 未做 | P1/Manual |
| 打开输出目录 | 应用层能力 | 未做 | MVP |

文件名模板变量必须在 UI 中提供插入菜单：

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

## 10. 命令和服务模式

| 能力 | BBDown 命令 | 旧 Python GUI | 新版计划 |
| --- | --- | --- | --- |
| WEB 登录 | `BBDown login` | 已做 | MVP |
| TV 登录 | `BBDown logintv` | 已做 | MVP |
| API 服务器模式 | `BBDown serve -l <url>` | 未做 | P2 |
| 版本检测 | `--help` 输出含版本 | 未做 | MVP |
| Debug 日志 | `--debug` | 未做 UI | P1 |

## 11. 旧版 GUI 特色迁移

| 旧版特色 | 新版处理 |
| --- | --- |
| “首页 / 设置 / 账号”三页 | 改为主工作台、任务、设置、日志 |
| 萌化文案 | 可以保留少量风格，但默认做成稳定工具风 |
| 封面自适应缩放 | 保留 |
| 分 P 弹窗选择 | 保留并增强搜索/全选/反选 |
| 画质和编码顺序选择弹窗 | 保留并增强拖拽排序 |
| 流信息标色 | 保留，改为 badge+颜色 |
| 解析缓存 | 保留，后续可持久化 |
| 自动检测 bin 下 BBDown/ffmpeg | 保留，同时支持用户目录和 sidecar |
| 日志系统 | 重做为任务日志+应用日志 |

