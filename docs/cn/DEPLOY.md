# Fluss Gateway 部署文档

## 目录结构

```
deploy/
├── docker/
│   ├── Dockerfile.standalone      # 自构建镜像（含 Fluss 集群）
│   ├── Dockerfile.release          # Release 发布镜像
│   ├── docker-compose.standalone.yml # 开发环境（含 Fluss 集群）
│   └── docker-compose.prod.yml     # 生产环境（仅 Gateway）
├── systemd/
│   └── fluss-gateway.service       # systemd 单元文件模板
├── config/
│   └── gateway.toml.example        # 配置文件示例
└── install.sh                      # 一键安装脚本
```

---

## 方式一：Docker Compose（一键启动完整环境）

适合快速测试或没有其他 Fluss 集群的场景。Fluss 集群 + Gateway 一起启动。

```bash
# 1. 克隆仓库
git clone <repo-url> fluss-gateway
cd fluss-gateway

# 2. 构建镜像（首次需要，约 5-10 分钟）
docker build -t localhost/fluss-gateway:latest -f deploy/docker/Dockerfile.standalone .

# 3. 启动完整环境（ZooKeeper + Fluss + Gateway）
docker compose -f deploy/docker/docker-compose.standalone.yml --project-name fluss-gateway up -d

# 4. 等待就绪
sleep 30
curl http://localhost:8080/health
# 应返回: {"status":"ok"}
```

**清理**：
```bash
docker compose -f deploy/docker/docker-compose.standalone.yml --project-name fluss-gateway down
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

> 如果不使用 `command` 覆盖，默认启动参数为 `fluss-gateway serve --host 0.0.0.0 --port 8080`，
> 仍需通过环境变量或配置文件指定 `fluss.coordinator`。

---

## 方式三：GitHub Release 下载安装（推荐，无需编译）

从 GitHub Releases 页面下载预编译的二进制文件，无需本地编译。

### 1. 选择对应平台的 Release 包

访问 [GitHub Releases](https://github.com/<owner>/fluss-gateway/releases)，选择对应架构的 tarball：

| 文件名 | 平台 | 架构 |
|--------|------|------|
| `fluss-gateway-x86_64-linux.tar.gz` | Linux | x86_64 (amd64) |
| `fluss-gateway-aarch64-linux.tar.gz` | Linux | ARM64 |
| `fluss-gateway-aarch64-macos.tar.gz` | macOS | Apple Silicon (M1/M2/M3) |

每个 tarball 包含：
- `fluss-gateway` — 预编译二进制
- `gateway.toml.example` — 配置文件示例
- `install.sh` — 一键安装脚本（仅 Linux）

### 2. 下载并安装

**自动安装**（推荐，Linux 仅）：

```bash
# 下载对应架构的 tarball
tar xzf fluss-gateway-x86_64-linux.tar.gz
cd fluss-gateway-x86_64-linux

# 运行安装脚本（需要 sudo）
sudo bash install.sh \
  --coordinator=coordinator-server:9123 \
  --port=8080 \
  --auth-type=none
```

`install.sh` 会自动完成：
- 安装二进制到 `/usr/local/bin/fluss-gateway`
- 创建 `fluss` 系统用户
- 生成配置文件到 `/etc/fluss-gateway/gateway.toml`
- 安装并启动 systemd 服务
- 执行健康检查确认服务就绪

**手动安装**（macOS 或不想用脚本的场景）：

```bash
# 解压
tar xzf fluss-gateway-aarch64-macos.tar.gz
cd fluss-gateway-aarch64-macos

# 安装二进制
sudo cp fluss-gateway /usr/local/bin/
sudo chmod 755 /usr/local/bin/fluss-gateway

# 创建配置目录
sudo mkdir -p /etc/fluss-gateway
sudo cp gateway.toml.example /etc/fluss-gateway/gateway.toml

# 编辑配置（至少修改 fluss.coordinator）
sudo vi /etc/fluss-gateway/gateway.toml

# 直接运行
fluss-gateway serve --config=/etc/fluss-gateway/gateway.toml
```

### 3. 命令行一键安装

也可以直接通过 `curl` 下载并安装（Linux x86_64）：

```bash
# 设置变量（替换为实际版本号）
VERSION="v0.1.0"
ARCH="x86_64"  # 或 "aarch64"

# 下载并安装
curl -fsSL "https://github.com/<owner>/fluss-gateway/releases/download/${VERSION}/fluss-gateway-${ARCH}-linux.tar.gz" \
  | tar xz
cd "fluss-gateway-${ARCH}-linux"
sudo bash install.sh --coordinator=localhost:9123
```

---

## 方式四：从源码编译 + systemd 部署

无需 Docker，适合生产环境或二次开发。

### 1. 编译二进制

```bash
git clone <repo-url> fluss-gateway
cd fluss-gateway
cargo build --release
sudo cp target/release/fluss-gateway /usr/local/bin/
sudo chmod 755 /usr/local/bin/fluss-gateway
```

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

### 4. CLI 参数

优先级：CLI 参数 > 配置文件 > 默认值。

```bash
fluss-gateway serve [OPTIONS]

选项：
      --host <HOST>                        绑定地址
      --port <PORT>                        监听端口
      --fluss-coordinator <ADDR>           Fluss Coordinator 地址
      --auth-type <TYPE>                   认证类型：none | passthrough
      --sasl-username <USER>               SASL 用户名（none 模式回退）
      --sasl-password <PASS>               SASL 密码（none 模式回退）
      --config <PATH>                      配置文件路径
      --pool-max-connections <N>           连接池最大连接数
      --pool-idle-timeout-secs <N>         连接池空闲超时（秒）
      --log-level <LEVEL>                  日志级别：debug | info | warn | error
  -h, --help                               打印帮助信息
```

---

## 方式五：运维管理

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
| `unknown subcommand 'server'` | 命令拼写错误 | 使用 `fluss-gateway serve`（不是 `server`） |
