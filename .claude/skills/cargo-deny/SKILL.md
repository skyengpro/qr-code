---
name: cargo-deny
description: Run and fix cargo-deny (licenses, RustSec advisories, banned/duplicate crates, sources) for this workspace using deny.toml. Use when adding or upgrading dependencies, when `cargo deny check` or the CI deny job fails, or when asked about dependency policy.
---

# cargo-deny

Configuration: `deny.toml` at the repo root. Install once with
`cargo install --locked cargo-deny`.

```sh
cargo deny check                 # all four checks
cargo deny check licenses        # one check
cargo deny check advisories      # fetches the RustSec DB (needs network)
cargo deny list                  # licenses of every crate in the graph
```

## Fixing failures

* **licenses** — a dependency uses a license not in `[licenses].allow`. Check whether it is
  permissive and compatible with closed-source apps (MIT/Apache/BSD/ISC/Zlib are fine;
  GPL/AGPL/LGPL are not; MPL-2.0 needs a team decision). Prefer replacing the crate. Only
  extend `allow` with a reviewed license, never with a per-crate `exceptions` hack unless
  the license is crate-specific.
* **advisories** — upgrade the crate (`cargo update -p name`). If no fix exists and the
  vulnerable code path is unreachable, add an `ignore = [{ id = "RUSTSEC-…", reason = "…" }]`
  entry with a concrete reason and a TODO to remove it.
* **bans / duplicates** — `multiple-versions = "warn"`: try to align versions in
  `[workspace.dependencies]` (`cargo tree -d` shows duplicates). Duplicates that come only
  from upstream crates are acceptable warnings. `openssl` is banned outright.
* **sources** — only crates.io is allowed. Git dependencies need an explicit
  `allow-git` entry and a reason in the PR.

Never silence a check by loosening `deny.toml` without saying why in the commit message.
