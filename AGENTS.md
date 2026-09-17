# revu contributor guide

## Product direction

- Keep `revu` lightweight, fast to launch, and easy to ship as one terminal binary.
- Optimize for reading a diff like a book: code is the focus and chrome stays quiet.
- Prefer familiar, discoverable controls: arrows, Enter, Delete, Tab, and shifted actions.
- Power-user aliases may remain compatible, but do not crowd the UI or manual with them.
- Use ANSI colors for terminal compatibility: black context rows with white
  code, plus very pale indexed green and red backgrounds for changed rows.
- Use GitHub pull-request diffs as the interaction reference without copying its web UI.
- Mouse support should complement complete keyboard navigation.

## Code map

- `src/main.rs`: terminal lifecycle and keyboard/mouse event routing.
- `src/app.rs`: application state, navigation, selection, comments, and interaction logic.
- `src/ui.rs`: ratatui layout, styling, overlays, and click geometry.
- `src/diff.rs` and `src/git.rs`: unified-diff parsing and Git operations.
- `src/comments.rs` and `src/publish.rs`: local drafts and GitHub publishing.
- `src/voice.rs`: optional local recording/transcription; Linux must build without ALSA.

## Working conventions

- Make focused changes and preserve existing behavior unless the request changes it.
- Keep rendering in `ui.rs`; put reusable interaction state and bounds logic in `app.rs`.
- Clamp dimensions and scroll offsets so narrow terminals and repository changes stay safe.
- Keep line-number/review gutters fixed while code pans; offer wrapping as a
  simple toggle rather than replacing horizontal navigation.
- Keep both compact changed-section and full-file review modes available.
- Avoid background animation or redraw loops that consume CPU without useful state changes.
- Keep `README.md` a concise manual: install, usage, controls, commands, and requirements.
- Do not add a README image until the maintainer supplies the replacement.

## Verification

- Run `cargo test` and `git diff --check` for every change.
- For dependency or platform changes, verify `cargo check --target x86_64-unknown-linux-gnu`.
- After completing any change, run `cargo install --path . --force` so the local
  `revu` command always reflects the latest working tree.
- Commit or push only when explicitly requested.
