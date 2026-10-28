# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues. Use the `gh` CLI for all operations.

## Conventions

- Create an issue: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- Read an issue: `gh issue view <number> --json number,title,body,labels,comments`.
- List issues: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- Make an issue a sub-issue of a parent: `gh issue create --parent <parent> ...`, or `gh issue edit <parent> --add-sub-issue <child>` afterwards (`gh` 2.94+). Older `gh`: `gh api --method POST repos/<owner>/<repo>/issues/<parent>/sub_issues -F sub_issue_id=<child-db-id>`.
- Blocking: GitHub's native issue dependencies are the canonical, UI-visible representation. Add an edge with `gh issue create --blocked-by <n>,<n> ...` or `gh issue edit <child> --add-blocked-by <blocker>` (`gh` 2.94+). Where dependencies aren't available, fall back to a `Blocked by: #<n>, #<n>` line near the top of the child body.
- Comment on an issue: `gh issue comment <number> --body "..."`.
- Apply / remove labels: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`.
- Close: post the explanation with `gh issue comment <number> --body "..."`, then `gh issue close <number>`.

Infer the repo from `git remote -v`; `gh` does this automatically when run inside a clone.

## Pull requests as a triage surface

PRs as a request surface: no.

When set to `yes`, PRs are handled like issues and use the `gh pr` equivalents. This repo keeps the flag off by default.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Read it with `gh issue view <number>` or `gh issue list`.

## Wayfinding operations

Used by `/wayfinder`. The map is a single issue with child issues as tickets.

- Map: a single issue labelled `wayfinder:map`, holding Notes / Decisions-so-far / Fog in the body.
- Child ticket: an issue linked to the map as a GitHub sub-issue. Where sub-issues are not enabled, add `Part of #<map>` at the top of the child body.
- Blocking: use `Blocked by:` lines or GitHub dependencies, as above.
- Claim: `gh issue edit <n> --add-assignee @me`.
- Resolve: `gh issue comment <n> --body "<answer>"`, then `gh issue close <n>`.

This repo's remote is `https://github.com/mawdesign/serviceFlow.git`. The GitHub CLI must be authenticated before issue creation or label creation can be used in practice.
