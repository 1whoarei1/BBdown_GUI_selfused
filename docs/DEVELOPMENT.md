# 开发说明

## 环境

- Windows 10/11
- Node.js 22 与 npm
- Rust stable（包含 `rustfmt`）
- Microsoft Edge WebView2 Runtime
- 本地 BBDown 与 FFmpeg，用于真实解析和下载冒烟测试

## 安装与检查

```powershell
npm ci
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml
```

当前基线为前端生产构建通过、33 个 Rust 测试通过。默认测试使用固定 fixture，不依赖 B 站网络或真实账号。

## 运行

只运行浏览器前端预览：

```powershell
npm run dev
```

运行真实桌面应用：

```powershell
npm run tauri dev
```

首次启动后，在设置页配置 `BBDown.exe` 和 `ffmpeg.exe`。应用配置写入：

```text
%APPDATA%\com.bbdown.next\config.json
```

不要把配置文件、`BBDown.data`、`BBDownTV.data`、二维码、Cookie、token 或带签名的媒体 URL 加入 fixture、日志或提交。

## 打包

```powershell
npm run tauri build
```

安装包位于 `src-tauri/target/release/bundle/`。当前仓库不捆绑 BBDown 或 FFmpeg；如未来改变分发方式，必须先补齐对应许可证和第三方声明。

## 在线冒烟

在线冒烟不进入默认 CI。人工验证至少覆盖：

1. 启动后工具检测不弹控制台窗口。
2. 扫码或 Cookie 登录后账号状态正确。
3. 普通单 P、多 P、番剧 EP 和 SS 解析。
4. 封面、作者、简介、分 P 和流信息展示。
5. 创建、停止和完成下载任务。
6. 打开下载目录与退出登录。

冒烟测试不要提交真实 Cookie、二维码、媒体签名 URL 或下载内容。

