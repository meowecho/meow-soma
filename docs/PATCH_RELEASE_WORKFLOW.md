# Patch Release Workflow

Use this flow for high-priority post-launch fixes (`v0.1.x`).

## Trigger Criteria

Start a patch release when one of these is true:

- Open `severity/p0` issue
- Open `severity/p1` issue without acceptable workaround
- Release-blocking regression in install, config setup, or core command surface

## Branch and Tag Convention

- Patch branch: `release/v0.1.x`
- Patch tag: `v0.1.<patch>`

Example:

```bash
git fetch --tags origin
BASE_TAG="$(git tag --list 'v0.1.*' --sort=-version:refname | head -n1)"
NEXT_TAG="v0.1.<next>"
git checkout -b release/v0.1.x "$BASE_TAG"
git cherry-pick -x <fix-commit>
git tag "$NEXT_TAG"
git push origin release/v0.1.x
git push origin "$NEXT_TAG"
```

Base `release/v0.1.x` from the latest stable `v0.1.<n>` tag before cherry-picking fixes.

## Minimum Verification Gates

Before pushing patch tag:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo check`
- `cargo test`
- Targeted smoke for affected command path

## Communication Requirements

- Update `CHANGELOG.md` with patch notes
- Post incident summary with:
  - impact
  - root cause
  - fix scope
  - user action required (if any)

## Merge Back Strategy

After patch is released:

1. Merge/cherry-pick patch commits back to `main`
2. Confirm no divergence between `release/v0.1.x` and `main` on fixed files
3. Update backlog for remaining related work

## Incident Closeout

- Ensure issue has:
  - final severity
  - owner
  - root cause
  - prevention follow-up task
- Link to release tag and relevant PR/commit in the issue
