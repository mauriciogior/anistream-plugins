# Attribution and source provenance

The ani-cli provider adapts the protocol and parser from pystardust/ani-cli 5.1.6,
commit 0301d67d5768d332b049821a30082ad43d9a215a, and the native Rust adaptation in
mauriciogior/anistream at 4dd7811b5731366dd6c7675392166be37a8d9694.
Copyright belongs to the respective contributors. The provider and tooling in this
repository are distributed under GPL-3.0-or-later; see LICENSE. This is an independent
project, not an official ani-cli or Anistream release.

The WIT ABI and Cloudflare store template, including its compiled Rust store runtime,
come from bitknox/anistream at 811ac0610eb31168a3db38980ca9fde1c5818985.
Those files retain the upstream MIT license in LICENSE-store. Store source is available
at https://github.com/bitknox/anistream/tree/811ac0610eb31168a3db38980ca9fde1c5818985/crates/anistream-store-core
and its Worker adapter in deploy/cloudflare at the same revision. Upstream's
`deploy/sync.sh` rebuilds the generated store runtime. We have changed the Worker name,
store display name and local package scripts, not its protocol or authentication.

Keeping the provider in this repository does not change the Anistream app's license.
The store distributes plugin code, not video files. Use sources only where you have
the right to access their content.
