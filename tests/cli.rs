use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_chan-downloader"))
}

#[test]
fn it_prints_version() {
    let output = bin().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("chan-downloader {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn it_prints_help() {
    let output = bin().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for flag in [
        "--thread",
        "--output",
        "--reload",
        "--interval",
        "--limit",
        "--concurrent",
    ] {
        assert!(stdout.contains(flag), "missing {flag} in help:\n{stdout}");
    }
    assert!(!stdout.contains("--verbose"), "--verbose should be hidden");
}

#[test]
fn it_fails_without_thread() {
    let output = bin().output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--thread <URL>"));
}
