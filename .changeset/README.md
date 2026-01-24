# Changesets

This project uses [changesets](https://github.com/changesets/changesets) for version management.

## Adding a changeset

When making changes that should be released, run:

```bash
bun changeset
```

This will prompt you to:
1. Select the type of change (major/minor/patch)
2. Write a summary of your changes

## Release flow

1. PRs with changesets are merged to master
2. The release workflow creates a "Version Packages" PR
3. When that PR is merged, binaries are built and a GitHub release is created
