#![cfg(feature = "cli")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn cli(temp: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jartender"));
    command.current_dir(temp.path());
    for key in [
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "NO_PROXY",
        "no_proxy",
    ] {
        command.env_remove(key);
    }
    command
}

fn recording_proxy() -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("proxy was not used: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            assert!(request.len() < 64 * 1024);
        }
        stream
            .write_all(
                b"HTTP/1.1 502 Fixture proxy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        String::from_utf8(request).unwrap()
    });
    (url, worker)
}

#[test]
fn global_proxy_overrides_saved_and_environment_policy_for_all_network_commands() {
    for (args, host) in [
        (vec!["versions"], "piston-meta.mojang.com"),
        (vec!["versions", "fabric"], "meta.fabricmc.net"),
        (
            vec!["install", "1.21.4", "fabric", "latest", "demo"],
            "meta.fabricmc.net",
        ),
        (vec!["java", "versions", "openjdk"], "jdk.java.net"),
        (
            vec!["java", "install", "21", "openjdk", "--dry-run"],
            "jdk.java.net",
        ),
        (vec!["java", "version", "21"], "www.oracle.com"),
    ] {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("config.json"), r#"{"no_proxy":true}"#).unwrap();
        let (url, worker) = recording_proxy();
        let output = cli(&temp)
            .args(&args)
            .args(["--proxy", &url])
            .env("ALL_PROXY", "http://127.0.0.1:1")
            .env("NO_PROXY", "*")
            .output()
            .unwrap();
        assert!(!output.status.success(), "fixture should stop the download");
        let request = worker.join().unwrap();
        assert!(
            request.starts_with(&format!("CONNECT {host}:443 ")),
            "{args:?}: {request}"
        );
        assert!(!temp.path().join("Servers/demo").exists());
    }
}

#[test]
fn saved_proxy_and_environment_fallback_are_used() {
    for saved in [true, false] {
        let temp = TempDir::new().unwrap();
        let (url, worker) = recording_proxy();
        let mut command = cli(&temp);
        if saved {
            std::fs::write(
                temp.path().join("config.json"),
                serde_json::json!({"proxy":url}).to_string(),
            )
            .unwrap();
            command
                .env("ALL_PROXY", "http://127.0.0.1:1")
                .env("NO_PROXY", "*");
        } else {
            command.env("HTTPS_PROXY", url);
        }
        assert!(!command.arg("versions").output().unwrap().status.success());
        assert!(
            worker
                .join()
                .unwrap()
                .starts_with("CONNECT piston-meta.mojang.com:443 ")
        );
    }
}

fn failure(output: Output, expected: &str) {
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains(expected), "{error}");
}

#[test]
fn invalid_settings_conflicts_and_direct_override_fail_without_network() {
    let temp = TempDir::new().unwrap();
    for config in ["{broken", r#"{"proxy":42}"#, r#"{"no_proxy":"yes"}"#] {
        std::fs::write(temp.path().join("config.json"), config).unwrap();
        failure(
            cli(&temp).args(["versions"]).output().unwrap(),
            "cannot read",
        );
    }
    std::fs::write(temp.path().join("config.json"), r#"{"proxy":"not-a-url"}"#).unwrap();
    failure(cli(&temp).args(["versions"]).output().unwrap(), "proxy");
    failure(
        cli(&temp)
            .args(["java", "versions", "oracle", "0", "--no-proxy"])
            .env("ALL_PROXY", "http://127.0.0.1:1")
            .output()
            .unwrap(),
        "Java major",
    );
    failure(
        cli(&temp)
            .args(["versions", "--proxy", "http://localhost:7890", "--no-proxy"])
            .output()
            .unwrap(),
        "cannot be used with",
    );
    std::fs::write(
        temp.path().join("config.json"),
        r#"{"proxy":"http://localhost:7890","no_proxy":true}"#,
    )
    .unwrap();
    failure(
        cli(&temp).args(["versions"]).output().unwrap(),
        "mutually exclusive",
    );
    // Local commands remain usable while the download configuration needs fixing.
    std::fs::create_dir(temp.path().join("Servers")).unwrap();
    assert!(
        cli(&temp)
            .args(["java", "types"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(cli(&temp).args(["list"]).output().unwrap().status.success());
}
