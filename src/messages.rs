//! =============================================================================
//! messages.rs — Определения структур сообщений для WebSocket-протокола
//! =============================================================================
//!
//! Этот модуль содержит все структуры данных, которые мы отправляем и получаем
//! через WebSocket-соединение с FastAPI сервером.
//!
//! В Rust мы используем serde для автоматической сериализации (Rust → JSON)
//! и десериализации (JSON → Rust). Атрибут #[derive(Serialize, Deserialize)]
//! генерирует весь необходимый код автоматически.
//!
//! =============================================================================

// Импортируем макросы derive из serde для автогенерации кода сериализации
use serde::{Deserialize, Serialize};

// =============================================================================
// ИСХОДЯЩИЕ СООБЩЕНИЯ (Wrapper → API)
// =============================================================================

/// Сообщение аутентификации — отправляется первым после подключения к WebSocket.
///
/// API использует это сообщение чтобы:
/// 1. Проверить secret_key и убедиться, что это легитимный wrapper
/// 2. Зарегистрировать сервер с именем `server` в системе
/// 3. Знать тип сервера (SCPSL, Minecraft и т.д.) для правильной обработки
///
/// # Пример JSON
/// ```json
/// {
///   "action": "auth",
///   "server": "server1",
///   "server_type": "SCPSL",
///   "secret_key": "my-secret-key"
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct AuthMessage {
    /// Тип сообщения — всегда "auth" для этой структуры.
    /// Используем "action" вместо "type", так как API ожидает это поле.
    #[serde(rename = "action")]
    pub action: String,

    /// Имя сервера (например, "server1", "lobby", "event-server")
    pub server: String,

    /// Тип сервера (например, "SCPSL", "Minecraft")
    pub server_type: String,

    /// Секретный ключ для аутентификации
    pub secret_key: String,
}

impl AuthMessage {
    /// Создаёт новое auth-сообщение с заданными параметрами.
    ///
    /// # Аргументы
    /// * `server` - Имя сервера
    /// * `server_type` - Тип сервера
    /// * `secret_key` - Секретный ключ
    pub fn new(server: &str, server_type: &str, secret_key: &str) -> Self {
        Self {
            action: "auth".to_string(),
            server: server.to_string(),
            server_type: server_type.to_string(),
            secret_key: secret_key.to_string(),
        }
    }
}

/// Сообщение с выводом консоли — отправляется при каждой строке из stdout/stderr.
///
/// Wrapper читает stdout/stderr процесса SCPSL построчно и отправляет
/// каждую строку в таком формате на API.
///
/// # Пример JSON
/// ```json
/// {
///   "type": "stdout",
///   "server": "server1",
///   "content": "Player John connected from 192.168.1.1"
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct StdoutMessage {
    /// Тип сообщения — всегда "stdout"
    #[serde(rename = "type")]
    pub msg_type: String,

    /// Имя сервера
    pub server: String,

    /// Содержимое строки из консоли
    pub content: String,
}

impl StdoutMessage {
    /// Создаёт новое stdout-сообщение.
    ///
    /// # Аргументы
    /// * `server` - Имя сервера
    /// * `content` - Строка из консоли SCPSL
    pub fn new(server: &str, content: String) -> Self {
        Self {
            msg_type: "stdout".to_string(),
            server: server.to_string(),
            content,
        }
    }
}

// =============================================================================
// ВХОДЯЩИЕ СООБЩЕНИЯ (API → Wrapper)
// =============================================================================

/// Входящее сообщение от API — может быть разных типов.
///
/// API использует поле "action" для идентификации типа сообщения.
///
/// # Примеры JSON
/// ```json
/// {"action": "auth_success", "server": "server1"}
/// {"action": "stdin", "server": "server1", "content": "reload"}
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action")]
pub enum IncomingMessage {
    /// Подтверждение успешной авторизации
    #[serde(rename = "auth_success")]
    AuthSuccess { server: String },

    /// Команда для stdin — записать content в stdin процесса
    #[serde(rename = "stdin")]
    Stdin { server: String, content: String },

    /// Любое другое сообщение — игнорируем
    #[serde(other)]
    Unknown,
}

// =============================================================================
// ВСПОМОГАТЕЛЬНЫЕ ТИПЫ
// =============================================================================

/// Обёртка для исходящих сообщений в очереди.
///
/// Мы используем enum чтобы в одном канале (mpsc) передавать разные типы
/// сообщений: и auth, и stdout. Это удобнее, чем делать два отдельных канала.
#[derive(Debug, Clone)]
pub enum OutgoingMessage {
    /// Сообщение аутентификации
    Auth(AuthMessage),
    /// Сообщение с выводом консоли
    Stdout(StdoutMessage),
}

impl OutgoingMessage {
    /// Сериализует сообщение в JSON-строку.
    ///
    /// # Возвращает
    /// * `Ok(String)` — JSON-строка
    /// * `Err(serde_json::Error)` — ошибка сериализации (маловероятно)
    ///
    /// # Пример
    /// ```rust
    /// let msg = OutgoingMessage::Stdout(StdoutMessage::new("s1", "hello".into()));
    /// let json = msg.to_json().unwrap();
    /// // json = r#"{"type":"stdout","server":"s1","content":"hello"}"#
    /// ```
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        // match — основной способ работы с enum в Rust.
        // Компилятор гарантирует, что мы обработали все варианты.
        match self {
            OutgoingMessage::Auth(msg) => serde_json::to_string(msg),
            OutgoingMessage::Stdout(msg) => serde_json::to_string(msg),
        }
    }
}

// =============================================================================
// ТЕСТЫ
// =============================================================================

#[cfg(test)] // Этот модуль компилируется только при запуске тестов
mod tests {
    use super::*; // Импортируем всё из родительского модуля

    /// Тест сериализации AuthMessage
    #[test]
    fn test_auth_message_serialization() {
        let msg = AuthMessage::new("server1", "SCPSL", "secret");
        let json = serde_json::to_string(&msg).unwrap();

        // Проверяем, что JSON содержит нужные поля
        assert!(json.contains(r#""type":"auth""#));
        assert!(json.contains(r#""server":"server1""#));
        assert!(json.contains(r#""server_type":"SCPSL""#));
        assert!(json.contains(r#""secret_key":"secret""#));
    }

    /// Тест сериализации StdoutMessage
    #[test]
    fn test_stdout_message_serialization() {
        let msg = StdoutMessage::new("server1", "Hello World".to_string());
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains(r#""type":"stdout""#));
        assert!(json.contains(r#""server":"server1""#));
        assert!(json.contains(r#""content":"Hello World""#));
    }

    /// Тест десериализации auth_success
    #[test]
    fn test_auth_success_deserialization() {
        let json = r#"{"action":"auth_success","server":"server1"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();

        match msg {
            IncomingMessage::AuthSuccess { server } => {
                assert_eq!(server, "server1");
            }
            _ => panic!("Expected AuthSuccess message"),
        }
    }

    /// Тест десериализации stdin-сообщения
    #[test]
    fn test_stdin_message_deserialization() {
        let json = r#"{"action":"stdin","server":"server1","content":"reload"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();

        match msg {
            IncomingMessage::Stdin { server, content } => {
                assert_eq!(server, "server1");
                assert_eq!(content, "reload");
            }
            _ => panic!("Expected Stdin message"),
        }
    }

    /// Тест десериализации неизвестного сообщения
    #[test]
    fn test_unknown_message_deserialization() {
        let json = r#"{"action":"some_random_action","data":"whatever"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();

        assert!(matches!(msg, IncomingMessage::Unknown));
    }
}
