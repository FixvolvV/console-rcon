//! =============================================================================
//! config.rs — Конфигурация приложения
//! =============================================================================
//!
//! Этот модуль отвечает за:
//! 1. Определение всех параметров конфигурации
//! 2. Парсинг CLI-аргументов с помощью clap
//! 3. Чтение переменных окружения
//! 4. Объединение CLI и ENV (CLI имеет приоритет)
//!
//! Clap с derive-макросами позволяет описать CLI декларативно через структуру.
//! Атрибут #[arg(env = "...")] автоматически читает значение из ENV, если
//! аргумент не передан через командную строку.
//!
//! =============================================================================

use clap::Parser;

// =============================================================================
// CLI АРГУМЕНТЫ
// =============================================================================

/// Структура CLI-аргументов.
///
/// Parser — derive-макрос из clap, который генерирует парсер командной строки.
/// Каждое поле становится аргументом или опцией.
///
/// # Пример использования
/// ```bash
/// scpsl-wrapper --scpsl-bin /path/to/LocalAdmin --port 7778
/// ```
#[derive(Parser, Debug, Clone)]
#[command(
    name = "scpsl-wrapper",
    author = "Your Name",
    version = "0.1.0",
    about = "WebSocket-based RCON wrapper for SCP:SL server",
    // long_about — расширенное описание для --help
    long_about = "
SCPSL-Wrapper — это supervisor-обёртка для SCP: Secret Laboratory сервера.

Wrapper запускает LocalAdmin как дочерний процесс, перехватывает его stdout/stderr,
и стримит вывод через WebSocket на FastAPI-сервер. Также принимает команды от API
и передаёт их в stdin процесса (RCON).

Используется как PID 1 в Docker-контейнере."
)]
pub struct CliArgs {
    /// Путь к бинарному файлу SCPSL (LocalAdmin).
    ///
    /// #[arg(...)] — атрибут, настраивающий аргумент:
    /// - long: --scpsl-bin
    /// - env: читать из SCPSL_BIN_PATH если не передан
    /// - default_value: значение по умолчанию
    #[arg(
        long = "scpsl-bin",
        env = "SCPSL_BIN_PATH",
        default_value = "/root/game/LocalAdmin",
        help = "Путь к исполняемому файлу LocalAdmin"
    )]
    pub scpsl_bin: String,

    /// Порт SCPSL сервера.
    ///
    /// Этот порт передаётся как аргумент в LocalAdmin.
    #[arg(
        long = "port",
        short = 'p',
        env = "SCPSL_PORT",
        default_value = "7777",
        help = "Порт игрового сервера SCPSL"
    )]
    pub port: u16,

    /// URL WebSocket API для RCON-подключения.
    #[arg(
        long = "api-url",
        env = "WRAPPER_API_URL",
        default_value = "ws://host.docker.internal:8000/server/rcon/connect",
        help = "WebSocket URL для подключения к API"
    )]
    pub api_url: String,

    /// Имя сервера (используется в сообщениях и как query-параметр).
    #[arg(
        long = "server-name",
        env = "WRAPPER_SERVER_NAME",
        default_value = "server1",
        help = "Уникальное имя этого сервера"
    )]
    pub server_name: String,

    /// Секретный ключ для аутентификации на API.
    ///
    /// Обязательный параметр — нет default_value.
    /// Если не передан ни через CLI, ни через ENV — приложение не запустится.
    #[arg(
        long = "secret-key",
        env = "WRAPPER_SECRET_KEY",
        help = "Секретный ключ для аутентификации (обязателен)"
    )]
    pub secret_key: String,

    /// Тип сервера (передаётся в auth-сообщении).
    #[arg(
        long = "server-type",
        env = "WRAPPER_SERVER_TYPE",
        default_value = "SCPSL",
        help = "Тип сервера для идентификации на API"
    )]
    pub server_type: String,

    /// Интервал переподключения к WebSocket в секундах.
    #[arg(
        long = "reconnect-secs",
        env = "WRAPPER_RECONNECT_SECS",
        default_value = "5",
        help = "Интервал между попытками переподключения (секунды)"
    )]
    pub reconnect_secs: u64,

    /// Вырезать ANSI escape-коды из вывода.
    ///
    /// ANSI-коды используются для цветного вывода в терминале.
    /// Если true — они будут удалены перед отправкой на API.
    #[arg(
        long = "strip-ansi",
        env = "WRAPPER_STRIP_ANSI",
        default_value = "true",
        help = "Удалять ANSI escape-коды из вывода"
    )]
    pub strip_ansi: bool,

    /// Размер буфера для исходящих сообщений.
    ///
    /// Если WebSocket отключён, сообщения копятся в буфере.
    /// При переполнении старые сообщения отбрасываются.
    #[arg(
        long = "buffer-size",
        env = "WRAPPER_BUFFER_SIZE",
        default_value = "10000",
        help = "Размер буфера исходящих сообщений"
    )]
    pub buffer_size: usize,

    /// Уровень логирования.
    ///
    /// Возможные значения: trace, debug, info, warn, error
    #[arg(
        long = "log-level",
        env = "RUST_LOG",
        default_value = "info",
        help = "Уровень логирования (trace/debug/info/warn/error)"
    )]
    pub log_level: String,
}

// =============================================================================
// КОНФИГ ПРИЛОЖЕНИЯ
// =============================================================================

/// Финальная конфигурация приложения.
///
/// Эта структура создаётся из CliArgs и содержит все нужные параметры
/// в удобном для использования виде.
///
/// Clone — позволяет клонировать конфиг (нужно для передачи в разные задачи).
/// Debug — позволяет выводить конфиг в логи.
#[derive(Debug, Clone)]
pub struct Config {
    /// Путь к бинарю LocalAdmin
    pub scpsl_bin: String,
    /// Порт сервера
    pub port: u16,
    /// WebSocket URL API
    pub api_url: String,
    /// Имя сервера
    pub server_name: String,
    /// Секретный ключ
    pub secret_key: String,
    /// Тип сервера
    pub server_type: String,
    /// Интервал реконнекта в секундах
    pub reconnect_secs: u64,
    /// Удалять ANSI-коды
    pub strip_ansi: bool,
    /// Размер буфера сообщений
    pub buffer_size: usize,
}

impl Config {
    /// Загружает конфигурацию из CLI-аргументов и переменных окружения.
    ///
    /// Clap автоматически:
    /// 1. Парсит аргументы командной строки
    /// 2. Для отсутствующих аргументов проверяет ENV
    /// 3. Для отсутствующих в ENV — использует default_value
    ///
    /// # Паникует
    /// Если обязательный аргумент (secret_key) не передан ни через CLI, ни через ENV.
    ///
    /// # Пример
    /// ```rust
    /// let config = Config::load();
    /// println!("Server: {}", config.server_name);
    /// ```
    pub fn load() -> Self {
        // Parser::parse() читает std::env::args() и парсит аргументы.
        // Если что-то не так (неизвестный аргумент, отсутствует обязательный) —
        // программа завершится с ошибкой и справкой.
        let args = CliArgs::parse();

        // Преобразуем CliArgs в Config
        Self {
            scpsl_bin: args.scpsl_bin,
            port: args.port,
            api_url: args.api_url,
            server_name: args.server_name,
            secret_key: args.secret_key,
            server_type: args.server_type,
            reconnect_secs: args.reconnect_secs,
            strip_ansi: args.strip_ansi,
            buffer_size: args.buffer_size,
        }
    }

    /// Строит полный URL для WebSocket-подключения с query-параметром server_name.
    ///
    /// # Возвращает
    /// URL в формате: ws://host:port/path?server_name=server1
    ///
    /// # Ошибки
    /// Возвращает Err если api_url невалидный.
    pub fn build_ws_url(&self) -> Result<String, url::ParseError> {
        // url::Url::parse() парсит строку в структуру URL
        let mut url = url::Url::parse(&self.api_url)?;

        // query_pairs_mut() даёт мутабельный доступ к query-параметрам
        // append_pair() добавляет пару ключ=значение
        url.query_pairs_mut()
            .append_pair("server_name", &self.server_name);

        // Ok(...) — возвращаем успешный результат
        Ok(url.to_string())
    }
}

// =============================================================================
// ТЕСТЫ
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ws_url() {
        let config = Config {
            scpsl_bin: "/root/game/LocalAdmin".to_string(),
            port: 7777,
            api_url: "ws://localhost:8000/rcon/connect".to_string(),
            server_name: "test-server".to_string(),
            secret_key: "secret".to_string(),
            server_type: "SCPSL".to_string(),
            reconnect_secs: 5,
            strip_ansi: true,
            buffer_size: 1000,
        };

        let url = config.build_ws_url().unwrap();
        assert_eq!(url, "ws://localhost:8000/rcon/connect?server_name=test-server");
    }

    #[test]
    fn test_build_ws_url_with_existing_params() {
        let config = Config {
            scpsl_bin: "/root/game/LocalAdmin".to_string(),
            port: 7777,
            api_url: "ws://localhost:8000/rcon?foo=bar".to_string(),
            server_name: "srv1".to_string(),
            secret_key: "secret".to_string(),
            server_type: "SCPSL".to_string(),
            reconnect_secs: 5,
            strip_ansi: true,
            buffer_size: 1000,
        };

        let url = config.build_ws_url().unwrap();
        // Должен добавить server_name к существующим параметрам
        assert!(url.contains("foo=bar"));
        assert!(url.contains("server_name=srv1"));
    }
}
