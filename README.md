# 🦞 revu

**Review your own code before you ship it.**

Read a clean diff, leave notes, and publish the review—all from the terminal.

<img width="1887" height="917" alt="image" src="https://github.com/user-attachments/assets/fbc20fd5-9707-4f2d-9ea1-e44663f59aba" />

## Quick start

```sh
git clone https://github.com/lamng3/revu.git && cd revu
cargo build --release
./target/release/revu /path/to/repo
```

## Keys

| Key | Action |
| --- | --- |
| `↑` `↓` or `j` `k` | Read the diff |
| `Tab` / `Shift+Tab` | Change file |
| `Enter` | Add or edit a comment |
| `Shift+↑` / `Shift+↓` | Select multiple lines |
| `Delete` | Remove a draft |
| `?` | Open the quick reference |
| `q` | Quit |

Useful commands: `:p` publishes drafts, `:pr [title]` creates a pull request,
and `:v` dictates a comment with local whisper.cpp.

Drafts stay in `.revu/`. Publishing requires [`gh`](https://cli.github.com);
voice requires [`whisper-cpp`](https://github.com/ggerganov/whisper.cpp).

## License

MIT.
