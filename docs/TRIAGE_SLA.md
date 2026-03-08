# Triage and SLA Policy

This document defines issue severity, response expectations, and ownership for post-launch operations.

## Required Labels

Base workflow labels:

- `triage`
- `incident`
- `bug`
- `regression`
- `performance`
- `security`
- `release-blocker`
- `severity/p0`
- `severity/p1`
- `severity/p2`
- `severity/p3`

Area labels:

- `area/runtime`
- `area/provider`
- `area/tui`
- `area/mcp`
- `area/tools`
- `area/state`

Issue forms capture `severity/*` and `area/*` as required fields, then maintainer triage maps
those values into labels.

## Severity Model

- `severity/p0`
  - Critical outage, data-loss risk, or security-critical behavior
  - No viable workaround
- `severity/p1`
  - Core user flow broken with high impact
  - Workaround exists but is operationally expensive
- `severity/p2`
  - Important defect with moderate impact
  - Workaround exists
- `severity/p3`
  - Low-impact defect, edge-case bug, or polish issue

## SLA Targets

- `severity/p0`
  - First response: 30 minutes
  - Mitigation plan: 2 hours
  - Target resolution or rollback decision: 24 hours
- `severity/p1`
  - First response: 4 hours
  - Mitigation plan: 1 business day
  - Target resolution: 3 business days
- `severity/p2`
  - First response: 1 business day
  - Target resolution: 10 business days
- `severity/p3`
  - First response: 3 business days
  - Scheduled into backlog by next planning cycle

## Ownership and Escalation

- Initial owner: on-call maintainer for release window
- Escalation path:
  - P0: immediate escalation to release owner and repository maintainer
  - P1: escalation to release owner if unresolved after one business day
- By end of initial triage (within first-response SLA), each issue must have:
  - severity classification (field + label)
  - area classification (field + label)
  - explicit assignee
- `Triage Guard` workflow enforces this metadata by adding `needs-triage` and posting a reminder comment until fields are complete.

## Triage Workflow

1. Create issue using bug/incident template.
2. During initial triage, add labels (`triage`, type, severity, and area) based on template fields.
3. Assign owner and set escalation expectation from SLA target.
4. Confirm repro and impact.
5. Decide path: hotfix (`docs/PATCH_RELEASE_WORKFLOW.md`) or scheduled backlog.
6. Post updates until closure with root cause and follow-up.

## First Usage Window Report

Within 72 hours of launch, publish a summary in `docs/reports/` including:

- Open issues by severity
- Mean response time vs SLA
- Incidents requiring rollback or patch
- Top 3 risks carried into next cycle
