# BBDown Next

​	BBDown Next 是一个面向 Windows 的 BBDown 桌面图形界面，使用 Tauri 2、React、TypeScript 和 Rust 构建。项目负责视频信息展示、账号状态、解析页签、下载配置和任务管理，实际媒体解析与下载由用户本机的 BBDown 执行。

> 本工具只是提供一个GUI，方便调用bbdown.exe和ffmpeg.exe，并展示信息。

## 功能

-  B 站视频链接解析。
- 多解析页签，保留已解析的视频信息。
- 展示封面、作者、简介、分 P、CID 和音视频流。
- 扫码或 Cookie 登录，持续显示账号状态；退出时删除本机账号凭据。
- 画质、编码、音频优先级和下载内容配置。
- 当前下载与历史任务列表。

## 页面展示

![image-20260718222323583](./README.assets/image-20260718222323583.png)


## 运行要求

- Windows 10/11。
- Microsoft Edge WebView2 Runtime。
- BBDown 1.6.3
- FFmpeg。

源码仓库和当前安装包配置不包含 `BBDown.exe` 或 `ffmpeg.exe`。启动应用后可在“设置”中打开两个工具的官方下载页面并配置本机路径。

## 开发

需要 Node.js 22、npm 和 Rust stable。克隆仓库后执行：

```powershell
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

只预览前端：

```powershell
npm run dev
```

前端预览不会调用 Tauri 后端、BBDown 或真实登录流程。

## 构建

生成 Windows 应用和安装包：

```powershell
npm run tauri build
```

构建产物位于 `src-tauri/target/release/`

## 数据目录

- 应用配置：`%APPDATA%\com.bbdown.next\config.json`
- 默认下载目录：`%USERPROFILE%\Downloads\BBDown Next`
- BBDown 扫码凭据：与配置的 `BBDown.exe` 位于同一目录

退出登录会清空应用配置中的 Cookie/token，并删除当前 BBDown 目录内的 `BBDown.data`、`BBDownTV.data` 和 `qrcode.png`。

## MIT许可

[MIT License](LICENSE)
