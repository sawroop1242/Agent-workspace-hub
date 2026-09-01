# Release Engineering

Releases are built by GitHub Actions from the protected `rust` branch, from
version-tagged commits only, and only after all CI gates pass.

## Versioning

- Semver, tracked in `Cargo.toml` (`version = "0.1.0"`).
- Tags use the `v` prefix (`v0.1.0`). The binary reports its version via
  `awh --version`, built from the same Cargo metadata.

## Process

1. Land changes on `rust` via PR; CI must be green
   (`.github/workflows/rust.yml`: fmt, build+test on 3 OSes, clippy,
   cargo audit).
2. Bump `version` in `Cargo.toml` and commit.
3. Tag: `git tag v0.1.0 && git push origin v0.1.0`.
4. Trigger the release workflow (Actions → *Release* → *Run workflow*),
   entering the same version tag.
5. The workflow (`.github/workflows/release-rust.yml`):
   - **verify** job re-runs fmt + clippy + tests on the tagged ref, and
     **fails if the requested tag does not match `Cargo.toml`'s version**
     (prevents mislabeled artifacts);
   - **build** job compiles release binaries for 6 targets
     (linux x86_64/aarch64, android aarch64, macos x86_64/aarch64,
     windows x86_64) and attaches them to the GitHub release;
   - **checksums** job downloads all assets, generates `sha256sums.txt`,
     and attaches it together with `scripts/install.sh`.

A release can never come from a ref that fails CI: every artifact flows
through `verify` first, and the branch protection on `rust` requires the
same checks to have passed to land the code there.

## Artifacts

| Asset | Target |
| --- | --- |
| `awh-linux-x86_64` | x86_64-unknown-linux-gnu |
| `awh-linux-aarch64` | aarch64-unknown-linux-gnu |
| `awh-android-aarch64` | aarch64-linux-android |
| `awh-macos-x86_64` | x86_64-apple-darwin |
| `awh-macos-aarch64` | aarch64-apple-darwin |
| `awh-windows-x86_64.exe` | x86_64-pc-windows-msvc |
| `sha256sums.txt` | SHA-256 checksums for every binary |
| `install.sh` | Installer script |

## Verifying a release

```bash
curl -LO https://github.com/sawroop1242/Agent-workspace-hub/releases/download/vX.Y.Z/sha256sums.txt
sha256sum -c sha256sums.txt --ignore-missing   # with the downloaded asset present
awh --version                                   # must print vX.Y.Z's version
```

## Documentation consistency

Every release must ship with documentation that matches its behavior
(`README.md`, everything under `docs/`). The completeness audit
(`docs/completeness-audit.md`) records what is verified and what is not at
release time.

## Status

The workflow is authored and committed; the first tagged release has **not
yet been cut** — that happens after the hardening phases are accepted and the
`rust` branch protection rule is confirmed active.
