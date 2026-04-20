# Fluss Gateway - Agent Constraints

> This document defines constraint rules that the agent **must load** before every task execution.

## Project Information

- **Project Name**: Fluss Gateway
- **Tech Stack**: Rust + fluss-rust + DataFusion + Axum
- **Positioning**: Fluss REST API gateway extending FIP-32, adding write capabilities, streaming consumption, and governance
- **Project Root**: `/Users/boyu/VscodeProjects/fluss-gateway`
- **Documentation**: `docs/en/` (English default), `docs/cn/` (Chinese)

## Authentication - Identity Passthrough

The gateway acts as a protocol bridge and does not maintain an independent auth system:
- Clients pass username/password via HTTP Basic Auth
- Gateway uses these credentials to connect to Fluss via SASL/PLAIN
- Permission control is entirely on the Fluss side (ZooKeeper ACL)
- Configuration: `[auth] type = "none" | "http_basic"`

## Rules That Must Be Followed

### 1. Code Standards

- Use Rust 2021 edition
- Follow Rust official coding standards (rustfmt + clippy)
- All public APIs must have documentation comments (`///`)
- Error handling uses `thiserror` for custom error types
- Async code uses `tokio` runtime
- HTTP framework: `axum`

### 2. Architecture Constraints

- Strictly follow the four-layer architecture: Protocol Frontend -> Query Engine -> Service Layer -> Backend Layer
- New code must be placed in the correct layer module
- `FlussBackend` trait is the core backend abstraction; all Fluss operations go through it
- Service layer encapsulates business logic; Controller/REST layer handles routing and serialization only

### 3. Design Principles

- **Stateless consumption**: Avoid the stateful consumer trap of Kafka REST
- **Connection pooling**: All Fluss connections must be managed through the connection pool
- **Fully async**: All I/O operations use `async/await`, returning Rust Futures
- **Error code convention**: Follow `HTTP Status Code + 2-digit suffix` pattern (e.g., 40401, 42901)

### 4. Security & Governance

- Authentication uses identity passthrough (HTTP Basic Auth -> SASL/PLAIN -> Fluss ACL)
- All endpoints go through rate limiting middleware by default
- Write path must pass four-dimensional rate limiting checks

### 5. Testing Requirements

- Use `MockFlussBackend` for unit tests
- Integration tests go in `tests/` directory
- New code must have corresponding tests

### 6. Dependency Management

- Prefer reusing existing FIP-32 components (FlussBackend trait, DataFusion integration, type mapping, etc.)
- New dependencies must be evaluated for necessity and license

## Key Reference Paths

### Codebases
| Resource | Path |
|----------|------|
| Fluss (Java) | `~/IdeaProjects/fluss-community/` |
| fluss-rust | `~/VscodeProjects/fluss-rust/` |
| DataFusion | `~/VscodeProjects/datafusion/` |

### Fluss Auth/Authz
| Resource | Path |
|----------|------|
| RPC Protocol | `~/IdeaProjects/fluss-community/fluss-rpc/src/main/proto/FlussApi.proto` |
| Auth Plugins | `~/IdeaProjects/fluss-community/fluss-common/src/main/java/.../security/auth/` |
| SASL/PLAIN | `~/IdeaProjects/fluss-community/fluss-common/src/main/java/.../security/auth/sasl/authenticator/` |
| ACL Authz | `~/IdeaProjects/fluss-community/fluss-server/src/main/java/.../server/authorizer/` |
| FlussPrincipal | `~/IdeaProjects/fluss-community/fluss-common/src/main/java/.../security/acl/FlussPrincipal.java` |

### fluss-rust Client
| Resource | Path |
|----------|------|
| Client Entry | `~/VscodeProjects/fluss-rust/crates/fluss/src/client/` |
| Connection Mgmt | `~/VscodeProjects/fluss-rust/crates/fluss/src/client/connection.rs` |
| Write Path | `~/VscodeProjects/fluss-rust/crates/fluss/src/client/write/` |
| Table Scan | `~/VscodeProjects/fluss-rust/crates/fluss/src/client/table/scanner.rs` |

### Design Documents
| Resource | Path |
|----------|------|
| FIP Design Doc | `~/AiWorkSpace/fluss-fip/FIP-REST-API/fluss-gateway-design.md` |
| Architecture | `docs/en/ARCHITECTURE.md` |
| API Design | `docs/en/API.md` |
| Project Progress | `docs/en/PROGRESS.md` |

## Load Documentation On Demand

The following documents should be loaded as needed during **planning and design phases**:

- `docs/en/ARCHITECTURE.md` - Architecture design details
- `docs/en/API.md` - API design details
- `docs/en/PROGRESS.md` - Project progress tracking
- `~/AiWorkSpace/fluss-fip/FIP-REST-API/fluss-gateway-design.md` - FIP complete design
