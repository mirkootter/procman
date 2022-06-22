use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
use std::net::SocketAddr;

mod output;
mod process;
mod watch;

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

fn make_error_response(status: hyper::StatusCode) -> Result<Response<Body>, Infallible> {
    let response = Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap();
    Ok(response)
}

mod server {
    use super::{IgnoreError, process};

    #[derive(Clone, Copy, Eq, PartialEq, Hash)]
    pub struct ProcessID(u64);
    
    #[derive(Default)]
    pub struct Server {
        processes: std::collections::HashMap<ProcessID, process::Process>,
        counter: std::sync::atomic::AtomicU64
    }

    impl Server {
        fn alloc_pid(&mut self) -> ProcessID {
            let counter = self.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            ProcessID(counter)
        }

        fn spawn_shell(&mut self, cmd: String, mut response: hyper::body::Sender) {
            let pid = self.alloc_pid();
            let process = process::Process::new(cmd);
            tokio::spawn(process.run());

            let mut watcher = process.watch();
            self.processes.insert(pid, process);

            tokio::spawn(async move {
                loop {
                    match watcher.read().await {
                        process::WatchResult::Exited(Some(exit_code)) => {
                            let message = format!("\n\nExited with exit code {}", exit_code);
                            response.send_data(hyper::body::Bytes::from(message)).await?;
                            break;
                        }
                        process::WatchResult::Exited(None) => {
                            response
                                .send_data(hyper::body::Bytes::from_static(
                                    b"\n\nProcess exited without exit code",
                                ))
                                .await?;
                            break;
                        }
                        process::WatchResult::OutputChunk(chunk) => {
                            response.send_data(hyper::body::Bytes::from(chunk)).await?;
                        }
                    }
                }
    
                Ok::<(), IgnoreError>(())
            });
        }
    }

    impl Server {
        pub async fn global_spawn_shell(cmd: String, response: hyper::body::Sender) {
            let mut server = GLOBAL_SERVER.lock().await;
            server.spawn_shell(cmd, response)
        }
    }

    lazy_static::lazy_static! {
        static ref GLOBAL_SERVER: tokio::sync::Mutex<Server> = tokio::sync::Mutex::new(Server::default());
    }
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

    {
        let (sender, body) = Body::channel();
        server::Server::global_spawn_shell(cmd.to_owned(), sender).await;

        let response = Response::builder()
            .header("Content-Type", "text/event-stream; charset=utf-8")
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
