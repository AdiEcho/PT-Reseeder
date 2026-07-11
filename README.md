# PT-Reseeder

跨 PT 站点间的高效种子调度工具。支持基于种子特征码的自动辅种，以及跨站发布种子（转种）操作。

提供桌面客户端（Tauri）和 Web 服务端两种运行模式，支持常驻后台运行。在无 GUI 或内网其他机器连接的场景下可通过浏览器访问 Web UI。

## 功能特性

- **Dashboard 面板** — 辅种运营数据、用户信息聚合展示
- **自动辅种** — 本地文件夹扫描 + 下载器种子读取，自动匹配并辅种
- **拆包辅种** — 支持集合包内单个文件匹配辅种
- **跨站转种** — 纯 API + WebView 自动填表，覆盖不同站点特性
- **站点管理** — Cookie / Token / Passkey 管理，用户信息解析展示
- **下载器管理** — 支持 qBittorrent、Transmission
- **任务调度** — 文件夹（数据源）与任务（调度计划）分离，一对多关系
- **频率控制** — 下载频率、搜索频率、上传限速等，避免封禁
- **明暗主题** — 支持明暗主题切换

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面壳 | Tauri 2 |
| 前端 | Leptos 0.8 (SSR + Hydrate WASM) |
| 后端 | Axum 0.8 |
| 数据库 | SQLite (sqlx) |
| 样式 | SCSS |

## 快速开始

### Docker 部署（推荐）

最简方式，仅需 Docker 环境：

```bash
# 克隆项目
git clone https://github.com/your-org/PT-Reseeder.git
cd PT-Reseeder

# 一键启动
docker compose up -d

# 查看日志
docker compose logs -f
```

启动后访问 http://localhost:3000

默认数据存储在 `./data/` 目录下（SQLite 数据库 + 配置文件）。

### 桌面客户端（macOS）

```bash
# 安装构建工具
cargo install cargo-leptos
cargo install tauri-cli --version '^2'
rustup target add wasm32-unknown-unknown

# 构建桌面应用
make build-desktop
```

构建产物位于 `dist/desktop/` 目录。

## 开发指南

### 环境要求

- Rust 1.75+
- `cargo-leptos`（`cargo install cargo-leptos`）
- `wasm32-unknown-unknown` target（`rustup target add wasm32-unknown-unknown`）

### 开发模式

```bash
# 启动开发服务器（热重载）
cargo leptos watch
```

访问 http://127.0.0.1:3000

### 构建

```bash
# 构建服务端（SSR 二进制 + WASM 前端）
make build-server

# 构建桌面客户端
make build-desktop

# 构建并收集产物到 dist/
make artifacts
```

### 项目结构

```
PT-Reseeder/
├── crates/
│   ├── core/          # 核心业务逻辑（站点、辅种、任务等）
│   ├── frontend/      # Leptos 前端（SSR + Hydrate）
│   ├── server/        # Axum 服务端
│   └── desktop/       # Tauri 2 桌面壳
├── migrations/        # SQLite 数据库迁移
├── style/             # SCSS 样式
├── Dockerfile         # Docker 多阶段构建
├── docker-compose.yml # Docker Compose 部署
└── Makefile           # 构建任务
```

## Docker 部署详解

### docker-compose.yml 配置

```yaml
services:
  pt-reseeder:
    build: .
    ports:
      - "3000:3000"
    volumes:
      - ./data:/data
    environment:
      - RUST_LOG=info,pt_reseeder=debug
```

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `LEPTOS_SITE_ADDR` | `0.0.0.0:3000` | 监听地址 |
| `DATABASE_URL` | `sqlite:///data/pt-reseeder.db` | 数据库路径 |
| `PT_RESEEDER_DATA_DIR` | `/data` | 数据目录 |
| `RUST_LOG` | `info` | 日志级别 |

### 数据持久化

所有数据存储在 `/data` 卷中，包括 SQLite 数据库和应用配置。使用 bind mount（`./data:/data`）或 Docker named volume 均可。

## 截图

<!-- TODO: 添加应用截图 -->

## 许可证

参见 [LICENSE](LICENSE) 文件。
