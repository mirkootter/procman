use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
use std::net::SocketAddr;

struct IgnoreError;

impl From<std::io::Error> for IgnoreError {
    fn from(_: std::io::Error) -> IgnoreError {
        IgnoreError
    }
}

impl From<hyper::Error> for IgnoreError {
    fn from(_: hyper::Error) -> IgnoreError {
        IgnoreError
    }
}

#[cfg(windows)]
fn shell_execute(cmd: &'_ str) -> std::io::Result<tokio::process::Child> {
    tokio::process::Command::new("cmd.exe")
        .arg("/c")
        .arg(cmd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
}

#[cfg(unix)]
fn shell_execute(cmd: &'_ str) -> std::io::Result<tokio::process::Child> {
    tokio::process::Command::new("sh")
        .arg(cmd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
}

fn make_error_response(status: hyper::StatusCode) -> Result<Response<Body>, Infallible> {
    let response = Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap();
    Ok(response)
}

async fn handle_shell(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let query = match req.uri().query() {
        None => return make_error_response(hyper::StatusCode::BAD_REQUEST),
        Some(query) => query,
    };

    let params = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect::<std::collections::HashMap<String, String>>();

    let cmd: &str = match params.get("cmd") {
        None => return make_error_response(hyper::StatusCode::BAD_REQUEST),
        Some(cmd) => cmd,
    };

    let mut child = match shell_execute(cmd) {
        Err(_) => return make_error_response(hyper::StatusCode::EXPECTATION_FAILED),
        Ok(child) => child,
    };

    {
        let (mut sender, body) = Body::channel();
        tokio::spawn(async move {
            if let Some(stdout) = &mut child.stdout {
                use tokio::io::AsyncReadExt;

                let mut buf = [0u8; 1024];
                loop {
                    let bytes_read = stdout.read(&mut buf).await?;
                    if bytes_read == 0 {
                        break;
                    }

                    sender
                        .send_data(hyper::body::Bytes::copy_from_slice(&buf[..bytes_read]))
                        .await?;
                }
            }

            let exit_code = match child.wait().await {
                Err(_) => None,
                Ok(exit_status) => exit_status.code(),
            };

            match exit_code {
                None => {
                    sender
                        .send_data(hyper::body::Bytes::from_static(b"\n\nSomething went wrong"))
                        .await?;
                }
                Some(exit_code) => {
                    let message = format!("\n\nExited with exit code {}", exit_code);
                    sender.send_data(hyper::body::Bytes::from(message)).await?;
                }
            }

            Ok::<(), IgnoreError>(())
        });

        let response = Response::builder()
            .header("Content-Type", "text/event-stream")
            .status(hyper::http::StatusCode::OK)
            .body(body)
            .unwrap();
        Ok(response)
    }
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&hyper::Method::GET, "/shell") => handle_shell(req).await,
        _ => make_error_response(hyper::StatusCode::NOT_FOUND),
    }
}

#[tokio::main]
async fn main() {
    // Construct our SocketAddr to listen on...
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // And a MakeService to handle each connection...
    let make_service = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });

    // Then bind and serve...
    let server = Server::bind(&addr).serve(make_service);

    // And run forever...
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
