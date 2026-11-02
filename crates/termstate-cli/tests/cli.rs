//! Runs the built `termstate` binary.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_termstate"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(args: &[&str]) -> String {
    let out = run(args, b"");
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn emit_report() {
    assert_eq!(
        stdout(&[
            "emit",
            "blocked",
            "--kind",
            "permission",
            "--app",
            "terraform",
            "--msg",
            "Apply 3 to add, 1 to change, 0 to destroy?"
        ]),
        "\x1b]7501;state=blocked:kind=permission:app=terraform:msg=QXBwbHkgMyB0byBhZGQsIDEgdG8gY2hhbmdlLCAwIHRvIGRlc3Ryb3k/\x1b\\"
    );
    assert_eq!(
        stdout(&[
            "emit",
            "working",
            "--id",
            "build/test",
            "--progress",
            "40",
            "--bel"
        ]),
        "\x1b]7501;state=working:id=build/test:progress=40\x07"
    );
}

#[test]
fn emit_clear() {
    assert_eq!(stdout(&["emit", "clear"]), "\x1b]7501;state=clear\x1b\\");
    assert_eq!(
        stdout(&["emit", "clear", "--id", "us-east"]),
        "\x1b]7501;state=clear:id=us-east\x1b\\"
    );
}

#[test]
fn emit_sanitizes_by_default_and_refuses_with_strict() {
    // "a b" in base64 is YSBi.
    assert_eq!(
        stdout(&["emit", "done", "--msg", "a\nb"]),
        "\x1b]7501;state=done:msg=YSBi\x1b\\"
    );
    let out = run(&["emit", "done", "--strict", "--msg", "a\nb"], b"");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("control character"));
}

#[test]
fn emit_rejects_bad_input() {
    for args in [
        &["emit", "working", "--progress", "101"][..],
        &["emit", "idle", "--app", "has space"],
        &["emit", "idle", "--id", "a//b"],
        &["emit", "paused"],
    ] {
        assert!(!run(args, b"").status.success(), "{args:?}");
    }
}

#[test]
fn emit_tmux_passthrough() {
    assert_eq!(
        stdout(&["emit", "idle", "--tmux"]),
        "\x1bPtmux;\x1b\x1b]7501;state=idle\x1b\x1b\\\x1b\\"
    );
}

const STREAM: &[u8] = b"build output\n\x1b]7501;state=working:app=cargo\x1b\\more\x1b]7501;state=paused\x07\x1b]7501;state=done:id=t:msg=SGk=\x07end\n";

#[test]
fn decode_lines() {
    let out = run(&["decode"], STREAM);
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["sequence"]["report"]["app"], "cargo");
    assert_eq!(lines[1]["discard"], true);
    assert_eq!(lines[1]["reason"], "unrecognized state");
    assert_eq!(lines[2]["sequence"]["report"]["msg"], "Hi");
    assert_eq!(lines[2]["terminator"], "bel");
}

#[test]
fn decode_records() {
    let out = run(&["decode", "--records"], STREAM);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v,
        serde_json::json!([
            { "id": null, "state": "working", "app": "cargo", "effective_app": "cargo" },
            { "id": "t", "state": "done", "msg": "Hi", "effective_app": "cargo" }
        ])
    );
}

#[test]
fn strip_removes_only_osc7501() {
    let out = run(
        &["strip"],
        b"a\x1b]0;title\x07b\x1b]7501;state=idle\x1b\\c\x1b[1md",
    );
    assert_eq!(out.stdout, b"a\x1b]0;title\x07bc\x1b[1md");
}

#[test]
fn emit_then_decode_round_trip() {
    let seq = stdout(&[
        "emit",
        "blocked",
        "--kind",
        "question",
        "--title",
        "Q",
        "--msg",
        "完成？",
    ]);
    let out = run(&["decode"], seq.as_bytes());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["sequence"]["report"]["msg"], "完成？");
    assert_eq!(v["sequence"]["report"]["kind"], "question");
}

/// Runs `termstate detect` on a pseudo-terminal made by `script`, writing
/// `replies` as the terminal's answers after the query has been sent.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn detect_on_pty(replies: &'static [u8], extra: &[&str]) -> Output {
    let bin = env!("CARGO_BIN_EXE_termstate");
    let mut cmd = Command::new("script");
    if cfg!(target_os = "macos") {
        cmd.args(["-q", "/dev/null", bin, "detect"]).args(extra);
    } else {
        let line =
            std::iter::once(format!("{bin} detect")).chain(extra.iter().map(|s| s.to_string()));
        cmd.args(["-qec", &line.collect::<Vec<_>>().join(" "), "/dev/null"]);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = stdin.write_all(replies);
    });
    child.wait_with_output().unwrap()
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
// spec: FD-DA1
fn detect_supported_on_a_pty() {
    let out = detect_on_pty(
        b"\x1b]7501;?:states=idle,done:kinds=auth\x1b\\\x1b[?62;22c",
        &["--json"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains(r#"{"kinds":["auth"],"states":["idle","done"],"supported":true}"#)
    );
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
// spec: FD-DA1
fn detect_unsupported_on_a_pty() {
    let out = detect_on_pty(b"\x1b[?62;22c", &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("unsupported"));
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn detect_times_out_as_unsupported() {
    let out = detect_on_pty(b"", &["--timeout-ms", "200"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn decode_reads_a_file_and_shows_clear_and_feature() {
    let dir = std::env::temp_dir().join(format!("termstate-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("stream");
    std::fs::write(
        &path,
        b"\x1b]7501;state=clear\x07\x1b]7501;state=clear:id=a\x07\x1b]7501;?\x07",
    )
    .unwrap();
    let text = stdout(&["decode", path.to_str().unwrap()]);
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["sequence"], serde_json::json!({ "clear": {} }));
    assert_eq!(
        lines[1]["sequence"],
        serde_json::json!({ "clear": { "id": "a" } })
    );
    assert_eq!(
        lines[2]["sequence"]["feature"]["kinds"],
        serde_json::json!(["permission", "question", "auth"])
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn every_kind_and_missing_file() {
    for k in ["permission", "question", "auth"] {
        assert!(stdout(&["emit", "blocked", "--kind", k]).contains(&format!("kind={k}")));
    }
    assert_eq!(
        run(&["decode", "/nonexistent/termstate"], b"")
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run(&["strip", "/nonexistent/termstate"], b"").status.code(),
        Some(2)
    );
}
