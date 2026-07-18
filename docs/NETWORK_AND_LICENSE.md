# 网络、第三方组件与 JiJiDown 边界

本文记录 BBDown Next 当前实现的网络访问和发布边界。结论基于 2026-07-18 的源码与打包配置审计。

## 运行时网络访问

BBDown Next 自身直接访问：

- `https://api.bilibili.com/x/web-interface/view`：视频标题、简介、封面、作者和分 P。
- `https://api.bilibili.com/x/web-interface/nav`：当前 B 站账号公开信息与登录状态。
- B 站 `hdslb.com` 图片域名：在界面中显示封面和头像。

用户点击跳转按钮时，系统浏览器会打开 `bilibili.com` 页面。解析和下载时，本地 `BBDown.exe` 会访问 B 站 API 与媒体 CDN。高级配置中的自定义 host 默认为空，当前界面也不提供第三方解析服务器入口。

当前源码没有 JiJiDown API、JiJiDown 更新服务、广告、遥测、崩溃上报或统计服务。npm 镜像和 Rust crate registry 只用于开发依赖安装，不是安装包运行时服务。

设置页的工具下载按钮只在用户点击后打开 BBDown 官方 GitHub Releases 或 FFmpeg 官方下载页，应用不会在后台请求这两个站点。

## 第三方组件

- BBDown 1.6.3：独立本地程序，官方仓库许可证为 MIT，版权声明为 `Copyright (c) 2020 nilaoda`。
- FFmpeg：独立本地程序，具体许可取决于所使用构建的编译选项，可能涉及 LGPL 或 GPL。
- Tauri、React、Lucide、reqwest 等：开源依赖，各自许可证以锁文件对应版本为准。

当前 Tauri 安装包没有把 `BBDown.exe` 或 `ffmpeg.exe` 打包进去。未来若随安装包分发它们，必须同时提供对应许可证、版权声明和必要的源代码/获取方式，尤其应先核对 FFmpeg 构建的许可证。

## 与 JiJiDown 的关系

`bbdown-next/src` 和 `bbdown-next/src-tauri` 没有引用 JiJiDown 域名、服务、代码或资源。产品名称、浅色界面、图标和实现代码均与 JiJiDown 分离；JiJiDown 只作为功能流程和信息架构研究对象。

仓库根目录的 `reverse/`、`JiJiDown.exe` 和 `JiJiDown_setup.exe` 含有第三方二进制或提取资源，只能作为本地兼容性研究证据，不属于 BBDown Next 发布内容。根目录 `.gitignore` 已默认排除这些文件，但若它们曾被 Git 跟踪，发布前仍需从索引和历史中单独清理。

通常，学习产品思路、操作流程和事实性接口行为，与复制源代码、图片、图标、音效、独特文案或品牌标识不是同一风险。为降低风险，公开发布时应继续满足：

- 不使用 JiJiDown 名称、Logo、图标、音效和提取的前端文件。
- 不宣称由 JiJiDown 官方提供、授权或兼容背书。
- 不连接 JiJiDown 的统计、更新、解析或下载服务。
- 保留独立设计和独立实现的提交记录与技术说明。
- 遵守 B 站服务条款、账号规则和视频内容权利要求。

本说明是工程审计结论，不替代针对具体发行地区、商业模式和素材来源的正式法律意见。
