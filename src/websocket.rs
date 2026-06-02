//! =============================================================================
//! websocket.rs — WebSocket клиент для связи с FastAPI сервером
//! =============================================================================
//!
//! Этот модуль отвечает за:
//! 1. Подключение к WebSocket API
//! 2. Отправку auth-сообщения при подключении
//! 3. Стриминг stdout-сообщений из канала на API
//! 4. Получение stdin-команд от API и отправку в канал команд
//! 5. Автоматический реконнект при разрыве соединения с прогрессивной задержкой
//!
//! Логика реконнекта:
//! - При любой ошибке соединения — пытаемся подключиться снова
//! - Задержка прогрессивно растёт: 5 → 10 → 20 → 40 → 60 → 60 сек
//! - При успешном соединении — задержка сбрасывается до базовой
//! - JSON-ошибки парсинга НЕ убивают соединение (просто игнорируем сообщение)
//! - AuthDenied — фатальная ошибка, wrapper завершается
//!
//! =============================================================================

use crate::config::Config;
use crate::messages::{IncomingMessage, OutgoingMessage};
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

// Специальная строка-маркер для AuthDenied — обрабатывается в run() особым образом
const AUTH_DENIED_MARKER: &str = "AUTH_DENIED";

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
/// 5. При разрыве — переподключается с прогрессивной задержкой
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
    /// Логика:
    /// - Базовая задержка из конфига (reconnect_secs)
    /// - При ошибке задержка растёт вдвое до max_delay (60 сек)
    /// - При успешном соединении сбрасывается до базовой
    /// - AuthDenied останавливает wrapper целиком (exit code 2)
    pub async fn run(&mut self, mut shutdown_rx: crate::signal::ShutdownReceiver) {
        let base_delay = self.config.reconnect_secs;
        let max_delay: u64 = 60;
        let mut current_delay = base_delay;

        loop {
            // Проверяем, не пришёл ли shutdown
            if shutdown_rx.is_shutdown() {
                info!("WebSocket клиент: получен shutdown, завершаюсь");
                break;
            }

            // Пытаемся подключиться и работать
            match self.connect_and_run().await {
                Ok(()) => {
                    info!("WebSocket соединение закрыто штатно, переподключаюсь...");
                    // Сбрасываем задержку — соединение было успешным
                    current_delay = base_delay;
                }
                Err(WsError::ConnectionFailed(ref msg)) if msg == AUTH_DENIED_MARKER => {
                    // Фатальная ошибка авторизации — нет смысла переподключаться
                    error!("Авторизация отклонена сервером. Wrapper завершается.");
                    std::process::exit(2);
                }
                Err(e) => {
                    warn!(
                        "WebSocket ошибка: {}. Жду {} сек до переподключения...",
                        e, current_delay
                    );
                }
            }

            // Ждём перед реконнектом, но прерываемся если пришёл shutdown
            let reconnect_delay = tokio::time::Duration::from_secs(current_delay);

            tokio::select! {
                _ = tokio::time::sleep(reconnect_delay) => {
                    // Прогрессивно увеличиваем задержку: 5 → 10 → 20 → 40 → 60 → 60
                    current_delay = (current_delay * 2).min(max_delay);
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
        let (ws_stream, response) = connect_async(&url)
            .await
            .map_err(|e| WsError::ConnectionFailed(e.to_string()))?;

        info!("WebSocket подключён, статус: {}", response.status());

        // split() разделяет stream на read-half и write-half.
        let (mut write, mut read) = ws_stream.split();

        // Шаг 2: Основной цикл обработки сообщений
        loop {
            tokio::select! {
                // biased — проверяем futures в порядке перечисления.
                // Это предотвращает starvation при высокой нагрузке.
                biased;

                // Получаем сообщение из канала для отправки на API
                outgoing = self.outgoing_rx.recv() => {
                    match outgoing {
                        Some(msg) => {
                            let json = serde_json::to_string(&msg).map_err(|e| {
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
                incoming = read.next() => {
                    match incoming {
                        Some(Ok(msg)) => {
                            self.handle_incoming_message(msg).await?;
                        }
                        Some(Err(e)) => {
                            return Err(WsError::ReceiveFailed(e.to_string()));
                        }
                        None => {
                            return Err(WsError::ConnectionClosed);
                        }
                    }
                }
            }
        }
    }

    async fn handle_incoming_message(&self, msg: Message) -> WsResult<()> {
        match msg {
            Message::Text(text) => {
                debug!("Получено от API: {}", text);

                // JSON-ошибка НЕ убивает соединение, просто игнорируем сообщение
                let parsed: IncomingMessage = match serde_json::from_str(&text) {
                    Ok(msg) => msg,
                    Err(e) => {
                        warn!(
                            "Не удалось распарсить сообщение от API: {} (raw: {})",
                            e, text
                        );
                        return Ok(()); // продолжаем слушать
                    }
                };

                match parsed {
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
                }
            }
            Message::Close(frame) => {
                info!("Сервер закрыл соединение: {:?}", frame);
                return Err(WsError::ConnectionClosed);
            }
            _ => {}
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
pub async fn stdin_writer_task(
    mut stdin: crate::process::StdinWriter,
    mut command_rx: mpsc::Receiver<String>,
) {
    info!("Stdin writer запущен");

    while let Some(command) = command_rx.recv().await {
        if let Err(e) = crate::process::write_stdin(&mut stdin, &command).await {
            error!("Ошибка записи в stdin: {}", e);
            break;
        }
    }

    info!("Stdin writer завершён");
}
