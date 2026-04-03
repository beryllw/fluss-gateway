# Fluss Gateway API 设计

## FIP-32 已有 REST API

| 方法 | 端点 | 描述 |
|------|------|------|
| `GET` | `/health` | 健康检查 |
| `GET` | `/v1/_databases` | 列出所有数据库 |
| `GET` | `/v1/{db}/_tables` | 列出数据库中的表 |
| `GET` | `/v1/{db}/{table}/_info` | 表结构和元数据 |
| `GET` | `/v1/{db}/{table}?pk.{col}={val}` | KV 点查 |
| `GET` | `/v1/{db}/{table}/prefix?prefix={p}&limit={n}` | KV 前缀扫描 |
| `POST` | `/v1/{db}/{table}/batch` | KV 批量查找 |
| `POST` | `/v1/{db}/{table}/scan` | 日志表有界扫描 |

## 本方案新增 REST API

### 写入

| 方法 | 端点 | 描述 |
|------|------|------|
| `POST` | `/v1/{db}/{table}/rows` | Append/Upsert 写入 |

### 流式消费

| 方法 | 端点 | 描述 |
|------|------|------|
| `WS` | `/v1/{db}/{table}/subscribe` | WebSocket 实时推送 |
| `GET` | `/v1/{db}/{table}/subscribe` | SSE 实时推送 |

### 元数据管理

| 方法 | 端点 | 描述 |
|------|------|------|
| `POST` | `/v1/_databases` | 创建数据库 |
| `DELETE` | `/v1/_databases/{db}` | 删除数据库 |
| `POST` | `/v1/{db}/_tables` | 创建表 |
| `PUT` | `/v1/{db}/_tables/{table}` | 修改表 |
| `DELETE` | `/v1/{db}/_tables/{table}` | 删除表 |

### Offset 管理

| 方法 | 端点 | 描述 |
|------|------|------|
| `GET` | `/v1/{db}/{table}/offsets` | 查询表 offset |

## 请求/响应示例

### Append 写入

```json
POST /v1/{db}/{table}/rows
Content-Type: application/json

{
  "format": "json",
  "rows": [
    {"values": [1, "Alice", 100]},
    {"values": [2, "Bob", 200]}
  ]
}
```

### Upsert 写入

```json
{
  "format": "json",
  "rows": [
    {"change_type": "Insert", "values": [1, "Alice", 100]},
    {"change_type": "UpdateAfter", "values": [1, "Alice", 150]},
    {"change_type": "Delete", "values": [2, "Bob", null]}
  ]
}
```

### Arrow IPC 高性能写入

```
POST /v1/{db}/{table}/rows
Content-Type: application/vnd.apache.arrow.ipc

[Arrow IPC binary data]
```

### WebSocket 流式消费

**客户端命令**:
```json
{"action": "subscribe", "offset": 42, "format": "json"}
{"action": "unsubscribe"}
{"action": "seek", "offset": 100}
```

**服务端推送**:
```json
{
  "offset": 42,
  "timestamp": 1719999999,
  "change_type": "AppendOnly",
  "values": {"id": 1, "name": "Alice", "score": 100}
}
```

## 序列化格式

| 格式 | Content-Type | 场景 |
|------|-------------|------|
| JSON | `application/json` | 通用兼容 |
| Arrow IPC | `application/vnd.apache.arrow.ipc` | 高性能批量 |

## 错误码体系

遵循 `HTTP状态码 + 2位后缀` 模式:

| 错误码 | 含义 |
|--------|------|
| 40401 | Table not found |
| 40402 | Database not found |
| 40901 | Table already exists |
| 40902 | Database already exists |
| 42201 | Invalid request payload |
| 42202 | Schema validation failed |
| 42205 | Operation not allowed (e.g., append to PK table) |
| 42901 | Rate limit exceeded |
| 50001 | Fluss Internal error |
| 50002 | Connection error |
