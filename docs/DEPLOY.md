# Fluss Gateway — 部署指南

> 在 Linux 机器上部署 Fluss Gateway 的完整指南。

## 前提条件

- Fluss 集群已运行（Coordinator + Tablet Server）
- Linux x86_64 或 aarch64
- 端口 8080 可访问（Gateway 默认端口）

---

## 方式一：Docker Compose（推荐，一键启动完整环境）

适合快速测试或没有其他 Fluss 集群的场景。Fluss 集群 + Gateway 一起启动。

```bash
# 1. 克隆仓库
git clone <repo-url> fluss-gateway
cd fluss-gateway

# 2. 构建镜像（首次需要，约 5-10 分钟）
podman build -t localhost/fluss-gateway:latest -f deploy/docker/Dockerfile.standalone .
# 或 docker build
# docker build -t localhost/fluss-gateway:latest -f deploy/docker/Dockerfile.standalone .

# 3. 启动完整环境（ZooKeeper + Fluss + Gateway）
podman compose -f deploy/docker/docker-compose.standalone.yml --project-name fluss-gateway up -d

# 4. 等待就绪
sleep 30
curl http://localhost:8080/health
# 应返回: {"status":"ok"}
```

**清理**：
```bash
podman compose -f deploy/docker/docker-compose.standalone.yml --project-name fluss-gateway down
```

---

## 方式二：Docker Compose（仅 Gateway，对接已有 Fluss）

适合已有 Fluss 集群，只需添加 Gateway。

创建 `docker-compose.yml`：

```yaml
version: "3"
services:
  gateway:
    image: localhost/fluss-gateway:latest
    command:
      - serve
      - --host=0.0.0.0
      - --port=8080
      - --fluss-coordinator=<YOUR_FLUSS_COORDINATOR>:9123
      - --auth-type=none
    ports:
      - "8080:8080"
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 5s
      timeout: 3s
      retries: 10
      start_period: 10s
```

```bash
docker compose up -d
curl http://localhost:8080/health
```

---

## 方式三：一键安装脚本（推荐）

自动完成：构建二进制 → 创建系统用户 → 安装配置 → systemd 服务 → 健康检查。

```bash
# 从源码目录运行
sudo bash deploy/install.sh

# 自定义参数
sudo bash deploy/install.sh \
  --coordinator=coordinator-server:9123 \
  --port=8080 \
  --auth-type=none \
  --log-level=info
```

参数说明：

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--coordinator=ADDR` | `localhost:9123` | Fluss Coordinator 地址 |
| `--port=N` | `8080` | HTTP 端口 |
| `--auth-type=TYPE` | `none` | `none` 或 `passthrough` |
| `--log-level=LEVEL` | `info` | `debug`/`info`/`warn`/`error` |

脚本自动检测：
- 如果已有编译好的二进制（`target/release/fluss-gateway`），直接使用
- 否则运行 `cargo build --release`（需要安装 Rust 工具链）
- 创建 `fluss` 系统用户、配置目录、日志目录
- 安装 systemd 服务并开机自启

**卸载**：
```bash
sudo systemctl stop fluss-gateway
sudo systemctl disable fluss-gateway
sudo rm /etc/systemd/system/fluss-gateway.service
sudo rm -rf /etc/fluss-gateway
sudo rm /usr/local/bin/fluss-gateway
sudo userdel fluss
sudo systemctl daemon-reload
```

---

## 方式四：systemd 手动部署

无需 Docker，适合生产环境。

### 1. 获取二进制

从源码编译：
```bash
git clone <repo-url> fluss-gateway
cd fluss-gateway
cargo build --release
sudo cp target/release/fluss-gateway /usr/local/bin/
```

或从 GitHub Releases 下载（CI 构建后）。

### 2. 安装服务

```bash
# 创建用户和配置目录
sudo useradd --system --no-create-home fluss
sudo mkdir -p /etc/fluss-gateway
sudo mkdir -p /var/log/fluss-gateway
sudo chown fluss:fluss /var/log/fluss-gateway

# 安装配置文件
sudo cp deploy/config/gateway.toml.example /etc/fluss-gateway/gateway.toml
sudo vi /etc/fluss-gateway/gateway.toml
# 至少修改 fluss.coordinator 为你的 Fluss 地址

# 安装 systemd 服务
sudo cp deploy/systemd/fluss-gateway.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable fluss-gateway
sudo systemctl start fluss-gateway

# 检查状态
sudo systemctl status fluss-gateway

# 查看日志
sudo journalctl -u fluss-gateway -f
```

### 3. 配置文件说明

`/etc/fluss-gateway/gateway.toml`：

```toml
[server]
host = "0.0.0.0"
port = 8080

[fluss]
coordinator = "localhost:9123"   # 改为你的 Fluss Coordinator 地址

[auth]
type = "none"  # "none" | "passthrough"

[pool]
max_connections = 500
idle_timeout_secs = 600

[log]
level = "info"  # "debug" | "info" | "warn" | "error"
```

---

## 验证部署

```bash
# 健康检查
curl http://localhost:8080/health

# 查询数据库
curl http://localhost:8080/v1/_databases

# 创建数据库
curl -X POST http://localhost:8080/v1/_databases \
  -H 'Content-Type: application/json' \
  -d '{"database_name":"my_db","ignore_if_exists":true}'

# 创建日志表
curl -X POST http://localhost:8080/v1/my_db/_tables \
  -H 'Content-Type: application/json' \
  -d '{
    "table_name":"my_log",
    "schema":[
      {"name":"id","data_type":"int"},
      {"name":"name","data_type":"string"},
      {"name":"value","data_type":"bigint"}
    ],
    "ignore_if_exists":true
  }'

# 写入数据
curl -X POST http://localhost:8080/v1/my_db/my_log/rows \
  -H 'Content-Type: application/json' \
  -d '{"rows":[{"values":[1,"Alice",100]},{"values":[2,"Bob",200]}]}'

# 扫描数据
curl -X POST http://localhost:8080/v1/my_db/my_log/scan \
  -H 'Content-Type: application/json' \
  -d '{"timeout_ms":5000}'
```

---

## 常见操作

### systemd

```bash
sudo systemctl restart fluss-gateway   # 重启
sudo systemctl stop fluss-gateway      # 停止
sudo systemctl disable fluss-gateway   # 禁用开机自启
sudo journalctl -u fluss-gateway -n 50 # 查看最近 50 行日志
```

### Docker

```bash
docker compose up -d --force-recreate  # 重建
docker compose logs -f gateway         # 查看日志
docker compose down                    # 停止并清理
```

### 调试模式

systemd 方式：编辑 `/etc/fluss-gateway/gateway.toml`，将 `log.level` 改为 `"debug"`，然后 `sudo systemctl restart fluss-gateway`。

Docker 方式：添加环境变量或挂载 debug 配置文件。

---

## 故障排查

| 问题 | 原因 | 解决 |
|------|------|------|
| `Connection refused` on 9123 | Fluss Coordinator 未运行或未暴露 | 检查 Fluss 集群状态 |
| `Exec format error` | 二进制架构不匹配（macOS 编译 vs Linux 运行） | 在 Linux 上重新编译或使用 Dockerfile.standalone |
| Gateway 启动后立即退出 | 无法连接 Fluss Coordinator | 检查 `coordinator` 配置地址是否正确 |
| 健康检查返回 ok 但 API 失败 | Fluss `advertised.listeners` 配置问题 | 确保容器内能解析 coordinator 地址 |
