struct Inner<Data> {
    data: Data,
    sender: tokio::sync::watch::Sender<()>,
}

pub struct WatchedData<Data> {
    inner: std::sync::Arc<tokio::sync::Mutex<Inner<Data>>>,
    receiver: tokio::sync::watch::Receiver<()>,
}

impl<Data: Send + 'static> WatchedData<Data> {
    pub fn new(data: Data) -> Self {
        let (sender, receiver) = tokio::sync::watch::channel(());
        let inner = Inner { data, sender };

        let inner = std::sync::Arc::new(tokio::sync::Mutex::new(inner));
        Self { inner, receiver }
    }

    pub async fn read_modify(&self, f: impl FnOnce(&mut Data)) {
        let mut inner = self.inner.lock().await;
        f(&mut inner.data);
        let _ = inner.sender.send(());
    }

    pub async fn read<T>(&self, f: impl FnOnce(&Data) -> T) -> T {
        let inner = self.inner.lock().await;
        f(&inner.data)
    }

    pub async fn wait_for_change(&mut self) {
        let _ = self.receiver.changed().await;
    }
}

impl<Data: Default + Send + 'static> Default for WatchedData<Data> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<Data> Clone for WatchedData<Data> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            receiver: self.receiver.clone(),
        }
    }
}
