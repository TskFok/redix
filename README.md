# Redix

Redix 是一个面向本地 Redis Standalone 的桌面客户端 MVP，使用 Rust + Tauri + React 构建。当前版本优先提供连接管理、键浏览、基础数据编辑和 Workbench 命令执行，适合开发环境中的单机 Redis 实例。

## 前置条件

- Node.js 20+ 和 npm。
- Rust stable、Cargo，以及 Tauri 所需的系统依赖。
- 一个可访问的本地 Redis Standalone 实例，默认可使用 `redis://127.0.0.1:6379`。
- 若 Redis 开启认证，在连接表单中填写密码；密码只交给本地后端并由系统钥匙串保存，连接列表不回显密码。

## 安装与开发

```bash
npm install
npm run dev
```

`npm run dev` 启动前端开发服务器。需要运行桌面壳时使用：

```bash
npm run tauri:dev
```

## 测试、检查与构建

```bash
npm run check:non-cloud
npm run test:frontend
npm run test:rust
npm run build
npm run tauri:build
```

`check:non-cloud` 只扫描产品源码目录和 `package.json`，不扫描 README、设计文档或范围说明。它用于阻止非本地产品入口意外进入代码和菜单文案。

Browser 使用 Redis `SCAN` 分页浏览键，不使用阻塞式全量键枚举。Workbench 只在当前本地连接上执行用户输入的 Redis 命令。

## 当前边界

本版本明确只支持本地 Redis Standalone。当前不支持 Redis Cloud、Azure Managed Redis、Cluster、Sentinel、TLS、SSH 或 Redis 模块功能，也不包含云登录、云账户、云端点、云数据库发现和云 SDK 集成。

Profiler、Slow Log、Pub/Sub、模块专用数据编辑器和远程托管实例管理同样不在本 MVP 范围内。
