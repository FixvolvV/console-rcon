//! =============================================================================
//! websocket.rs — WebSocket клиент для связи с FastAPI сервером
//! =============================================================================
//!
//! Этот модуль отвечает за:
//! 1. Подключение к WebSocket API
//! 2. Отправку auth-сообщения при подключении
//! 3. Стриминг stdout-сообщений из канала на API
//! 4. Получение stdin-команд от API и отправку в канал команд
//! 5. Автоматический реконнект при разрыве соединения
//!
//! Основные концепции:
//! - tokio-tungstenite — асинхронная WebSocket библиотека
//! - Stream/Sink — абстракции для чтения/записи данных
//! - tokio::select! — одновременное ожидание нескольких futures
//!
//! =============================================================================

use crate::config::Config;
use crate::messages::{AuthMessage, IncomingMessage, OutgoingMessage};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

// =============================================================================
// ТИПЫ
// =============================================================================

/// Результат операции WebSocket.
pub type WsResult<T> = Result<T, WsError>;

/// Ошибки WebSocket-клиента.
#[derive(Debug)]
pub enum WsError {
    /// Ошибка подключения
    ConnectionFailed(String),
    /// Ошибка отправки сообщения
    SendFailed(String),
    /// Ошибка получения сообщения
    ReceiveFailed(String),
    /// Соединение закрыто
    ConnectionClosed,
    /// Ошибка сериализации JSON
    JsonError(String),
    /// Ошибка построения URL
    UrlError(String),
}

// impl Display для WsError — позволяет использовать {} в format!/println!
impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsError::ConnectionFailed(msg) => write!(f, "Ошибка подключения: {}", msg),
            WsError::SendFailed(msg) => write!(f, "Ошибка отправки: {}", msg),
            WsError::ReceiveFailed(msg) => write!(f, "Ошибка получения: {}", msg),
            WsError::ConnectionClosed => write!(f, "Соединение закрыто"),
            WsError::JsonError(msg) => write!(f, "Ошибка JSON: {}", msg),
            WsError::UrlError(msg) => write!(f, "Ошибка URL: {}", msg),
        }
    }
}

// =============================================================================
// WEBSOCKET CLIENT
// =============================================================================

/// WebSocket-клиент для связи с FastAPI сервером.
///
/// Клиент работает в бесконечном цикле:
/// 1. Подключается к серверу
/// 2. Отправляет auth
/// 3. Читает сообщения из канала и отправляет на сервер
/// 4. Получает команды от сервера и отправляет в канал
/// 5. При разрыве — переподключается
pub struct WebSocketClient {
    /// Конфигурация (URL, server_name, secret_key и т.д.)
    config: Config,
    /// Receiver канала исходящих сообщений (stdout от SCPSL)
    outgoing_rx: mpsc::Receiver<OutgoingMessage>,
    /// Sender канала входящих команд (stdin для SCPSL)
    incoming_tx: mpsc::Sender<String>,
}

impl WebSocketClient {
    /// Создаёт новый WebSocket-клиент.
    ///
    /// # Аргументы
    /// * `config` - Конфигурация приложения
    /// * `outgoing_rx` - Receiver для сообщений, которые нужно отправить на API
    /// * `incoming_tx` - Sender для команд, полученных от API
    pub fn new(
        config: Config,
        outgoing_rx: mpsc::Receiver<OutgoingMessage>,
        incoming_tx: mpsc::Sender<String>,
    ) -> Self {
        Self {
            config,
            outgoing_rx,
            incoming_tx,
        }
    }

    /// Запускает WebSocket-клиент в бесконечном цикле с реконнектом.
    ///
    /// Эта функция никогда не возвращается (если не получит сигнал shutdown).
    /// Она:
    /// 1. Пытается подключиться к серверу
    /// 2. При успехе — обрабатывает сообщения
    /// 3. При разрыве — ждёт интервал реконнекта и повторяет
    ///
    /// # Аргументы
    /// * `shutdown_rx` - Receiver для сигнала shutdown
    pub async fn run(&mut self, mut shutdown_rx: crate::signal::ShutdownReceiver) {
        loop {
            // Проверяем, не пришёл ли shutdown
            if shutdown_rx.is_shutdown() {
                info!("WebSocket клиент: получен shutdown, завершаюсь");
                break;
            }

            // Пытаемся подключиться
            match self.connect_and_run().await {
                Ok(()) => {
                    // Соединение закрыто нормально (Close frame)
                    info!("WebSocket соединение закрыто, переподключаюсь...");
                }
                Err(e) => {
                    // Ошибка соединения
                    warn!("WebSocket ошибка: {}, переподключаюсь...", e);
                }
            }

            // Ждём перед реконнектом
            let reconnect_delay = tokio::time::Duration::from_secs(self.config.reconnect_secs);

            // select! позволяет прервать ожидание если пришёл shutdown
            tokio::select! {
                _ = tokio::time::sleep(reconnect_delay) => {
                    // Таймаут истёк, продолжаем цикл
                }
                _ = shutdown_rx.wait() => {
                    info!("WebSocket клиент: получен shutdown во время ожидания реконнекта");
                    break;
                }
            }
        }
    }

    /// Подключается к серверу и обрабатывает сообщения до разрыва.
    async fn connect_and_run(&mut self) -> WsResult<()> {
        // Строим URL с query-параметром
        let url = self
            .config
            .build_ws_url()
            .map_err(|e| WsError::UrlError(e.to_string()))?;

        info!("Подключаюсь к WebSocket: {}", url);

        // connect_async() подключается к WebSocket серверу.
        // Возвращает (WebSocketStream, Response).
        // WebSocketStream реализует Stream + Sink для двунаправленного обмена.
        let (ws_stream, response) = connect_async(&url)
            .await
            .map_err(|e| WsError::ConnectionFailed(e.to_string()))?;

        info!("WebSocket подключён, статус: {}", response.status());

        // split() разделяет stream на read-half и write-half.
        // Это нужно чтобы читать и писать независимо в разных ветках select!.
        let (mut write, mut read) = ws_stream.split();

        // Шаг 1: Отправляем auth-сообщение
        let auth_msg = AuthMessage::new(
            &self.config.server_name,
            &self.config.server_type,
            &self.config.secret_key,
        );
        let auth_json =
            serde_json::to_string(&auth_msg).map_err(|e| WsError::JsonError(e.to_string()))?;

        info!("Отправляю auth-сообщение");
        debug!("Auth payload: {}", auth_json);

        // send() отправляет сообщение через WebSocket.
        // Message::Text — текстовое сообщение (JSON).
        write
            .send(Message::Text(auth_json))
            .await
            .map_err(|e| WsError::SendFailed(e.to_string()))?;

        // Шаг 2: Основной цикл обработки сообщений
        loop {
            // tokio::select! ждёт завершения одного из трёх futures:
            // 1. Новое сообщение из канала (stdout от SCPSL)
            // 2. Новое сообщение от WebSocket (команда от API)
            // 3. Ничего из вышеперечисленного (biased означает приоритет сверху вниз)
            tokio::select! {
                // biased — проверяем futures в порядке перечисления.
                // Это предотвращает starvation при высокой нагрузке.
                biased;

                // Получаем сообщение из канала для отправки на API
                // recv() возвращает Option<T> — None если все senders закрыты
                outgoing = self.outgoing_rx.recv() => {
                    match outgoing {
                        Some(msg) => {
                            // Сериализуем и отправляем
                            let json = msg.to_json().map_err(|e| {
                                WsError::JsonError(e.to_string())
                            })?;

                            debug!("Отправляю на API: {}", json);

                            write.send(Message::Text(json)).await.map_err(|e| {
                                WsError::SendFailed(e.to_string())
                            })?;
                        }
                        None => {
                            // Канал закрыт — все producers дропнуты.
                            // Это значит, что stdout/stderr readers завершились,
                            // т.е. SCPSL процесс завершился.
                            info!("Канал исходящих сообщений закрыт, завершаю WebSocket");

                            // Отправляем Close frame
                            let _ = write.send(Message::Close(None)).await;
                            return Ok(());
                        }
                    }
                }

                // Получаем сообщение от WebSocket (от API)
                // next() возвращает Option<Result<Message, Error>>
                incoming = read.next() => {
                    match incoming {
                        Some(Ok(msg)) => {
                            self.handle_incoming_message(msg).await?;
                        }
                        Some(Err(e)) => {
                            // Ошибка чтения — соединение потеряно
                            return Err(WsError::ReceiveFailed(e.to_string()));
                        }
                        None => {
                            // Stream завершился — соединение закрыто
                            return Err(WsError::ConnectionClosed);
                        }
                    }
                }
            }
        }
    }

    /// Обрабатывает входящее WebSocket-сообщение.
    async fn handle_incoming_message(&self, msg: Message) -> WsResult<()> {
        match msg {
            // Текстовое сообщение — JSON от API
            Message::Text(text) => {
                debug!("Получено от API: {}", text);

                // Десериализуем JSON в IncomingMessage
                let parsed: IncomingMessage = serde_json::from_str(&text).map_err(|e| {
                    warn!("Не удалось распарсить сообщение от API: {}", e);
                    WsError::JsonError(e.to_string())
                })?;

                match parsed {
                    IncomingMessage::AuthAccess { server } => {
                        info!("Авторизация успешна для сервера: {}", server);
                    }

                    IncomingMessage::AuthDenied { server, reason } => {
                        error!(
                            "Авторизация отклонена для {}: {}",
                            server,
                            reason.as_deref().unwrap_or("без указания причины")
                        );
                        return Err(WsError::ConnectionClosed);
                    }

                    IncomingMessage::Stdin { server, content } => {
                        // Проверяем, что server совпадает с нашим
                        if server != self.config.server_name {
                            warn!(
                                "Получена команда для другого сервера: {} (мы: {})",
                                server, self.config.server_name
                            );
                            return Ok(());
                        }

                        info!("Получена команда от API: {}", content);

                        if let Err(e) = self.incoming_tx.try_send(content) {
                            error!("Не удалось отправить команду в stdin: {}", e);
                        }
                    }

                    IncomingMessage::Unknown => {
                        debug!("Получено сообщение неизвестного типа, игнорирую");
                    }
                }
            }

            // Ping — отвечаем Pong (tungstenite делает это автоматически)
            Message::Ping(data) => {
                debug!("Получен Ping, отвечаю Pong");
                // Примечание: tokio-tungstenite автоматически отвечает на Ping
                let _ = data; // suppress unused warning
            }

            // Pong — игнорируем
            Message::Pong(_) => {
                debug!("Получен Pong");
            }

            // Close — сервер закрывает соединение
            Message::Close(frame) => {
                info!("Сервер закрыл соединение: {:?}", frame);
                return Err(WsError::ConnectionClosed);
            }

            // Binary — не ожидаем бинарных сообщений
            Message::Binary(_) => {
                debug!("Получено бинарное сообщение, игнорирую");
            }

            // Frame — низкоуровневый frame, не используем
            Message::Frame(_) => {
                debug!("Получен raw frame, игнорирую");
            }
        }

        Ok(())
    }
}

// =============================================================================
// STDIN WRITER TASK
// =============================================================================

/// Задача для записи команд в stdin процесса SCPSL.
///
/// Эта функция запускается в отдельной tokio-задаче и читает команды
/// из канала, записывая их в stdin процесса.
///
/// # Аргументы
/// * `stdin` - Мутабельный stdin процесса
/// * `command_rx` - Receiver канала команд
pub async fn stdin_writer_task(
    mut stdin: crate::process::StdinWriter,
    mut command_rx: mpsc::Receiver<String>,
) {
    info!("Stdin writer запущен");

    // Читаем команды из канала пока он не закроется
    while let Some(command) = command_rx.recv().await {
        // Записываем команду в stdin процесса
        if let Err(e) = crate::process::write_stdin(&mut stdin, &command).await {
            error!("Ошибка записи в stdin: {}", e);
            // Если stdin закрыт — процесс завершился, выходим
            break;
        }
    }

    info!("Stdin writer завершён");
}

// =============================================================================
// ТЕСТЫ
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_error_display() {
        let err = WsError::ConnectionFailed("timeout".to_string());
        assert_eq!(format!("{}", err), "Ошибка подключения: timeout");

        let err = WsError::ConnectionClosed;
        assert_eq!(format!("{}", err), "Соединение закрыто");
    }
}
