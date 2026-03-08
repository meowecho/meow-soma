# Release Launch Checklist

Use this checklist for coordinated releases (starting with `v0.1.0`).
This complements the generic release checklist in `docs/RELEASE_CHECKLIST.md`.

## Scope Freeze

- [ ] `main` is frozen for launch scope (only release-critical fixes allowed)
- [ ] `CHANGELOG.md` includes target version section (`## [0.1.0] - YYYY-MM-DD`)
- [ ] Release notes draft is prepared and reviewed
- [ ] Rollback owner is assigned

## Go/No-Go Criteria

- [ ] CI on `main` is green (`fmt`, `clippy`, `check`, `test`)
- [ ] `scripts/release-local.sh v0.1.0` succeeds
- [ ] Install path validated on target platforms from release artifacts
- [ ] Provider smoke checks validated for at least one hosted provider and one local provider mode
- [ ] Security-sensitive tool approval paths tested (`tool exec` risky commands require approval)

## Release Execution

- [ ] Create tag: `git tag v0.1.0`
- [ ] Push tag: `git push origin v0.1.0`
- [ ] Verify `Release` workflow succeeded
- [ ] Verify GitHub Release assets and checksums are published
- [ ] Publish release notes

## Launch Communications

- [ ] Post launch note in project channel with version + highlights + known limitations
- [ ] Link install guide and quickstart (`docs/INSTALL.md`, `README.md`)
- [ ] Share rollback contact and triage channel
- [ ] Run `Sync Labels` workflow to ensure triage label catalog is present
- [ ] Confirm `Triage Guard` workflow is active

## Rollback Criteria

Rollback should be triggered if any of the following occurs:

- [ ] Install path is broken for a primary platform
- [ ] P0/P1 defect has no mitigation within SLA window
- [ ] Runtime command surface regression (`meow`, `ask`, `run`, `tool`, `mcp`) blocks normal usage

## 24h Post-Launch Review

- [ ] Gather incident/bug volume by severity
- [ ] Validate metrics trend against `docs/METRICS_BASELINE.md`
- [ ] Confirm ownership for all open P0/P1 issues
- [ ] Produce first triage summary report

## 72h Post-Launch Review

- [ ] Confirm no unresolved P0 issues
- [ ] Confirm mitigation plan for any open P1 issues
- [ ] Cut patch release if needed using `docs/PATCH_RELEASE_WORKFLOW.md`
- [ ] Refresh prioritized next-cycle items in `docs/BACKLOG_V0_2.md`
