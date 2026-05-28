# Proxy Term

一个基于 Rust + [Slint](https://slint.dev) 的桌面代理管理工具，提供简单的 GUI 界面来配置和管理系统代理环境变量。

## 功能

- **代理类型**：支持 HTTP 和 SOCKS5 代理
- **一键启动/停止**：通过 `setx` (Windows) 或 shell rc 文件 (Unix) 设置/清除代理环境变量
- **连接测试**：验证代理是否可用
- **配置持久化**：自动保存配置到 `%APPDATA%/proxy_term/config.json` (Windows) 或 `~/.config/proxy_term/config.json` (Unix)
- **绕过列表**：支持配置直连地址列表（no_proxy）

## 使用

启动后填写代理地址、端口等信息，点击 **启动代理** 即可生效（需要打开新终端）。点击 **停止代理** 清除所有代理环境变量。

## 构建

```bash
cargo build --release
```
