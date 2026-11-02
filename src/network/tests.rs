use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Instant;

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("proxy accept: {e}"),
        }
    }
}

fn headers(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() < 64 * 1024);
    }
    String::from_utf8(bytes).unwrap()
}

/// A proxy fixture that serves HTTP through CONNECT without contacting any origin.
pub(crate) fn serve_proxy(bodies: Vec<Vec<u8>>) -> (ProxyConfig, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = ProxyConfig::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        for body in bodies {
            let mut stream = accept(&listener);
            let connect = headers(&mut stream);
            assert!(connect.starts_with("CONNECT "), "{connect}");
            stream
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .unwrap();
            let request = headers(&mut stream);
            requests.push(format!("{connect}{request}"));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
        requests
    });
    (proxy, worker)
}

#[test]
fn explicit_proxy_tunnels_metadata_and_artifacts_without_resolving_origins() {
    let (proxy, worker) = serve_proxy(vec![br#"{"version":"21"}"#.to_vec(), b"jar bytes".to_vec()]);
    let agent = proxy.agent(Duration::from_secs(5));
    let metadata: serde_json::Value =
        crate::install::metadata(&agent, "http://metadata.invalid/version").unwrap();
    assert_eq!(metadata["version"], "21");
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("server.jar");
    crate::install::download(
        &agent,
        "http://artifacts.invalid/server.jar",
        &file,
        100,
        &mut |_| std::ops::ControlFlow::Continue(()),
    )
    .unwrap();
    assert_eq!(std::fs::read(file).unwrap(), b"jar bytes");
    let requests = worker.join().unwrap();
    assert!(requests[0].starts_with("CONNECT metadata.invalid:80 "));
    assert!(requests[1].starts_with("CONNECT artifacts.invalid:80 "));
}

#[test]
fn socks5h_sends_origin_hostname_to_proxy() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = ProxyConfig::new(&format!("socks5h://{}", listener.local_addr().unwrap())).unwrap();
    let worker = thread::spawn(move || {
        let mut stream = accept(&listener);
        let mut greeting = [0; 2];
        stream.read_exact(&mut greeting).unwrap();
        assert_eq!(greeting[0], 5);
        let mut methods = vec![0; greeting[1] as usize];
        stream.read_exact(&mut methods).unwrap();
        assert!(methods.contains(&0));
        stream.write_all(&[5, 0]).unwrap();
        let mut connect = [0; 5];
        stream.read_exact(&mut connect).unwrap();
        assert_eq!(&connect[..4], &[5, 1, 0, 3]);
        let mut host = vec![0; connect[4] as usize];
        stream.read_exact(&mut host).unwrap();
        assert_eq!(host, b"origin.invalid");
        let mut port = [0; 2];
        stream.read_exact(&mut port).unwrap();
        assert_eq!(u16::from_be_bytes(port), 80);
        stream
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
            .unwrap();
        assert!(headers(&mut stream).starts_with("GET /test "));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
    });
    assert_eq!(
        crate::install::response_bytes(
            &proxy.agent(Duration::from_secs(5)),
            "http://origin.invalid/test"
        )
        .unwrap(),
        b"ok"
    );
    worker.join().unwrap();
}

#[test]
fn proxy_validation_and_debug_do_not_expose_credentials() {
    for url in [
        "",
        "127.0.0.1:7890",
        "ftp://localhost:21",
        "http://localhost:0",
        "http://localhost:65536",
        "http://:7890",
        "http://localhost/path",
        "http://localhost?query",
        "http://localhost:7890/#fragment",
    ] {
        assert!(ProxyConfig::new(url).is_err(), "{url}");
    }
    let proxy = ProxyConfig::new("http://alice:secret@localhost:7890").unwrap();
    let debug = format!("{proxy:?}");
    assert!(!debug.contains("alice") && !debug.contains("secret"));
    let error = ProxyConfig::new("bad://alice:secret@localhost")
        .unwrap_err()
        .to_string();
    assert!(!error.contains("alice") && !error.contains("secret"));
}

#[test]
fn java_installers_receive_http_socks_and_direct_policies() {
    let http = ProxyConfig::new("http://localhost:7890")
        .unwrap()
        .agent(Duration::from_secs(5));
    let args = installer_proxy_args(&http).unwrap();
    for arg in [
        "-Dhttp.proxyHost=localhost",
        "-Dhttp.proxyPort=7890",
        "-Dhttps.proxyHost=localhost",
        "-Dhttps.proxyPort=7890",
        "-Dhttp.nonProxyHosts=",
    ] {
        assert!(args.iter().any(|value| value == arg), "{args:?}");
    }
    let socks = ProxyConfig::new("socks5h://localhost:1080")
        .unwrap()
        .agent(Duration::from_secs(5));
    assert!(
        installer_proxy_args(&socks)
            .unwrap()
            .iter()
            .any(|arg| arg == "-DsocksProxyHost=localhost")
    );
    let direct = ProxyConfig::Direct.agent(Duration::from_secs(5));
    assert!(direct.config().proxy().is_none());
    assert!(
        installer_proxy_args(&direct)
            .unwrap()
            .iter()
            .any(|arg| arg == "-Dhttp.proxyHost=")
    );
    for url in [
        "https://localhost:7890",
        "http://alice:secret@localhost:7890",
    ] {
        let proxy = ProxyConfig::new(url).unwrap().agent(Duration::from_secs(5));
        assert!(installer_proxy_args(&proxy).is_err());
    }
}
