<div align="center">
  <img src="/assets/logo.png" width="100" />

  <h1 align="center">Omni CLI</h1>

[Website](https://cli.omni.dev) | [Docs](https://docs.omni.dev/armory/omni-cli) | [Feedback](https://backfeed.omni.dev/workspaces/omni/projects/cli) | [Discord](https://discord.gg/omnidotdev)

</div>

**Omni CLI** is an agentic CLI for the Omni ecosystem. It provides three interfaces:

- **CLI**: Traditional command-line interface for scripting and automation
- **TUI**: Interactive terminal user interface for visual workflows
- **HTTP API**: RESTful API for remote access and integrations

## Installation

```bash
cargo install omnidotdev-cli
```

Or build from source:

```bash
git clone https://github.com/omnidotdev/cli
cd cli
cargo build --release
# Binary will be at target/release/omni
```

## Quick Start

### TUI Mode (Default)

```bash
omni
```

### CLI Mode

```bash
omni agent "summarize the README in this directory"
```

### HTTP API Mode

```bash
omni serve --host 0.0.0.0 --port 7890
```

## Configuration

```bash
omni config path    # Show config file location
omni config show    # Display current config
```

Configuration file (`~/.config/omni/cli/config.toml`):

```toml
[agent]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
max_tokens = 8192

[api]
host = "0.0.0.0"
port = 7890
token = "omni_..."  # Generate with: omni config generate-token
```

## HTTP API

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check (public) |
| `POST` | `/api/agent` | Execute an agentic task |
| `POST` | `/api/agent/stream` | Execute with SSE streaming |
| `GET` | `/api/history` | Get task execution history |
| `GET` | `/api/docs` | Swagger UI documentation |

### Authentication

For remote access, generate and configure an API token:

```bash
omni config generate-token
```

Then set it in your config or environment:

```bash
export OMNI_API_TOKEN="omni_..."
```

Requests require the `Authorization: Bearer <token>` header:

```bash
curl -X POST http://localhost:7890/api/agent \
  -H "Authorization: Bearer omni_..." \
  -H "Content-Type: application/json" \
  -d '{"prompt": "What is 2+2?"}'
```

## Development

### Version Syncing

Omni CLI uses a dual-package setup (Rust crate + npm package) with automated version synchronization:

- **Source of truth**: `package.json` holds the canonical version, and is used for Changesets
- **Sync script**: `scripts/syncVersion.ts` propagates the version to `Cargo.toml`
- **Changesets**: Manages version bumps and changelog generation

The sync script runs automatically during the release process via the `version` npm script:

```sh
bun run version  # syncs `package.json` version → `Cargo.toml`
```

### CI/CD

Two GitHub workflows handle versioning:

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `test.yml` | Push/PR to `master` | Runs tests and builds |
| `release.yml` | Push to `master` | Creates releases via Changesets, builds multi-platform binaries |

### Release Process

1. Create a changeset: `bun changeset`
2. Push to `master`
3. Changesets action creates a "Version Packages" PR
4. Merge the PR to trigger a release with binaries for:
   - `x86_64-unknown-linux-gnu`
   - `aarch64-unknown-linux-gnu`
   - `x86_64-apple-darwin`
   - `aarch64-apple-darwin`
5. **Manually** publish to crates.io: `cargo publish`

## License

The code in this repository is licensed under MIT, &copy; [Omni LLC](https://omni.dev). See [LICENSE.md](LICENSE.md) for more information.
