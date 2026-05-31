//! =============================================================================
//! config.rs — Конфигурация приложения
//! =============================================================================
//!
//! Парсинг CLI-аргументов и переменных окружения.
//! CLI имеет приоритет над ENV.
//!
//! =============================================================================

use clap::Parser;

/// CLI-аргументы приложения.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "console-rcon",
    version,
    about = "WebSocket-based RCON wrapper for game servers"
)]
pub struct CliArgs {
    /// Путь к бинарному файлу сервера
    #[arg(
        long = "server-bin",
        env = "SERVER_BIN_PATH",
        default_value = "/root/game/LocalAdmin",
        help = "Path to server executable"
    )]
    pub server_bin: String,

    /// Порт сервера (передаётся как аргумент)
    #[arg(
        long = "port",
        short = 'p',
        env = "SERVER_PORT",
        default_value = "7777",
        help = "Server port"
    )]
    pub port: u16,

    /// URL WebSocket API
    #[arg(
        long = "api-url",
        env = "RCON_API_URL",
        default_value = "ws://host.docker.internal:8000/server/rcon/connect",
        help = "WebSocket URL for API connection"
    )]
    pub api_url: String,

    /// Имя сервера
    #[arg(
        long = "server-name",
        env = "RCON_SERVER_NAME",
        default_value = "server1",
        help = "Unique server name"
    )]
    pub server_name: String,

    /// Секретный ключ для аутентификации
    #[arg(
        long = "secret-key",
        env = "RCON_SECRET_KEY",
        help = "Secret key for authentication (required)"
    )]
    pub secret_key: String,

    /// Тип сервера
    #[arg(
        long = "server-type",
        env = "RCON_SERVER_TYPE",
        default_value = "SCPSL",
        help = "Server type identifier"
    )]
    pub server_type: String,

    /// Интервал переподключения (секунды)
    #[arg(
        long = "reconnect-secs",
        env = "RCON_RECONNECT_SECS",
        default_value = "5",
        help = "Reconnect interval in seconds"
    )]
    pub reconnect_secs: u64,

    /// Удалять ANSI escape-коды
    #[arg(
        long = "strip-ansi",
        env = "RCON_STRIP_ANSI",
        default_value = "true",
        help = "Strip ANSI escape codes from output"
    )]
    pub strip_ansi: bool,

    /// Размер буфера сообщений
    #[arg(
        long = "buffer-size",
        env = "RCON_BUFFER_SIZE",
        default_value = "10000",
        help = "Message buffer size"
    )]
    pub buffer_size: usize,
}

/// Финальная конфигурация приложения.
#[derive(Debug, Clone)]
pub struct Config {
    pub server_bin: String,
    pub port: u16,
    pub api_url: String,
    pub server_name: String,
    pub secret_key: String,
    pub server_type: String,
    pub reconnect_secs: u64,
    pub strip_ansi: bool,
    pub buffer_size: usize,
}

impl Config {
    /// Загружает конфигурацию из CLI + ENV.
    pub fn load() -> Self {
        let args = CliArgs::parse();

        Self {
            server_bin: args.server_bin,
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

    /// Строит WebSocket URL с query-параметром server_name.
    pub fn build_ws_url(&self) -> Result<String, url::ParseError> {
        let mut url = url::Url::parse(&self.api_url)?;
        url.query_pairs_mut()
            .append_pair("server_name", &self.server_name);
        Ok(url.to_string())
    }
}
