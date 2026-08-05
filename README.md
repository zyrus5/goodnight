# Goodnight

一个可直接开始开发的前后端分离 Web 工程脚手架。开发时前后端独立运行，发布时前端资源会嵌入 Rust 二进制文件。

## 技术栈

- 前端：React、TypeScript、Vite、React Router、Zustand、Axios
- 后端：Rust、Axum、Tokio、SQLx
- 数据库：PostgreSQL

## 目录

```text
.
├── frontend/          # React 前端
├── build.rs           # 构建前端并交给 Rust 嵌入
├── src/               # Axum 后端
│   ├── routes/        # API 路由
│   ├── app.rs         # Router 与中间件
│   ├── config.rs      # 环境配置
│   └── db.rs          # PostgreSQL 连接池
└── docker-compose.yml # 本地 PostgreSQL
```

## 本地启动

要求：Rust、Node.js 20+、Docker。

```bash
# 1. 启动 PostgreSQL
docker compose up -d postgres

# 2. 启动后端（默认 http://127.0.0.1:3000）
cp .env.example .env
cargo run

# 3. 新开终端，启动前端（默认 http://localhost:5173）
cd frontend
cp .env.example .env
npm install
npm run dev
```

前端开发服务器会把 `/api` 代理到 Axum。连通性接口：

```bash
curl http://127.0.0.1:3000/api/health
```

## 打包与部署

```bash
cargo build --release
```

该命令会自动执行以下步骤：

1. 若 `frontend/node_modules` 不存在，使用锁文件执行 `npm ci`。
2. 执行前端生产构建。
3. 将 `frontend/dist` 的全部资源嵌入 Rust 二进制文件。

最终只需部署一个文件：

```text
target/release/goodnight
```

运行后，`http://127.0.0.1:3000` 提供前端页面，`/api/*` 提供后端接口。PostgreSQL 等外部服务和环境变量仍需在部署环境中配置。

## 验证

```bash
cargo test
cargo build --release
```
