//! `termstate`: OSC 7501 (Program Status Protocol) from the shell.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use termstate_core::limits::{MAX_MSG_DECODED, MAX_TITLE_DECODED};
use termstate_core::scan::{Event, Scanner};
use termstate_core::{Id, Kind, Report, State, Terminator, sanitize, write_clear};
use termstate_store::Store;

mod json;
#[cfg(unix)]
mod tty;

#[derive(Parser)]
#[command(name = "termstate", version, about, long_about = None)]
/// Report program status to the terminal with OSC 7501, check whether the
/// terminal supports it, or decode and strip it from captured output.
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write one status report.
    Emit(EmitArgs),
    /// Ask the terminal whether it supports OSC 7501.
    ///
    /// Exits 0 if supported, 1 if not, 2 on error (for example, no tty).
    Detect {
        /// How long to wait for the terminal's replies, in milliseconds.
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u64,
        /// Print the result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print every OSC 7501 sequence in a stream as a JSON line.
    Decode {
        /// Input file; standard input if omitted.
        file: Option<PathBuf>,
        /// Instead of one line per sequence, apply them to a record set and
        /// print the final records.
        #[arg(long)]
        records: bool,
    },
    /// Copy a stream to standard output with OSC 7501 sequences removed.
    Strip {
        /// Input file; standard input if omitted.
        file: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum StateArg {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
    /// Remove the record (and everything beneath it), or every record with no --id.
    Clear,
}

#[derive(Clone, Copy, ValueEnum)]
enum KindArg {
    Permission,
    Question,
    Auth,
}

#[derive(clap::Args)]
struct EmitArgs {
    /// The state to report.
    state: StateArg,
    /// Record id, such as `build/test`; omit for the root record.
    #[arg(long)]
    id: Option<String>,
    /// What a blocked program needs (only with `blocked`).
    #[arg(long, value_enum)]
    kind: Option<KindArg>,
    /// Percentage 0-100 (only with `working` or `blocked`).
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    progress: Option<u8>,
    /// Stable program identifier, such as `cargo` or `claude-code`.
    #[arg(long)]
    app: Option<String>,
    /// Short label for the record.
    #[arg(long)]
    title: Option<String>,
    /// One line of status text.
    #[arg(long)]
    msg: Option<String>,
    /// Refuse text with control characters or over the limit instead of
    /// replacing them with spaces and truncating.
    #[arg(long)]
    strict: bool,
    /// End the sequence with BEL instead of ESC \.
    #[arg(long)]
    bel: bool,
    /// Write to the controlling terminal (/dev/tty) instead of standard
    /// output, for hooks whose output is captured.
    #[arg(long)]
    tty: bool,
    /// Wrap in tmux's DCS passthrough so it reaches the outer terminal
    /// (needs `set -g allow-passthrough on`).
    #[arg(long)]
    tmux: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Emit(args) => emit(args).map(|()| ExitCode::SUCCESS),
        Command::Detect { timeout_ms, json } => detect(timeout_ms, json),
        Command::Decode { file, records } => decode(file, records).map(|()| ExitCode::SUCCESS),
        Command::Strip { file } => strip(file).map(|()| ExitCode::SUCCESS),
    };
    result.unwrap_or_else(|e| {
        eprintln!("termstate: {e}");
        ExitCode::from(2)
    })
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn emit(a: EmitArgs) -> Result<()> {
    let term = if a.bel {
        Terminator::Bel
    } else {
        Terminator::St
    };
    let mut seq = String::new();
    let state = match a.state {
        StateArg::Idle => State::Idle,
        StateArg::Working => State::Working,
        StateArg::Done => State::Done,
        StateArg::Blocked => State::Blocked,
        StateArg::Error => State::Error,
        StateArg::Clear => {
            let id = a.id.as_deref().map(Id::new).transpose()?;
            write_clear(&mut seq, id, term)?;
            return write_out(&seq, a.tty, a.tmux);
        }
    };
    let clean = |s: &Option<String>, max| match s {
        Some(s) if !a.strict => Some(sanitize(s, max)),
        other => other.clone(),
    };
    let (title, msg) = (
        clean(&a.title, MAX_TITLE_DECODED),
        clean(&a.msg, MAX_MSG_DECODED),
    );

    let mut b = Report::builder(state);
    if let Some(id) = &a.id {
        b = b.id(id);
    }
    if let Some(k) = a.kind {
        b = b.kind(match k {
            KindArg::Permission => Kind::Permission,
            KindArg::Question => Kind::Question,
            KindArg::Auth => Kind::Auth,
        });
    }
    if let Some(p) = a.progress {
        b = b.progress(p);
    }
    if let Some(app) = &a.app {
        b = b.app(app);
    }
    if let Some(t) = &title {
        b = b.title(t);
    }
    if let Some(m) = &msg {
        b = b.msg(m);
    }
    b.build()?.write(&mut seq, term)?;
    write_out(&seq, a.tty, a.tmux)
}

fn write_out(seq: &str, tty: bool, tmux: bool) -> Result<()> {
    let bytes = if tmux {
        format!("\x1bPtmux;{}\x1b\\", seq.replace('\x1b', "\x1b\x1b"))
    } else {
        seq.to_owned()
    };
    if tty {
        let mut f = File::options().write(true).open("/dev/tty")?;
        f.write_all(bytes.as_bytes())?;
        f.flush()?;
    } else {
        let mut out = io::stdout().lock();
        out.write_all(bytes.as_bytes())?;
        out.flush()?;
    }
    Ok(())
}

#[cfg(unix)]
fn detect(timeout_ms: u64, as_json: bool) -> Result<ExitCode> {
    use termstate_core::detect::Outcome;
    let outcome = tty::detect(std::time::Duration::from_millis(timeout_ms))?;
    let supported = matches!(outcome, Outcome::Supported(_));
    if as_json {
        println!("{}", json::outcome(&outcome));
    } else {
        match outcome {
            Outcome::Supported(c) => {
                let states: Vec<_> = c.states().map(State::as_str).collect();
                let kinds: Vec<_> = c.kinds().map(Kind::as_str).collect();
                println!(
                    "supported states={} kinds={}",
                    states.join(","),
                    kinds.join(",")
                );
            }
            Outcome::Unsupported => println!("unsupported"),
        }
    }
    Ok(if supported {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

#[cfg(not(unix))]
fn detect(_: u64, _: bool) -> Result<ExitCode> {
    Err("detect needs a Unix terminal".into())
}

fn input(file: Option<PathBuf>) -> Result<Box<dyn Read>> {
    Ok(match file {
        Some(p) => Box::new(File::open(p)?),
        None => Box::new(io::stdin().lock()),
    })
}

/// Feeds `r` through a scanner in chunks.
fn scan(mut r: Box<dyn Read>, mut on: impl FnMut(Event<'_>) -> io::Result<()>) -> Result<()> {
    let mut scanner = Scanner::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut err = Ok(());
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        scanner.feed(&buf[..n], |e| {
            if err.is_ok() {
                err = on(e);
            }
        });
        std::mem::replace(&mut err, Ok(()))?;
    }
    scanner.finish(|e| {
        if err.is_ok() {
            err = on(e);
        }
    });
    Ok(err?)
}

fn decode(file: Option<PathBuf>, records: bool) -> Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    let mut store = Store::new();
    scan(input(file)?, |e| match e {
        Event::Passthrough(_) => Ok(()),
        Event::Sequence {
            message,
            terminator,
        } => {
            if records {
                if let Ok(m) = &message {
                    store.apply(m);
                }
                Ok(())
            } else {
                writeln!(out, "{}", json::sequence(&message, terminator))
            }
        }
    })?;
    if records {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&json::records(&store))?
        )?;
    }
    out.flush()?;
    Ok(())
}

fn strip(file: Option<PathBuf>) -> Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    scan(input(file)?, |e| match e {
        Event::Passthrough(b) => out.write_all(b),
        Event::Sequence { .. } => Ok(()),
    })?;
    out.flush()?;
    Ok(())
}
