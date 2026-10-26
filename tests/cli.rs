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

mod thread_errors {
    use super::bin;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    /// Runs the downloader on a thread served by `response`, in a temporary
    /// working directory. Returns the exit code, stderr and whether the thread
    /// directory was left behind.
    async fn run(response: ResponseTemplate) -> (Option<i32>, String, bool) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(response)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();

        let output = bin()
            .args(["-t", &format!("{}/wg/thread/1", server.uri())])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let thread_dir = dir.path().join("downloads").join("wg").join("1");
        (
            output.status.code(),
            String::from_utf8(output.stderr).unwrap(),
            thread_dir.exists(),
        )
    }

    // The server is driven by the test runtime while the binary runs
    #[tokio::test(flavor = "multi_thread")]
    async fn it_fails_when_blocked() {
        let (code, stderr, left_dir) =
            run(ResponseTemplate::new(403).set_body_string("<title>Please wait</title>")).await;
        assert_eq!(code, Some(1));
        assert!(stderr.contains("403 Forbidden"), "{stderr}");
        assert!(!left_dir, "empty thread directory left behind");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn it_fails_when_no_file_can_be_downloaded() {
        // Links to files that don't exist on 4chan's CDN (fails offline too)
        let page = r#"<a href="//i.4cdn.org/wg/0.jpg">a</a> <a href="//i.4cdn.org/wg/1.jpg">b</a>"#;
        let (code, stderr, left_dir) = run(ResponseTemplate::new(200).set_body_string(page)).await;
        assert_eq!(code, Some(1));
        assert!(
            stderr.contains("could not download any of the 2 files"),
            "{stderr}"
        );
        assert!(!left_dir, "empty thread directory left behind");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn it_keeps_going_when_one_of_several_threads_fails() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let first = format!("{}/wg/thread/1", server.uri());
        let second = format!("{}/p/thread/2", server.uri());

        let output = bin()
            .args(["-t", &first, &second])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(stderr.contains(&format!("failed to load {first}")), "{stderr}");
        assert!(stderr.contains(&format!("failed to load {second}")), "{stderr}");
        assert!(stderr.contains("2 of 2 threads failed"), "{stderr}");
        assert!(!dir.path().join("downloads/wg/1").exists());
        assert!(!dir.path().join("downloads/p/2").exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn it_fails_when_no_images_are_found() {
        let (code, stderr, left_dir) =
            run(ResponseTemplate::new(200).set_body_string("<html>no files here</html>")).await;
        assert_eq!(code, Some(1));
        assert!(stderr.contains("no images found"), "{stderr}");
        assert!(!left_dir, "empty thread directory left behind");
    }
}
