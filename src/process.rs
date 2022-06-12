#[derive(Debug)] // TODO: Remove debug
pub enum WatchResult {
    Exited(Option<i32>),
    OutputChunk(Vec<u8>),
}

mod process_watcher {
    use super::WatchResult;

    #[derive(Default)]
    pub struct Data {
        pub output: super::super::output::Output,
        pub exit_code: Option<Option<i32>>,
    }

    pub type WatchedData = super::super::watch::WatchedData<Data>;

    pub struct Watcher {
        data: WatchedData,
        pos: super::super::output::OutputPos,
    }

    impl Watcher {
        pub(super) fn new(data: WatchedData) -> Self {
            Self {
                data,
                pos: Default::default(),
            }
        }

        pub async fn read(&mut self) -> WatchResult {
            loop {
                let result = self
                    .data
                    .read(|data| match data.output.try_read(self.pos) {
                        None => data.exit_code.map(WatchResult::Exited),
                        Some((next_pos, chunk)) => {
                            self.pos = next_pos;
                            Some(WatchResult::OutputChunk(chunk.to_owned()))
                        }
                    })
                    .await;

                if let Some(result) = result {
                    return result;
                }

                self.data.wait_for_change().await;
            }
        }
    }
}

pub use process_watcher::Watcher;

pub struct Process {
    command: String,
    data: process_watcher::WatchedData,
}

impl Process {
    pub fn new(command: String) -> Process {
        Process {
            command,
            data: Default::default(),
        }
    }

    pub fn run(&self) -> tokio::task::JoinHandle<Result<Option<i32>, String>> {
        let mut child = match shell_execute(&self.command) {
            Err(_) => {
                return tokio::spawn(async { Err("Could not spawn process".to_owned()) });
            }
            Ok(child) => child,
        };

        let collect_stdout = self.spawn_proxy(child.stdout.take());
        let collect_stderr = self.spawn_proxy(child.stderr.take());

        {
            let data = self.data.clone();
            tokio::spawn(async move {
                let _ = collect_stdout.await;
                let _ = collect_stderr.await;
    
                match child.wait().await {
                    Err(_) => Err("Could not wait for process".to_owned()),
                    Ok(exit_status) => {
                        let code = exit_status.code();
                        data.read_modify(move |data| {
                            data.exit_code = Some(code);
                        }).await;
                        Ok(code)
                    },
                }
            })
        }
    }

    pub fn watch(&self) -> Watcher {
        Watcher::new(self.data.clone())
    }

    fn spawn_proxy<
        Input: tokio::io::AsyncReadExt + std::marker::Unpin + std::marker::Send + 'static,
    >(
        &self,
        input: Option<Input>,
    ) -> tokio::task::JoinHandle<()> {
        match input {
            None => tokio::spawn(async {}),
            Some(mut input) => {
                let data = self.data.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4024];
                    loop {
                        let bytes_read = match input.read(&mut buf).await {
                            Ok(0) => break,
                            Err(_) => break,
                            Ok(n) => n,
                        };

                        let chunk = &buf[..bytes_read];

                        let _ = data
                            .read_modify(move |data| {
                                data.output.append(chunk);
                            })
                            .await;
                    }
                })
            }
        }
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
