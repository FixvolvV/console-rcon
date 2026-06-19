mod config;
mod messages;
mod process;
mod signal;
mod websocket;

use config::Config;
use messages::OutgoingMessage;
use signal::ShutdownCoordinator;
use tokio::sync::mpsc;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    let config = Config::load();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Не удалось установить tracing subscriber");

    info!(
        "=== Console RCON Wrapper v{} ===",
        env!("CARGO_PKG_VERSION")
    );
    info!("Server name: {}", config.server_name);
    info!("Server type: {}", config.server_type);
    info!("Server binary: {}", config.server_bin);
    info!("Server port: {}", config.port);
    info!("API URL: {}", config.api_url);
    info!(
        "Reconnect interval: {} сек (с прогрессивным backoff до 60 сек)",
        config.reconnect_secs
    );
    info!("Strip ANSI: {}", config.strip_ansi);
    info!("Buffer size: {}", config.buffer_size);
    info!("Secret key: [HIDDEN]");

    let (outgoing_tx, outgoing_rx) = mpsc::channel::<OutgoingMessage>(config.buffer_size);

    let (incoming_tx, incoming_rx) = mpsc::channel::<String>(100);

    let (mut shutdown_coordinator, shutdown_rx) = ShutdownCoordinator::new();

    info!("Запускаю сервер...");

    let mut child = match process::spawn_server(&config).await {
        Ok(child) => child,
        Err(e) => {
            error!("Не удалось запустить сервер: {}", e);
            error!(
                "Проверьте, что файл {} существует и исполняемый",
                config.server_bin
            );
            std::process::exit(1);
        }
    };

    let stdin = child.stdin.take().expect("stdin должен быть piped");
    let stdout = child.stdout.take().expect("stdout должен быть piped");
    let stderr = child.stderr.take().expect("stderr должен быть piped");

    // --- Задача чтения stdout ---
    let outgoing_tx_stdout = outgoing_tx.clone();
    let server_name_stdout = config.server_name.clone();
    let strip_ansi = config.strip_ansi;

    let stdout_handle = tokio::spawn(async move {
        process::read_stdio(stdout, outgoing_tx_stdout, server_name_stdout, strip_ansi).await;
        info!("Задача stdout reader завершена");
    });

    // --- Задача чтения stderr ---
    let outgoing_tx_stderr = outgoing_tx.clone();
    let server_name_stderr = config.server_name.clone();

    let stderr_handle = tokio::spawn(async move {
        process::read_stdio(stderr, outgoing_tx_stderr, server_name_stderr, strip_ansi).await;
        info!("Задача stderr reader завершена");
    });

    // --- Задача записи в stdin ---
    let stdin_handle = tokio::spawn(async move {
        websocket::stdin_writer_task(stdin, incoming_rx).await;
        info!("Задача stdin writer завершена");
    });

    drop(outgoing_tx);

    let mut ws_client = websocket::WebSocketClient::new(config.clone(), outgoing_rx, incoming_tx);

    let ws_handle = tokio::spawn(async move {
        ws_client.run(shutdown_rx).await;
        info!("Задача WebSocket клиента завершена");
    });

    // --- Задача обработки сигналов ---
    let signal_handle = tokio::spawn(async move {
        signal::wait_for_shutdown_signal().await;
        shutdown_coordinator.shutdown();
    });

    let exit_code = tokio::select! {
        // Ждём завершения процесса сервера
        status = child.wait() => {
            match status {
                Ok(exit_status) => {
                    let code = exit_status.code().unwrap_or(1);
                    if exit_status.success() {
                        info!("Сервер завершился успешно (код {})", code);
                    } else {
                        warn!("Сервер завершился с ошибкой (код {})", code);
                    }
                    code
                }
                Err(e) => {
                    error!("Ошибка ожидания завершения сервера: {}", e);
                    1
                }
            }
        }

        // Ждём завершения задачи обработки сигналов
        _ = signal_handle => {
            info!("Получен сигнал завершения, останавливаю сервер...");
            let code = process::terminate_child(&mut child, 10).await.unwrap_or(0);
            code
        }
    };

    info!("Завершаю работу wrapper'а...");

    let cleanup_timeout = tokio::time::Duration::from_secs(5);

    stdout_handle.abort();
    stderr_handle.abort();
    stdin_handle.abort();
    ws_handle.abort();

    tokio::time::sleep(cleanup_timeout).await;

    info!("Wrapper завершён с кодом {}", exit_code);

    std::process::exit(exit_code);
}
