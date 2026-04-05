# Fluss Gateway 部署文档

## 目录结构

```
deploy/
├── docker/
│   ├── Dockerfile                  # 最小化运行镜像
│   ├── docker-compose.dev.yml      # 开发环境（含 Fluss 集群）
│   └── docker-compose.prod.yml     # 生产环境（仅 Gateway）
├── systemd/
│   └── fluss-gateway.service       # systemd 单元文件模板
└── config/
    └── gateway.toml.example        # 配置文件示例
```

---

## 1. 开发环境

```bash
# 启动 Fluss 集群
docker compose -f deploy/docker/docker-compose.dev.yml up -d

# 编译并运行 Gateway
cargo build --release
./target/release/fluss-gateway serve --fluss-coordinator=localhost:9123

# 或使用运维脚本（后台运行）
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123
```

端口：`9123`（Coordinator）、`9124`（Tablet Server）、`2181`（ZooKeeper）。

---

## 2. Docker 生产部署

```bash
cargo build --release
docker build -f deploy/docker/Dockerfile . -t fluss-gateway:latest
FLUSS_COORDINATOR=fluss-prod:9123 docker compose -f deploy/docker/docker-compose.prod.yml up -d
```

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `FLUSS_COORDINATOR` | （必填） | Fluss Coordinator 地址 |
| `GATEWAY_PORT` | `8080` | HTTP 监听端口 |
| `LOG_LEVEL` | `info` | 日志级别 |

---

## 3. 物理机部署

```bash
sudo cp target/release/fluss-gateway /usr/local/bin/
sudo mkdir -p /etc/fluss-gateway
sudo cp deploy/config/gateway.toml.example /etc/fluss-gateway/gateway.toml
sudo cp deploy/systemd/fluss-gateway.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now fluss-gateway
```

---

## 4. 配置

优先级：CLI 参数 > 配置文件 > 默认值。

```toml
[server]
host = "0.0.0.0"
port = 8080

[fluss]
coordinator = "localhost:9123"

[auth]
# "none" - 使用启动时静态凭据
# "passthrough" - 从 HTTP Basic Auth 提取每请求凭据
type = "none"
startup_username = ""
startup_password = ""

[pool]
max_connections = 500
idle_timeout_secs = 600

[log]
level = "info"
```

| 模式 | 适用场景 | 说明 |
|------|----------|------|
| `none` | 单租户、内网隔离 | 所有请求共享启动凭据 |
| `passthrough` | 多租户、需要 ACL | 每个 HTTP 请求携带自己的凭据 |

---

## 5. 运维管理

```bash
./bin/fluss-gateway.sh start -- --fluss-coordinator=localhost:9123  # 后台启动
./bin/fluss-gateway.sh status                                         # 查看状态
./bin/fluss-gateway.sh stop                                           # 优雅停止
./bin/fluss-gateway.sh restart -- --fluss-coordinator=localhost:9123  # 重启
```

PID 文件默认 `/tmp/fluss-gateway.pid`，可通过 `--pid-file=PATH` 自定义。

### 优雅关闭

收到 SIGTERM/SIGINT 信号后：
1. 停止接收新请求
2. 等待现有请求完成（最多 10 秒）
3. 关闭连接池，清除缓存的 Fluss 连接
4. 退出进程

---

## 6. 故障排查

```bash
# 检查 Coordinator 是否可达
nc -zv fluss-prod 9123

# 检查端口占用
lsof -i :8080

# 查看错误日志
cat /tmp/fluss-gateway.err

# systemd 日志
journalctl -u fluss-gateway -f
```
