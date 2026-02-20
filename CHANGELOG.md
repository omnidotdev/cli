# Changelog

## Unreleased

### Added

- `omni auth login` — authenticate with cloud Synapse using your Omni account email and password; stores the access token in `~/.config/omni/cli/config.toml`
- `omni auth logout` — clear the stored access token
- Cloud auth token is now used as the Bearer token for Synapse requests, enabling Omni Credits from the CLI without manual API key configuration
