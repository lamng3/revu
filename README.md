# 🦞 revu 

**Review your own code before you ship it.** A keyboard-first TUI for walking through diffs, leaving inline comments, and publishing them straight to GitHub.

```
~∿~≈~∿≈<(°º°)>~∿~≈~∿≈~∿≈
» Snappy: small PRs, big wins.
```

<img width="1885" height="913" alt="image" src="https://github.com/user-attachments/assets/3d166ff1-cc31-498d-9f77-99ae0e6bb602" />

## What it does

- 📂 Loads your branch's diff against `origin/HEAD` (or any base)
- ✎ Drop inline comments on any line — single or multi-line ranges
- 💾 Drafts saved locally in `.revu/` and survive reloads
- 🚀 `:p` publishes every draft to your PR as real review comments (via `gh`)
- 🎙 `:v` dictates comments via local whisper.cpp — no cloud, no tokens
- 🦞 Ships with **Snappy**, a lobster who lives in the corner and cheers you on

## Meet Snappy

Snappy is the whole point. He sits there while water flows past, snaps his claws, and drops affirmations like *"delete more than you add"* and *"snip snap, ship it!"*. Pet him with `:pet`. Customize his voice at `~/.revu/affirmations.txt`.

## Quick start

```sh
git clone https://github.com/lamng3/revu.git && cd revu
cargo build --release
cd /your/repo && revu
```

Press `?` for keys. `:q` to quit.

**Needs:** `git`, [`gh`](https://cli.github.com) for publishing, [`whisper-cpp`](https://github.com/ggerganov/whisper.cpp) for voice.

## Keys (the short list)

| | |
|---|---|
| `j` / `k` | line down / up |
| `Shift+↑↓` | switch file |
| `n` / `N` | next / prev hunk |
| `c` | comment on current line |
| `V` | multi-line selection |
| `:p` | publish to PR |
| `:v` | voice mode |
| `:r` | reload diff |
| `?` | help |

## License

MIT. Built with [ratatui](https://github.com/ratatui-org/ratatui), [cpal](https://github.com/RustAudio/cpal), [whisper.cpp](https://github.com/ggerganov/whisper.cpp), and one cold-blooded lobster.
