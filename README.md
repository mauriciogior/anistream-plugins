# ani-cli for Anistream

An independent ani-cli provider plugin for Anistream, with a single-plugin store on
Cloudflare Workers. No custom Anistream app build is required.

- Plugin ID: `ani-cli`
- Store display name: **ani-cli for Anistream**
- Worker: `anistream-ani-cli`
- Runtime ABI: `anistream:provider@1.1.0`

The plugin runs inside Anistream's WASM sandbox. It searches, lists episodes and
resolves HLS streams and soft subtitles using host-provided HTTP access. The store
only distributes the plugin and updates; playback does not pass through Cloudflare.
It declares only `hianime.at` and `zokoanime.video` as fetch hosts. Hosts and markup
can change; that requires a new plugin version. Browser challenges are reported as
blocked. The plugin has no browser-challenge bypass or DRM support.

## Install

Join the store through **Providers → Plugin stores → Join a store**, paste the invite
link given by the store owner, confirm the displayed permissions and install **ani-cli**.
The store starts invite-only. There is no public join link until its owner chooses to
open joining. Store updates become available in Anistream when a new plugin version
is published.

Alternatively, build `dist/ani-cli.wasm` and import it through Anistream's plugin import
control. In a desktop/terminal installation, `anistream --plugins` prints the directory
where it can be placed. If using the earlier custom app with built-in ani-cli, use an
upstream app for this plugin to avoid a duplicate provider ID.

## Build and verify

Install Rust through rustup, with the pinned toolchain and `wasm32-wasip2` target.
Use a temporary HOME for tests; keep CARGO_HOME and RUSTUP_HOME pointed at your normal
Rust caches. Prefer an isolated CARGO_TARGET_DIR too.

```sh
cargo test --locked --manifest-path plugin/Cargo.toml --lib
cargo clippy --locked --manifest-path plugin/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path plugin/Cargo.toml -- --check
./scripts/build-plugin.sh
```

The build script loads the actual WASM in Anistream's interpreter, checks its ABI and
manifest, stamps the manifest required by the Cloudflare store, and reloads the stamped
component. It writes the installable component and SHA-256 checksum to `dist/`.
Its optional `--live QUERY` argument checks search, episodes and resolution through the
sandbox and validates an HLS master playlist, without fetching video segments.

## Run the store locally

```sh
cd store
bun install --frozen-lockfile
cp .dev.vars.example .dev.vars
# Replace the example code in .dev.vars with a long random local setup code.
bun run dev
```

Open `http://localhost:8787/admin/` to claim this local store. Local D1/R2 data stays in
`store/.wrangler/`, which is ignored. Never commit owner invites, setup codes, tokens or
store databases. The `store/` runtime is the upstream implementation, not a new store
protocol. See NOTICE.md for its pinned source and license.

## Deploy to Cloudflare

Use Workers, with D1 for records and R2 for plugin files. A static Pages deployment
cannot implement the store's signed index, invite and owner operations on its own.

```sh
cd store
bun install --frozen-lockfile
bunx wrangler login
bunx wrangler r2 bucket create anistream-ani-cli-files
bunx wrangler deploy
bunx wrangler secret put ANISTREAM_STORE_SETUP_CODE
```

Wrangler creates the D1 database when it has no ID in the config. Create the named R2
bucket before deploying; skip the bucket-create command when it already exists.
R2 must be enabled for the account. Review Cloudflare's current billing and limits
before enabling a paid plan. The store cannot be claimed until the setup secret exists.

Open `https://<worker>/admin/`, enter your private setup code, and redeem the returned
owner invite in Anistream. Then use **Open the admin page** from Anistream and upload
`dist/ani-cli.wasm`. Publish only this plugin in this store. On iPhone, use the web admin
page for owner actions. Invite your devices, or explicitly enable open joining if you
want a reusable public link. Keep the owner invite private.

For a custom domain, add it to the Worker in Cloudflare. Invite links use the request
origin unless ANISTREAM_STORE_URL is set in the Worker configuration.

## Update

Change the plugin package version for every published change; an existing version is
immutable. Build and verify the component, then upload the new `dist/ani-cli.wasm` through
the store admin page. Existing members can update without adding the store again.
The GitHub workflow builds and uploads a stamped component artifact for each push/PR.

To update the store runtime, copy `src/` and `store/` from a reviewed Anistream release's
`deploy/cloudflare/` directory, keep your Wrangler bindings and secrets, update the pinned
revision in NOTICE.md, then run `bun run check` and the store journey before deploying.
Keep D1/R2 resource IDs from the deployed config so updates retain members and plugins.

## License

Provider: GPL-3.0-or-later. Upstream WIT and store: MIT. See LICENSE, LICENSE-store and
NOTICE.md. Independent of the upstream project's release and license decisions.

## Verified on 29 October 2026

- Seven parser, stream and boundary tests passed; plugin and tooling clippy passed with warnings denied.
- The release WASM component is 398,724 bytes after manifest stamping.
- The component loaded and reloaded under Anistream's interpreter with default limits.
- Live interpreter check: 30 search hits, 13 episodes for the successful sample, one HLS
  stream and one subtitle; the master playlist was valid. One earlier sample returned
  NotFound and the next resolved. No video segments were requested by this check.
- The upstream store journey passed against a fresh local Cloudflare Worker using the
  real plugin, including signed-index/download integrity, invites, open joining,
  revocation and admin access.
- Worker bundle dry run passed. A new on-device playback test of this plugin is still
  pending; the earlier device confirmation was for the native-provider fork.
