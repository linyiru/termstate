# termstate (CLI)

Report program status to the terminal with **OSC 7501**, the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status),
from any shell script or hook.

```sh
cargo install termstate-cli

termstate emit working --app backup --msg "Syncing photos"
rsync -a ~/Photos backup:/photos \
  && termstate emit done  --app backup --msg "Photos synced" \
  || termstate emit error --app backup --msg "rsync failed"

termstate emit blocked --kind permission --msg "Approve deploy to production?"
termstate emit clear                      # remove every record

termstate detect                          # exit 0 if the terminal supports it
some-program | termstate decode           # one JSON line per sequence
some-program | termstate decode --records # the records a terminal would keep
some-program | termstate strip            # output without OSC 7501
```

- Text is made safe by default (control characters become spaces, long text
  is truncated); `--strict` refuses it instead.
- `--tty` writes to `/dev/tty`, for hooks whose standard output is captured.
- `--tmux` wraps the sequence for tmux passthrough
  (`set -g allow-passthrough on`).

Part of [termstate](https://github.com/linyiru/termstate). MIT or Apache-2.0.
