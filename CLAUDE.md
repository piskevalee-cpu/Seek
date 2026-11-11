# SEEK

Spec: docs/SPEC.md (source of truth). Rust workspace, ratatui TUI.
- Branch per task from develop; never commit to main or develop.
- Before push: cargo fmt && cargo clippy -- -D warnings && cargo test
- Conventional Commits. Scopes: tui core llm local tools skills widgets install docs
- Keep system prompt + default tool schemas under 1,500 tokens (a test enforces it).
- UI: every piece of info has exactly one home (SPEC 10.2). Never add a second display.
- Do not implement features from later milestones.
- After a change, walk docs/TESTING.md: it is the checklist for the TUI by hand.
