# Goodnight 1.0.1 部署说明

## 文件放置与启动

发布包中的可执行文件已经包含前端页面，不需要另外部署 `frontend` 目录。复制 `.env.example` 为 `.env`，修改配置后将它与 `goodnight`（macOS）或 `goodnight.exe`（Windows）放在同一目录即可。

macOS：

```bash
chmod +x goodnight
./goodnight
```

Windows PowerShell：

```powershell
Copy-Item .env.example .env
.\goodnight.exe
```

应用会优先读取进程已有的系统环境变量，然后读取当前工作目录（及其父目录）的 `.env`；如果仍未找到，再读取可执行文件同目录的 `.env`。

## 必填配置

- `APP_ENCRYPTION_KEYS`：始终必填。必须使用生产随机密钥，不能使用示例值。macOS/Linux 可运行 `openssl rand -base64 32` 生成密钥，然后配置为 `APP_ENCRYPTION_KEYS=v1:<生成结果>`。
- `BOOTSTRAP_ADMIN_USERNAME`、`BOOTSTRAP_ADMIN_PASSWORD`：仅首次连接空数据库、系统尚无用户时必填。密码至少 10 位；管理员创建后可不再配置。

以下配置有程序默认值，因此并非“缺少就无法启动”，但生产环境通常需要明确配置：

- `DATABASE_URL`：默认是本机 `postgres://postgres:postgres@localhost:5432/goodnight`。实际数据库不是这个地址时必须配置。
- `JENKINS_USERNAME`、`JENKINS_PASSWORD`：默认空字符串。Jenkins 要求认证时必须配置，否则相关连接与任务操作会失败。
- `APP_HOST`：默认 `127.0.0.1`，仅本机可访问；需要让其他机器访问时配置为 `0.0.0.0`，并通过防火墙或反向代理控制访问。
- `SESSION_SECURE`：默认 `false`；站点通过 HTTPS 提供服务时建议设为 `true`。

其余参数均有默认值或属于可选功能，可以不配置：

- `APP_PORT=3000`
- `FRONTEND_ORIGIN=http://localhost:5173`
- `RUST_LOG=goodnight=debug,tower_http=info`
- `BOOTSTRAP_ADMIN_DISPLAY_NAME=系统管理员`
- `SESSION_HOURS=24`
- `SCHEDULER_INTERVAL_SECONDS=5`
- `WORKER_INTERVAL_SECONDS=2`
- `GLOBAL_JOB_CONCURRENCY=16`
- `PER_JENKINS_CONCURRENCY=4`
- `METRICS_TOKEN`：不配置时指标接口不可用。

## 外部依赖

运行机器需要能够访问 PostgreSQL 和已配置的 Jenkins。数据库表结构会在应用启动时自动初始化。服务启动后访问 `http://<APP_HOST>:<APP_PORT>`，健康检查地址是 `/api/health`。

本次发布的 macOS 与 Windows 可执行文件未做商业证书签名。首次运行时，macOS Gatekeeper 或 Windows SmartScreen 可能要求人工确认。
