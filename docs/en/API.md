# Fluss Gateway API Documentation

Base URL: `http://localhost:8080`. All requests and responses are JSON.

---

## 1. Health Check

```
GET /health
```

**Response `200 OK`:**
```json
{ "status": "ok" }
```

---

## 2. Metadata API

### 2.1 List Databases

```
GET /v1/_databases
```

**Response:** `["db1", "db2"]`

### 2.2 List Tables

```
GET /v1/{db}/_tables
```

**Response:** `["table1", "table2"]`

### 2.3 Get Table Info

```
GET /v1/{db}/{table}/_info
```

**Response:**
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

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `table_id` | integer | Internal Fluss table ID |
| `columns` | array | Column definitions (name + data type) |
| `has_primary_key` | boolean | Whether the table has a primary key |
| `num_buckets` | integer | Number of buckets |

---

## 3. Read Operations

### 3.1 Point Lookup

```
GET /v1/{db}/{table}?pk.{col}={value}
```

Look up records by primary key. Supports composite keys via multiple `pk.{col}` params.

**Response:**
```json
[{ "user_id": 1, "username": "alice", "email": "alice@example.com" }]
```

**Errors:**
- Non-PK table → `400xx`
- Missing PK column → `400xx`

### 3.2 Batch Lookup

```
POST /v1/{db}/{table}/batch
```

**Request body:**
```json
{
  "keys": [
    { "pk.user_id": "1" },
    { "pk.user_id": "2" }
  ]
}
```

Non-existent keys are silently skipped.

### 3.3 Log Table Scan

```
POST /v1/{db}/{table}/scan
```

**Request body:**
```json
{
  "timeout_ms": 5000,
  "limit": 100,
  "projection": ["col1", "col2"]
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `timeout_ms` | int | 5000 | Poll timeout in milliseconds |
| `limit` | int | unlimited | Max rows to return |
| `projection` | array | all columns | Return only specified columns |

---

## 4. Write Operations

```
POST /v1/{db}/{table}/rows
```

Automatically routes based on table type:

| Table type | Operation |
|------------|-----------|
| Log table (no PK) | append |
| PK table | upsert |
| PK table + `change_type: "Delete"` | delete |

**Request body:**
```json
{
  "rows": [
    { "values": [1, "Alice", 100] },
    { "values": [2, "Bob", 200] }
  ]
}
```

**Delete (PK table only):**
```json
{
  "rows": [
    { "values": [1, "Alice", 100], "change_type": "Delete" }
  ]
}
```

**`rows` element fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `values` | array | yes | Values in column order |
| `change_type` | string | no | Set to `"Delete"` for deletion |

**Response:** `{ "row_count": 2 }`

---

## 5. Error Format

All errors:
```json
{ "error_code": 40001, "message": "missing pk column: user_id" }
```

Error code: `<HTTP status><2-digit suffix>`.

| Error | HTTP | Description |
|-------|------|-------------|
| `400xx` | 400 | Bad request |
| `401xx` | 401 | Unauthorized (missing credentials in passthrough mode) |
| `404xx` | 404 | Resource not found |
| `500xx` | 500 | Internal error |

---

## 6. Authentication

| Mode | Description |
|------|-------------|
| `none` (default) | All requests share static startup credentials |
| `passthrough` | Each request must include HTTP Basic Auth; credentials passed to Fluss via SASL/PLAIN for ACL enforcement |

```bash
curl -u username:password http://localhost:8080/v1/_databases
```
