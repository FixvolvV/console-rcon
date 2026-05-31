//! =============================================================================
//! messages.rs — Определения структур сообщений для WebSocket-протокола
//! =============================================================================
//!
//! Этот модуль содержит все структуры данных, которые мы отправляем и получаем
//! через WebSocket-соединение с FastAPI сервером.
//!
//! ПРОТОКОЛ:
//! Все сообщения (и исходящие, и входящие) используют поле "type" для
//! идентификации типа сообщения. Это упрощает парсинг и единообразит формат.
//!
//! Возможные типы:
//! - Исходящие: "auth", "stdout"
//! - Входящие:  "auth_access", "auth_denied", "stdin"
//!
//! =============================================================================

use serde::{Deserialize, Serialize};

// =============================================================================
// ИСХОДЯЩИЕ СООБЩЕНИЯ (Wrapper → API)
// =============================================================================

/// Сообщение аутентификации — отправляется первым после подключения к WebSocket.
///
/// # Пример JSON
/// ```json
/// {
///   "type": "auth",
///   "server": "server1",
///   "server_type": "SCPSL",
///   "secret_key": "my-secret-key"
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct AuthMessage {
    /// Тип сообщения — всегда "auth"
    #[serde(rename = "type")]
    pub msg_type: String,

    /// Имя сервера
    pub server: String,

    /// Тип сервера (SCPSL, Minecraft и т.д.)
    pub server_type: String,

    /// Секретный ключ для аутентификации
    pub secret_key: String,
}

impl AuthMessage {
    /// Создаёт новое auth-сообщение с заданными параметрами.
    pub fn new(server: &str, server_type: &str, secret_key: &str) -> Self {
        Self {
            msg_type: "auth".to_string(),
            server: server.to_string(),
            server_type: server_type.to_string(),
            secret_key: secret_key.to_string(),
        }
    }
}

/// Сообщение с выводом консоли — отправляется при каждой строке из stdout/stderr.
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

/// Входящее сообщение от API.
///
/// Все варианты различаются по полю "type" в JSON.
/// Атрибут `#[serde(tag = "type")]` говорит serde:
/// "смотри на поле type и выбирай соответствующий вариант enum".
///
/// # Примеры JSON
/// ```json
/// {"type": "auth_access", "server": "server1"}
/// {"type": "auth_denied", "server": "server1", "reason": "wrong key"}
/// {"type": "stdin", "server": "server1", "content": "reload"}
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IncomingMessage {
    /// Подтверждение успешной авторизации.
    /// Имя варианта `AuthAccess` сериализуется как `auth_access`
    /// благодаря `rename_all = "snake_case"`.
    AuthAccess { server: String },

    /// Отказ в авторизации.
    /// Поле `reason` опционально (может прийти, а может нет).
    AuthDenied {
        server: String,
        #[serde(default)]
        reason: Option<String>,
    },

    /// Команда для stdin — записать `content` в stdin процесса.
    Stdin { server: String, content: String },

    /// Любое другое сообщение — игнорируем.
    /// Атрибут `#[serde(other)]` ловит всё, что не подошло ни под один вариант.
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
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        match self {
            OutgoingMessage::Auth(msg) => serde_json::to_string(msg),
            OutgoingMessage::Stdout(msg) => serde_json::to_string(msg),
        }
    }
}

// =============================================================================
// ТЕСТЫ
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_message_serialization() {
        let msg = AuthMessage::new("server1", "SCPSL", "secret");
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains(r#""type":"auth""#));
        assert!(json.contains(r#""server":"server1""#));
        assert!(json.contains(r#""server_type":"SCPSL""#));
        assert!(json.contains(r#""secret_key":"secret""#));
    }

    #[test]
    fn test_stdout_message_serialization() {
        let msg = StdoutMessage::new("server1", "Hello World".to_string());
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains(r#""type":"stdout""#));
        assert!(json.contains(r#""server":"server1""#));
        assert!(json.contains(r#""content":"Hello World""#));
    }

    #[test]
    fn test_auth_access_deserialization() {
        let json = r#"{"type":"auth_access","server":"server1"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();

        match msg {
            IncomingMessage::AuthAccess { server } => {
                assert_eq!(server, "server1");
            }
            _ => panic!("Expected AuthAccess message"),
        }
    }

    #[test]
    fn test_auth_denied_deserialization() {
        let json = r#"{"type":"auth_denied","server":"server1","reason":"wrong key"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();

        match msg {
            IncomingMessage::AuthDenied { server, reason } => {
                assert_eq!(server, "server1");
                assert_eq!(reason, Some("wrong key".to_string()));
            }
            _ => panic!("Expected AuthDenied message"),
        }
    }

    #[test]
    fn test_auth_denied_without_reason() {
        // Поле reason опционально
        let json = r#"{"type":"auth_denied","server":"server1"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();

        match msg {
            IncomingMessage::AuthDenied { server, reason } => {
                assert_eq!(server, "server1");
                assert_eq!(reason, None);
            }
            _ => panic!("Expected AuthDenied message"),
        }
    }

    #[test]
    fn test_stdin_message_deserialization() {
        let json = r#"{"type":"stdin","server":"server1","content":"reload"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();

        match msg {
            IncomingMessage::Stdin { server, content } => {
                assert_eq!(server, "server1");
                assert_eq!(content, "reload");
            }
            _ => panic!("Expected Stdin message"),
        }
    }

    #[test]
    fn test_unknown_message_deserialization() {
        let json = r#"{"type":"some_random_type","data":"whatever"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();

        assert!(matches!(msg, IncomingMessage::Unknown));
    }
}
