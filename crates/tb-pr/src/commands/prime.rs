use crate::error::Result;

pub fn run() -> Result<()> {
    print!(
        r#"# tb-pr — GitHub PR radar

A kanban-style TUI + CLI for tracking GitHub PRs that need your attention
across the Productive organization.

## Status

Skeleton only. Data fetching, cache, and TUI land in later milestones.

## Commands (stubs)

- `tb-pr tui`     — interactive kanban dashboard (not implemented)
- `tb-pr list`    — pretty / JSON listing of PRs (not implemented)
- `tb-pr show`    — detail view of one PR (not implemented)
- `tb-pr refresh` — force full fetch, update cache (not implemented)
- `tb-pr open`    — open PR URL in browser (not implemented)
- `tb-pr prime`   — this context dump
- `tb-pr skill`   — install or print SKILL.md
- `tb-pr config init|show` — manage configuration
- `tb-pr doctor`  — verify gh auth, config, cache health
"#
    );
    Ok(())
}
