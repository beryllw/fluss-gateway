# Fluss Gateway API 文档

Base URL：`http://localhost:8080`。所有请求和响应均使用 JSON。

---

## 1. 健康检查

```
GET /health
```

**响应 `200 OK`：**
```json
{ "status": "ok" }
```

---

## 2. 元数据 API

### 2.1 列出所有数据库

```
GET /v1/_databases
```

**响应：** `["db1", "db2"]`

### 2.2 列出表

```
GET /v1/{db}/_tables
```

**响应：** `["table1", "table2"]`

### 2.3 获取表信息

```
GET /v1/{db}/{table}/_info
```

**响应：**
```json
{
  "table_id": 1,
  "database": "mydb",
  "table": "users",
  "columns": [
    { "name": "user_id", "data_type": "Int(32)" },
    { "name": "username", "data_type": "String(Default)" }
  ],
  "has_primary_key": true,
  "num_buckets": 3
}
```

**字段说明：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `table_id` | integer | Fluss 内部表 ID |
| `columns` | array | 列定义（列名 + 数据类型） |
| `has_primary_key` | boolean | 是否为主键表 |
| `num_buckets` | integer | 分桶数 |

---

## 3. 读操作

### 3.1 主键点查

```
GET /v1/{db}/{table}?pk.{col}={value}
```

根据主键查询。支持复合主键（多个 `pk.{col}` 参数）。

**响应：**
```json
[{ "user_id": 1, "username": "alice", "email": "alice@example.com" }]
```

**错误：**
- 非主键表 → `400xx`
- 缺少主键列 → `400xx`

### 3.2 批量主键查询

```
POST /v1/{db}/{table}/batch
```

**请求体：**
```json
{
  "keys": [
    { "pk.user_id": "1" },
    { "pk.user_id": "2" }
  ]
}
```

不存在的键会被跳过。

### 3.3 日志表扫描

```
POST /v1/{db}/{table}/scan
```

**请求体：**
```json
{
  "timeout_ms": 5000,
  "limit": 100,
  "projection": ["col1", "col2"]
}
```

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `timeout_ms` | int | 5000 | 轮询超时（毫秒） |
| `limit` | int | 无 | 最大返回行数 |
| `projection` | array | 所有列 | 只返回指定列 |

---

## 4. 写操作

```
POST /v1/{db}/{table}/rows
```

根据表类型自动路由：

| 表类型 | 操作 |
|--------|------|
| 日志表（无主键） | append |
| 主键表 | upsert |
| 主键表 + `change_type: "Delete"` | delete |

**请求体：**
```json
{
  "rows": [
    { "values": [1, "Alice", 100] },
    { "values": [2, "Bob", 200] }
  ]
}
```

**删除（仅主键表）：**
```json
{
  "rows": [
    { "values": [1, "Alice", 100], "change_type": "Delete" }
  ]
}
```

**`rows` 元素字段：**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `values` | array | 是 | 按表列顺序排列的值数组 |
| `change_type` | string | 否 | 设为 `"Delete"` 时执行删除 |

**响应：** `{ "row_count": 2 }`

---

## 5. 错误格式

所有错误统一格式：
```json
{ "error_code": 40001, "message": "missing pk column: user_id" }
```

错误码格式：`<HTTP 状态码><2 位后缀>`。

| 错误码 | HTTP 状态码 | 说明 |
|--------|-------------|------|
| `400xx` | 400 | 请求参数错误 |
| `401xx` | 401 | 未认证（passthrough 模式缺少凭据） |
| `404xx` | 404 | 资源不存在 |
| `500xx` | 500 | 内部错误 |

---

## 6. 认证

| 模式 | 说明 |
|------|------|
| `none`（默认） | 所有请求共享启动时静态凭据 |
| `passthrough` | 每请求携带 HTTP Basic Auth，凭据通过 SASL/PLAIN 传给 Fluss 执行 ACL 鉴权 |

```bash
curl -u username:password http://localhost:8080/v1/_databases
```
