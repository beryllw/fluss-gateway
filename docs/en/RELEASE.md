# Versioning & Release Guide

This document describes the versioning strategy and release process for Fluss Gateway.

[Chinese version](../cn/RELEASE.md)

## Version Scheme

The project follows [Semantic Versioning (SemVer)](https://semver.org/), formatted as `MAJOR.MINOR.PATCH`:

| Type | When to Increment | Example |
|------|-------------------|---------|
| MAJOR | Incompatible API changes | `1.0.0` → `2.0.0` |
| MINOR | Backward-compatible new features | `0.1.0` → `0.2.0` |
| PATCH | Backward-compatible bug fixes | `0.1.0` → `0.1.1` |

Pre-release versions use a `-` suffix, e.g., `1.0.0-beta.1`.

## Where the Version Lives

The `version` field in `Cargo.toml` is the **single source of truth**. All other locations read from it automatically:

| Location | How It Gets the Version | Notes |
|----------|------------------------|-------|
| `Cargo.toml` | Defined directly | The only place you need to update manually |
| CLI `--version` | `clap` reads `CARGO_PKG_VERSION` | Run `fluss-gateway --version` to see it |
| OpenAPI docs | `env!("CARGO_PKG_VERSION")` | Displayed in Swagger UI |
| Git tag | Created by cargo-release | Format: `v{version}`, e.g., `v0.2.0` |

## Release Process

### Prerequisites (One-Time Setup)

```bash
# Install cargo-release
cargo install cargo-release
```

### Standard Release

```bash
# 1. Make sure you're on main and up to date
git checkout main
git pull

# 2. Verify CI checks pass
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --bin fluss-gateway

# 3. Release (dry run first, then execute)
cargo release patch           # dry run: shows what will happen
cargo release patch --execute # actually execute the release
```

`cargo release` automatically performs these steps:

1. Updates `version` in `Cargo.toml` (e.g., `0.1.0` → `0.1.1`)
2. Commits the change: `chore: release v0.1.1`
3. Creates a Git tag: `v0.1.1`
4. Pushes both the commit and tag to the remote

Once pushed, the GitHub Actions Release workflow triggers automatically:
- Multi-platform binary builds (Linux x86_64/aarch64, macOS aarch64)
- Multi-arch Docker image build and push to GHCR
- GitHub Release creation (draft — requires manual confirmation to publish)

### Choosing a Version Bump

```bash
cargo release patch            # 0.1.0 → 0.1.1  (bug fix)
cargo release minor            # 0.1.0 → 0.2.0  (new feature)
cargo release major            # 0.1.0 → 1.0.0  (breaking change)
cargo release 0.3.0            # explicit version
cargo release 1.0.0-beta.1     # pre-release version
```

### Manual Release (Fallback)

If you prefer not to use cargo-release:

```bash
# 1. Edit the version in Cargo.toml
#    version = "0.1.0" → version = "0.2.0"

# 2. Update Cargo.lock
cargo check

# 3. Commit
git add Cargo.toml Cargo.lock
git commit -m "chore: release v0.2.0"

# 4. Tag
git tag v0.2.0

# 5. Push
git push && git push --tags
```

### Manual CI Trigger

The Release workflow also supports manual triggering via GitHub Actions (`workflow_dispatch`), useful for hotfixes or test releases:

1. Go to GitHub repo → Actions → Release workflow
2. Click "Run workflow"
3. Enter the version (e.g., `v0.2.0`) — this field is required

## Branching Strategy

The project uses a simple trunk-based development model:

```
main (default branch)
  ├── feature/xxx  →  PR → merge to main
  ├── fix/xxx      →  PR → merge to main
  └── v0.2.0 (tag) ← tag from main to release
```

- **main branch**: Always kept in a releasable state; all features and fixes merge via PR
- **No release branches**: Tags are created directly from main
- **Tag = Release**: Pushing a `v*` tag triggers CI to build and publish

## CI/CD Pipelines

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| CI (`ci.yml`) | PR to main | Format check, Clippy, unit tests, integration tests, build check |
| Release (`release.yml`) | Push `v*` tag or manual trigger | Multi-platform build, Docker image, GitHub Release creation |

## Related Files

| File | Description |
|------|-------------|
| `Cargo.toml` | Version definition (single source of truth) |
| `release.toml` | cargo-release configuration |
| `.github/workflows/release.yml` | Release CI workflow |
| `.github/workflows/ci.yml` | PR CI workflow |
