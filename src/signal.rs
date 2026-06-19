use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::oneshot;
use tracing::{info, warn};

pub async fn wait_for_shutdown_signal() {
    let mut sigterm =
        signal(SignalKind::terminate()).expect("Не удалось зарегистрировать обработчик SIGTERM");

    let mut sigint =
        signal(SignalKind::interrupt()).expect("Не удалось зарегистрировать обработчик SIGINT");

    tokio::select! {

        _ = sigterm.recv() => {
            info!("Получен SIGTERM, начинаю graceful shutdown");
        }
        _ = sigint.recv() => {
            info!("Получен SIGINT (Ctrl+C), начинаю graceful shutdown");
        }
    }
}

pub struct ShutdownCoordinator {
    sender: Option<oneshot::Sender<()>>,
}

impl ShutdownCoordinator {
    pub fn new() -> (Self, ShutdownReceiver) {
        let (tx, rx) = oneshot::channel();

        let coordinator = Self { sender: Some(tx) };
        let receiver = ShutdownReceiver { receiver: Some(rx) };

        (coordinator, receiver)
    }

    pub fn shutdown(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(());
        } else {
            warn!("Shutdown уже был вызван");
        }
    }
}

pub struct ShutdownReceiver {
    receiver: Option<oneshot::Receiver<()>>,
}

impl ShutdownReceiver {
    pub async fn wait(&mut self) {
        if let Some(rx) = self.receiver.take() {
            let _ = rx.await;
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.receiver.is_none()
    }
}
