# Goodnight · Jenkins Job 统一编排平台

Goodnight 用于统一维护多个客户和环境中的 Jenkins Pipeline Job，并通过 DAG 完成串行、并行和混合编排。平台包含本地用户与组件权限、连接发现、Job 参数版本、立即/指定时间/Cron 调度、执行恢复、节点日志、停止和审计。

## 本地启动

要求 Rust、Node.js 20+、Docker。首次启动前必须配置管理员、Jenkins 公共账户和应用加密密钥：

```bash
cp .env.example .env
cp frontend/.env.example frontend/.env
docker compose up -d postgres
cargo run
```

另开终端运行前端开发服务器：

```bash
cd frontend
npm ci
npm run dev
```

访问 `http://localhost:5173`，使用 `BOOTSTRAP_ADMIN_USERNAME` 和 `BOOTSTRAP_ADMIN_PASSWORD` 登录。引导管理员只会在用户表为空时创建。

## Jenkins 与安全配置

- 所有 Jenkins 使用 `JENKINS_USERNAME` / `JENKINS_PASSWORD` 公共账户。
- 环境中配置的 Jenkins 地址只需使用 `http` 或 `https`；保存时平台会请求 `<Jenkins 地址>/api/json` 验证连通性和公共账户权限。
- 生产环境 Jenkins 强制 HTTPS，Jenkins 证书必须通过正常的 TLS 校验。
- `APP_ENCRYPTION_KEYS` 格式为 `版本:Base64密钥`，逗号分隔；第一项是写入密钥。生成新密钥可运行 `openssl rand -base64 32`，轮换时把新版本放在最前面并保留旧版本供解密。
- 生产部署应将 `SESSION_SECURE=true`，并通过 Secret 管理器注入密码和密钥。

## 调度与恢复

调度器使用 PostgreSQL advisory transaction lock 和唯一触发键防止多实例重复触发；节点通过带租约的数据库领取机制执行。服务重启后会继续查询已经保存 queue/build 标识的节点，不会自动重新提交状态不明的 Jenkins Job。

五段 Cron 必须选择 IANA 时区。指定时间漏执行会补一次，周期任务不补历史次数且禁止重叠。

## 构建与验证

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cd frontend && npm run typecheck && npm run build
cargo build --release
```

生产构建会把 `frontend/dist` 嵌入 Rust 二进制。健康检查位于 `/api/health`；Prometheus 文本指标位于 `/api/metrics`，请求需携带 `Authorization: Bearer <METRICS_TOKEN>`。

数据库结构由 `migrations/` 中的 SQLx migration 管理，应用启动时自动执行。历史任务版本、执行快照和审计日志不会被业务 API 删除。
