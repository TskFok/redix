# 依赖安全审计（2026-10-28）

本次检查 npm 全量依赖（含开发依赖）、Cargo.lock 中的跨平台依赖，以及 Tauri 和官方插件的安全公告。在现有 `main` 分支完成修复。

## 修复内容

| 依赖 | 原版本 | 修复版本 | 处理原因 |
| --- | --- | --- | --- |
| undici | 7.30.0 | 7.30.1 | npm 检出 1 个受影响包，关联 8 条公告（4 high、4 moderate），涉及 TLS 校验绕过、缓存污染及信息泄露、拒绝服务和完整性校验绕过 |
| tokio | 1.53.1 | 1.53.3 | RUSTSEC-2026-0386：`AsyncFd` 构造器违反 I/O 安全约束，可能导致 use-after-free；RustSec 将其分类为 unsound informational 告警 |

`undici` 由开发依赖 `jsdom@29.1.1` 引入，`7.30.1` 满足其 `^7.25.0` 约束，仅更新 `package-lock.json`。Tokio 是直接依赖，同时更新 `Cargo.toml` 的最低版本和 `Cargo.lock`。无主版本升级、无新增 override 或审计忽略项。

Tokio 官方补丁弃用旧 `AsyncFd` 构造器并引入要求调用者保证文件描述符有效性的 unsafe 构造器。项目源码没有直接使用这些 API；本次确认补丁版本和回归结果，不将审计消警等同于所有传递调用路径的安全证明。

## 公告核查

- [undici 7.30.1 官方发布说明](https://github.com/nodejs/undici/releases/tag/v7.30.1)覆盖全部 8 条公告：`GHSA-4qv8-2p9j-2685`、`GHSA-f72v-6w45-q472`、`GHSA-xjj6-f33q-929c`、`GHSA-4w7w-5xrc-9h26`、`GHSA-2m5v-pch6-74hx`、`GHSA-qrxm-fqhp-jwvm`、`GHSA-9fm3-97px-rjf6`、`GHSA-55wc-3pg8-jfw6`。
- [Tokio RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html)：当前版本线修复于 `1.53.3`。
- 检查 [Tauri 官方公告列表](https://github.com/tauri-apps/tauri/security/advisories)和[官方插件公告列表](https://github.com/tauri-apps/plugins-workspace/security/advisories)。现有 `tauri 2.11.6` 已包含 [IPC channel 访问控制修复](https://github.com/tauri-apps/tauri/security/advisories/GHSA-w28w-mhc8-qvjv)；传递依赖 `tauri-plugin-fs 2.5.2` 已包含[文件系统作用域修复](https://github.com/tauri-apps/plugins-workspace/security/advisories/GHSA-9g54-6x48-9vpw)，无需额外升级。

## 审计结果与剩余告警

| 项目 | 修复前 | 修复后 |
| --- | --- | --- |
| npm 受影响包 | 1（最高 high） | 0 |
| Cargo 正式漏洞计数 | 0 | 0 |
| Cargo unsound 告警 | 2 | 1 |
| Cargo 停维护告警 | 9 | 9 |

RustSec 数据库包含 1,380 条公告，版本为 `99670cfc0bc574433034b75876a4b791ccaa2efb`（2026-10-27）。

剩余告警未被忽略，也未标为已修复：

- `glib 0.18.5`：[RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) 涉及迭代器未定义行为。Linux 的 `redix → tauri → gtk 0.18 → glib 0.18` 依赖链限制了版本；修复版为 `glib >=0.20`，无法通过兼容补丁升级直接替换，需要上游迁移或另行评估维护补丁。
- `proc-macro-error`、5 个 `unic-*`、`zbus_names`、`zvariant` 和 `zvariant_derive` 仍有停维护告警。这些需要上游依赖迁移；例如 `unic-*` 可通过更大范围的 Tauri 配套组件及 `urlpattern` 升级处理，不属于本次定向安全补丁。

因此，Cargo 正式漏洞计数为 0 不代表整个依赖树没有安全或维护风险。

## 验证

- `npm ci` 成功；修复后 `npm audit --json` 全量结果为 0。
- `npm test -- --maxWorkers=2`：73 个文件、850 项测试通过。
- `npm run build` 通过。
- `npm run tauri -- build --debug --no-bundle --ci -- --locked` 通过，生成 macOS 调试二进制；未打包、签名或发布。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml`：523 通过、0 失败、49 ignored；这些 ignored 测试需要显式 Redis/SSH 等环境，本次未额外运行。
- Rust 测试首次受沙箱本地 TCP 监听限制影响；取得对应权限后完整重跑成功。
- `git diff --check` 通过；独立复审未发现阻断问题。

验证环境为 macOS、Node 25.5.0、npm 11.17.0、Rust 1.93.1、cargo-audit 0.22.1，未执行 Windows/Linux 原生构建与运行验证。保留既有 jsdom navigation 提示、Vite 主包体积提示、Rust unused 警告及 npm 安装脚本待批准提示，未改变安装脚本安全策略。

后续复查命令：

```sh
npm ci
npm audit
cargo audit --file src-tauri/Cargo.lock
npm test -- --maxWorkers=2
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
```
