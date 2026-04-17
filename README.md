# revu

A terminal UI for reviewing your own code before you push it — or for walking through any diff, leaving inline comments, and publishing them to GitHub as real PR review comments.

Built in Rust. Made for the keyboard. Runs inside any git repo.

## Why

Most of my bugs are the ones I spot myself, five minutes after pushing. `revu` is the "pre-flight" — open a TUI, step through your diff line by line, leave inline notes, tweak, publish. No context switch to the browser, no tab shuffle, no losing your train of thought.

## Features

- **Diff vs merge-base** — current branch against `origin/HEAD` (or any base you pass). Falls back to working-tree diff if there are no committed changes.
- **Inline comments** on any line. Drafts live in `.revu/comments.json` at the repo root (auto-gitignored) and survive reloads.
- **Orphan detection** — if the line a comment was anchored to disappears between reloads, the comment is flagged instead of silently deleted.
- **Publish to GitHub** — `:p` turns drafts into real review comments on the PR for the current branch (via `gh`). No open PR? `revu` offers to make one.
- **Voice mode** — `:v` toggles local speech-to-text via whisper.cpp (`ggml-base.en`, ~140 MB, auto-downloaded on first use). Dictate a comment, keep your hands on the keyboard.
- **Multi-line range comments** — select a range with `V` and the comment anchors to the whole span on GitHub.
- **Terminal-native palette** — claude-code-ish greens/reds on the diffs, warm accents, nothing garish.
- **Mouse friendly** — click files, click lines, scroll the diff. Also fully navigable with just the keyboard.

## Meet Snappy 🦞

Every review tool needs a mascot. `revu` has Snappy, a lobster who lives in the top-right corner of the TUI. Snappy stays put while the water flows past him — claws snap every couple of seconds, eyes wink now and then, and every 45–90 seconds he pops up with an affirmation:

```
~∿~≈~∿≈<(°º°)>~∿~≈~∿≈~∿≈
» Snappy: small PRs, big wins.
```

He has a handful of moods out of the box ("snip snap, ship it!", "delete more than you add.", "green is good. red is honest."), but you can drown him in your own encouragement by editing `~/.revu/affirmations.txt` — one line per affirmation, auto-created the first time he boots.

Snappy doesn't do anything important. That's the point.

## Install

```sh
git clone https://github.com/lamng3/revu.git
cd revu
cargo build --release
# optionally:
cp target/release/revu /usr/local/bin/
```

Prereqs for the full experience:
- **git** (obviously)
- **[gh](https://cli.github.com)** — for publishing review comments
- **[whisper.cpp](https://github.com/ggerganov/whisper.cpp)** — `brew install whisper-cpp` on macOS; needed for `:v` voice mode

## Usage

```sh
cd /path/to/your/repo
revu
# or point at a specific repo / base ref
revu /path/to/repo
revu . origin/develop
```

### Keys

**Navigation**
| key | action |
|---|---|
| `↑` / `↓` / `k` / `j` | move by line |
| `Shift+↑` / `Shift+↓` | next / previous file |
| `n` / `]` | next hunk |
| `N` / `[` | previous hunk |
| `g` / `G` | top / bottom of file |
| `J` | jump to latest diff |
| `PgUp` / `PgDn` | page scroll |
| mouse click | select line / file / comment |

**Commenting**
| key | action |
|---|---|
| `c` | comment on current line |
| `V` | start / end a multi-line selection |
| `x` | delete comment on current line |
| `Enter` | save comment |
| `Shift+Enter` | newline inside comment |
| `Esc` | cancel |

**Command mode** — type `:` then one of:
- `:help` — show all keys
- `:w` — save drafts
- `:r` — reload diff
- `:v` — toggle voice mode
- `:p` — publish drafts to the PR
- `:q` — quit

**Overlays**
- `?` — help
- `f` — file picker overlay
- `m` — comments overlay

## How drafts are stored

```
your-repo/
├── .revu/
│   ├── .gitignore        # auto-written, keeps drafts out of git
│   └── comments.json     # your drafts; keyed by (file, line, side, commit_sha)
└── …
```

Each comment records the tip SHA it was written against. On reload `revu` verifies each line still exists in the diff. If it doesn't, the comment is marked `orphaned` and shown with a `?` in the comments panel so nothing ever disappears silently.

## Publishing

`:p` posts every unpublished, non-orphaned draft to the PR attached to the current branch via `gh api …/pulls/:num/comments`. Multi-line selections post as real GitHub range comments (`start_line` + `line`). Once a comment is published it's marked `✓` locally — running `:p` again won't re-post it.

If the branch has no open PR, `:p` will offer to create one for you.

## Built with

- [ratatui](https://github.com/ratatui-org/ratatui) + [crossterm](https://github.com/crossterm-rs/crossterm) — the TUI
- [cpal](https://github.com/RustAudio/cpal) + [hound](https://github.com/ruuda/hound) — audio capture + WAV
- [whisper.cpp](https://github.com/ggerganov/whisper.cpp) — local STT
- [gh](https://cli.github.com) — GitHub publishing
- one (1) cold-blooded lobster

## License

MIT.
