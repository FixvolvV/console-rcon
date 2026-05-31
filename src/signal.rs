//! =============================================================================
//! signal.rs — Обработка Unix-сигналов (SIGTERM, SIGINT)
//! =============================================================================
//!
//! Этот модуль отвечает за корректную обработку сигналов завершения.
//!
//! В Docker-контейнере wrapper работает как PID 1. Когда Docker хочет
//! остановить контейнер, он отправляет SIGTERM процессу с PID 1.
//! Wrapper должен:
//! 1. Перехватить сигнал
//! 2. Корректно завершить дочерний процесс SCPSL
//! 3. Закрыть WebSocket-соединение
//! 4. Выйти с правильным exit code
//!
//! Также обрабатываем SIGINT (Ctrl+C) для удобства локальной разработки.
//!
//! =============================================================================

use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::oneshot;
use tracing::{info, warn};

// =============================================================================
// SHUTDOWN SIGNAL
// =============================================================================

/// Ожидает сигнал завершения (SIGTERM или SIGINT).
///
/// Эта функция блокируется (асинхронно) пока не придёт один из сигналов.
/// После получения сигнала возвращает управление, чтобы вызывающий код
/// мог начать graceful shutdown.
///
/// # Пример
/// ```rust
/// // В отдельной задаче:
/// wait_for_shutdown_signal().await;
/// println!("Получен сигнал завершения!");
/// // Начинаем shutdown...
/// ```
pub async fn wait_for_shutdown_signal() {
    // signal() создаёт future, который завершится при получении сигнала.
    // SignalKind::terminate() — это SIGTERM на Unix.
    // .expect() паникует если не удалось зарегистрировать обработчик.
    let mut sigterm = signal(SignalKind::terminate())
        .expect("Не удалось зарегистрировать обработчик SIGTERM");

    // SignalKind::interrupt() — это SIGINT (Ctrl+C).
    let mut sigint = signal(SignalKind::interrupt())
        .expect("Не удалось зарегистрировать обработчик SIGINT");

    // tokio::select! — ждёт завершения одного из futures и выполняет
    // соответствующую ветку. Остальные futures отменяются.
    tokio::select! {
        // .recv() ждёт получения сигнала. Возвращает Some(()) при получении.
        _ = sigterm.recv() => {
            info!("Получен SIGTERM, начинаю graceful shutdown");
        }
        _ = sigint.recv() => {
            info!("Получен SIGINT (Ctrl+C), начинаю graceful shutdown");
        }
    }
}

// =============================================================================
// SHUTDOWN COORDINATOR
// =============================================================================

/// Координатор graceful shutdown.
///
/// Этот struct управляет процессом завершения работы:
/// - Предоставляет способ уведомить все задачи о shutdown
/// - Позволяет задачам подписаться на уведомление о shutdown
///
/// Использует oneshot-канал — канал для одноразовой отправки значения.
/// После отправки канал закрывается, и все получатели узнают об этом.
pub struct ShutdownCoordinator {
    /// Sender для отправки сигнала shutdown.
    /// Option потому что мы можем отправить только один раз.
    sender: Option<oneshot::Sender<()>>,
}

impl ShutdownCoordinator {
    /// Создаёт новую пару координатор-получатель.
    ///
    /// # Возвращает
    /// Кортеж из:
    /// - ShutdownCoordinator — для вызова shutdown
    /// - ShutdownReceiver — для ожидания shutdown (можно клонировать)
    pub fn new() -> (Self, ShutdownReceiver) {
        // oneshot::channel() создаёт пару sender/receiver.
        // Через sender можно отправить ровно одно значение.
        // Receiver получит его или узнает, что sender дропнут.
        let (tx, rx) = oneshot::channel();

        let coordinator = Self { sender: Some(tx) };
        let receiver = ShutdownReceiver { receiver: Some(rx) };

        (coordinator, receiver)
    }

    /// Отправляет сигнал shutdown всем получателям.
    ///
    /// Можно вызвать только один раз. Повторные вызовы игнорируются.
    pub fn shutdown(&mut self) {
        // take() извлекает значение из Option, оставляя None.
        // Это гарантирует, что мы не отправим сигнал дважды.
        if let Some(sender) = self.sender.take() {
            // send() отправляет значение. Игнорируем результат —
            // если receiver уже дропнут, нам всё равно.
            let _ = sender.send(());
        } else {
            warn!("Shutdown уже был вызван");
        }
    }
}

/// Получатель сигнала shutdown.
///
/// Каждая задача, которая должна реагировать на shutdown, получает
/// свой экземпляр ShutdownReceiver.
///
/// ВАЖНО: oneshot::Receiver нельзя клонировать напрямую.
/// Если нужно несколько получателей — используйте tokio::sync::broadcast
/// или создавайте несколько oneshot-каналов.
pub struct ShutdownReceiver {
    receiver: Option<oneshot::Receiver<()>>,
}

impl ShutdownReceiver {
    /// Ожидает сигнала shutdown.
    ///
    /// Эта функция асинхронно блокируется пока не будет вызван
    /// ShutdownCoordinator::shutdown() или пока координатор не будет дропнут.
    ///
    /// # Пример
    /// ```rust
    /// tokio::select! {
    ///     _ = shutdown_rx.wait() => {
    ///         println!("Получен shutdown, завершаюсь");
    ///         break;
    ///     }
    ///     msg = socket.recv() => {
    ///         // Обрабатываем сообщение
    ///     }
    /// }
    /// ```
    pub async fn wait(&mut self) {
        // take() извлекает receiver из Option.
        // Если уже извлечён — возвращаемся сразу.
        if let Some(rx) = self.receiver.take() {
            // await ждёт получения значения или закрытия канала.
            // Игнорируем результат — нам важен сам факт получения.
            let _ = rx.await;
        }
    }

    /// Проверяет, был ли уже получен сигнал shutdown.
    ///
    /// Неблокирующая проверка.
    pub fn is_shutdown(&self) -> bool {
        // Если receiver уже None — значит, мы уже обработали shutdown
        self.receiver.is_none()
    }
}

// =============================================================================
// ТЕСТЫ
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_coordinator() {
        let (mut coordinator, mut receiver) = ShutdownCoordinator::new();

        // Проверяем, что shutdown ещё не получен
        assert!(!receiver.is_shutdown());

        // Отправляем shutdown в отдельной задаче
        tokio::spawn(async move {
            // Небольшая задержка
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            coordinator.shutdown();
        });

        // Ждём shutdown
        receiver.wait().await;

        // Теперь должен быть shutdown
        assert!(receiver.is_shutdown());
    }
}
