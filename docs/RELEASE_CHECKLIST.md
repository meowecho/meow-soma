# Release Checklist

Use this checklist for every version tag.

## Pre-release

- [ ] Phase scope to release is complete and accepted
- [ ] `CHANGELOG.md` has target version section (`## [X.Y.Z] - YYYY-MM-DD`)
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo check`
- [ ] `cargo test`

## Build Artifacts

- [ ] Run `scripts/release-local.sh vX.Y.Z`
- [ ] Verify tarball exists in `dist/`
- [ ] Verify checksum file exists and matches artifact
- [ ] Validate executable runs:
  - [ ] `./target/release/meow --help`

## Tag and Publish

- [ ] `git tag vX.Y.Z`
- [ ] `git push origin vX.Y.Z`
- [ ] GitHub release workflow finished successfully
- [ ] GitHub Release includes artifacts and checksums
- [ ] For milestone release (starting `v0.1.0`), run `docs/LAUNCH_CHECKLIST.md`

## Post-release Smoke

- [ ] Install binary on macOS or Linux
- [ ] `meow config setup --provider <provider>`
- [ ] Set provider credential env var if required
- [ ] `meow config validate`
- [ ] `meow ask "health check"`

## Follow-up

- [ ] Move remaining items to `Unreleased` section
- [ ] Track release issues with `triage` + type (`incident`/`bug`) + severity + area labels
- [ ] Use `docs/TRIAGE_SLA.md` and `docs/METRICS_BASELINE.md` for first 72h window
