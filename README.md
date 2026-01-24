<div align="center">
  <img src="/assets/logo.png" width="100" />

  <h1 align="center">Omni CLI</h1>

[Website](https://cli.omni.dev) | [Docs](https://docs.omni.dev/cli/overview) | [Provide feedback on Omni Backfeed](https://backfeed.omni.dev/organizations/omni/projects/cli) | [Join Omni community on Discord](https://discord.gg/omnidotdev)

</div>

**Omni CLI** is an agentic CLI for the Omni ecosystem. It provides three interfaces:

- **CLI**: Traditional command-line interface for scripting and automation
- **TUI**: Interactive terminal user interface for visual workflows
- **HTTP API**: RESTful API for remote access and integrations

## Installation

```bash
# From source
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

## License

The code in this repository is licensed under MIT, &copy; [Omni LLC](https://omni.dev). See [LICENSE.md](LICENSE.md) for more information.
