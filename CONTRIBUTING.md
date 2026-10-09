# 贡献指南

## 开发流程

1. 从 `main` 创建主题分支。
2. 保持改动聚焦，不提交无关格式化或生成文件。
3. 为后端行为改动补充 Rust 单元测试或 fixture。
4. 提交前运行完整检查。

```powershell
npm ci
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

## 安全与版权

- 不提交 Cookie、token、二维码、账号配置或媒体签名 URL。
- 不提交 BBDown、FFmpeg、JiJiDown 等第三方二进制或提取资源。
- fixture 必须固定、脱敏，并说明来源场景。
- 引入或移植开源代码需保留许可和作者声明；协议参考见 `THIRD_PARTY_NOTICES.md`。图标、图片、音效或其他资源不得无授权复制。

Pull Request 应说明行为变化、验证命令和仍存在的限制。

