# BBDown Next

BBDown Next 是一个面向 Windows 的 BBDown 桌面图形界面，使用 Tauri 2、React、TypeScript 和 Rust 构建。项目负责视频信息展示、账号状态、解析页签、下载配置和任务管理，实际媒体解析与下载由用户本机的 BBDown 执行。

> 本项目不是哔哩哔哩、BBDown 或 JiJiDown 的官方项目，也不代表上述项目提供授权或兼容性背书。

## 功能

- BV、AV、EP、SS 和 B 站视频链接解析。
- 多解析页签，保留已解析的视频信息。
- 展示封面、作者、简介、分 P、CID 和音视频流。
- 扫码或 Cookie 登录，持续显示账号状态；退出时删除本机账号凭据。
- 画质、编码、音频优先级和下载内容配置。
- 当前下载与历史任务列表。
- BBDown、FFmpeg 工具检测及官方下载入口。
- Windows 子进程后台运行，不弹出控制台窗口。

## 运行要求

- Windows 10/11。
- Microsoft Edge WebView2 Runtime。
- BBDown 1.6.3 兼容版本。
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

构建产物位于 `src-tauri/target/release/`，不提交到 Git。正式发行应通过 GitHub Releases 上传安装包及校验值。

## 数据目录

- 应用配置：`%APPDATA%\com.bbdown.next\config.json`
- 新用户默认下载目录：`%USERPROFILE%\Downloads\BBDown Next`
- BBDown 扫码凭据：与配置的 `BBDown.exe` 位于同一目录

退出登录会清空应用配置中的 Cookie/token，并删除当前 BBDown 目录内的 `BBDown.data`、`BBDownTV.data` 和 `qrcode.png`。不会注销其他浏览器、手机或独立 BBDown 目录中的账号。

## 网络与隐私

应用自身只调用 B 站视频详情和账号状态接口；解析与下载阶段由本地 BBDown 访问 B 站 API 和媒体 CDN。项目没有 JiJiDown 服务依赖、广告、遥测、统计或崩溃上报。

完整说明见 [网络、第三方组件与 JiJiDown 边界](docs/NETWORK_AND_LICENSE.md)。

## 文档

- [开发说明](docs/DEVELOPMENT.md)
- [重构计划](docs/REFACTOR_PLAN.md)
- [功能矩阵](docs/FEATURE_MATRIX.md)
- [技术设计](docs/TECHNICAL_DESIGN.md)
- [元数据调用链](docs/METADATA_PIPELINE.md)
- [BBDown CLI 参考](docs/research/bbdown-cli-reference.md)

## 贡献

提交前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)，并确保前端构建、Rust 格式检查和测试通过。

## 许可

项目自身代码尚未声明开源许可证。在仓库所有者选择许可证前，默认保留全部权利。BBDown、FFmpeg、Tauri、React 等第三方组件适用各自许可证；不要把本地第三方二进制、账号数据或逆向提取资源提交到本仓库。

