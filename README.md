# 🦞 revu

Read your code changes before you ship them.

`revu` is a small terminal diff reader for reviewing a branch, drafting inline
comments, and publishing them to a GitHub pull request.

## Install

```bash
git clone https://github.com/lamng3/revu.git && cd revu
cargo install --path .
```

Requires Rust and Git. Publishing comments or creating a pull request also
requires the authenticated [`gh`](https://cli.github.com) CLI.

## Usage

Run `revu` inside a repository:

```bash
revu
```

Or pass a repository and optional base branch:

```bash
revu /path/to/repository
revu /path/to/repository main
```

The default view compares the current branch with `main`, `master`, or the
remote default branch. Working-tree and untracked changes are included.

## Controls

| Key | Action |
| --- | --- |
| `↑` / `↓` or `j` / `k` | Move through the diff |
| `Tab` / `Shift+Tab` | Next / previous file |
| `[` / `]` | Previous / next hunk |
| `Enter` | Add or edit a comment |
| `Shift+↑` / `Shift+↓` | Extend a line selection |
| `Delete` | Remove a draft |
| `f` / `m` | Browse files / comments |
| `Shift+R` | Reload changes |
| `?` | Show the quick reference |
| `q` | Quit |

Comments are saved locally in `.revu/`.

## Commands

Type `:` to enter a command.

| Command | Action |
| --- | --- |
| `:p` | Publish draft comments |
| `:pr [title]` | Create a pull request |
| `:v` | Start or stop voice recording |
| `:pet` | Pet Snappy |
| `:q` | Quit |

Voice transcription uses local
[`whisper.cpp`](https://github.com/ggerganov/whisper.cpp). Native recording is
currently available on macOS and Windows; Linux builds keep voice recording
disabled to avoid a system ALSA dependency.

## License

MIT.
