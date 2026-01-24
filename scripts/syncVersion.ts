/**
 * @file Sync version from `package.json` to `Cargo.toml`, useful in CI.
 * Run with `bun scripts/syncVersion.ts`
 */

const pkg = await Bun.file("package.json").json();
const version = pkg.version;

const cargo = await Bun.file("Cargo.toml").text();

const updatedCargo = cargo.replace(
  /^version\s*=\s*"[^"]*"/m,
  `version = "${version}"`,
);

await Bun.write("Cargo.toml", updatedCargo);

console.log(`Synced version ${version} to Cargo.toml`);

export {};
