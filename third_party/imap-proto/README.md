Vendored from crates.io crate `imap-proto` `0.10.2`.

Provenance:

- Upstream crate: <https://crates.io/crates/imap-proto/0.10.2>
- Upstream repository: <https://github.com/djc/tokio-imap>
- Original crates.io checksum: `16a6def1d5ac8975d70b3fd101d57953fe3278ef2ee5d7816cba54b1d1dfc22f`

Why this exists:

- `imap = "2"` still depends on the `0.10.x` line.
- The workspace carries a minimal local patch so future Rust versions do not turn the dependency's macro/lifetime patterns into a build break.

Maintenance rule:

- Keep this directory as close to upstream `0.10.2` as possible.
- If you update or regenerate it, record the new upstream source and checksum here.
- Avoid unrelated local edits so the `Cargo.toml` `[patch.crates-io]` entry remains auditable.
