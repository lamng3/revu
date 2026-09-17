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
| `↑` / `↓` | Move through the diff |
| `←` / `→` or `Shift+wheel` | Pan code and disable wrapping |
| `w` | Toggle code wrapping |
| `a` | Toggle full file / modified sections |
| `Tab` / `Shift+Tab` | Next / previous file |
| `Enter` | Add or edit a comment |
| `Shift+↑` / `Shift+↓` | Extend a line selection |
| `Delete` | Remove a draft |
| `f` / `m` | Browse files / comments |
| `Shift+R` | Reload changes |
| `?` | Show the quick reference |
| `q` | Quit |

Long code lines pan horizontally by default; press `w` to wrap them instead.
Drag the files panel's rounded right border to resize it. Line-number and
review gutters stay fixed while code moves. The bottom scrollbar can also be
clicked or dragged.

Modified rows use light-green and light-red backgrounds instead of `+` and `-`
markers.

Comments are saved locally in `.revu/`.

## Commands

Type `:` to enter a command.

| Command | Action |
| --- | --- |
| `:p` | Publish draft comments |
| `:pr [title]` | Create a pull request |
| `:v` | Start or stop voice recording |
| `:all` | Toggle full file / modified sections |
| `:pet` | Pet Snappy |
| `:q` | Quit |

Voice transcription uses local
[`whisper.cpp`](https://github.com/ggerganov/whisper.cpp). Native recording is
currently available on macOS and Windows; Linux builds keep voice recording
disabled to avoid a system ALSA dependency.

## License

MIT.
