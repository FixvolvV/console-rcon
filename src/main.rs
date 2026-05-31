//! =============================================================================
//! main.rs — Точка входа приложения scpsl-wrapper
//! =============================================================================
//!
//! SCPSL-Wrapper — это supervisor-обёртка для SCP: Secret Laboratory сервера,
//! которая обеспечивает удалённый RCON через WebSocket.
//!
//! ## Архитектура
//!
//! Wrapper запускает несколько параллельных задач (tokio tasks):
//!
//! ```text
//!                    ┌─────────────────────────────────────┐
//!                    │           SCPSL Process             │
//!                    │         (LocalAdmin 7777)           │
//!                    └──────┬──────────┬──────────┬────────┘
//!                           │          │          │
//!                        stdout     stderr     stdin
//!                           │          │          ▲
//!                           ▼          ▼          │
//!                    ┌──────────┐ ┌──────────┐ ┌──────────┐
//!                    │ stdout   │ │ stderr   │ │  stdin   │
//!                    │ reader   │ │ reader   │ │  writer  │
//!                    └────┬─────┘ └────┬─────┘ └────▲─────┘
//!                         │            │            │
//!                         ▼            ▼            │
//!                    ┌────────────────────────┐     │
//!                    │   outgoing_tx (mpsc)   │     │
//!                    │   buffer: 10000 msgs   │     │
//!                    └───────────┬────────────┘     │
//!                                │                  │
//!                                ▼                  │
//!                    ┌─────────────────────────┐    │
//!                    │    WebSocket Client     │────┘
//!                    │  (connect, auth, loop)  │
//!                    └───────────┬─────────────┘
//!                                │
//!                                ▼
//!                    ┌─────────────────────────┐
//!                    │    FastAPI Server       │
//!                    │ (RCON WebSocket API)    │
//!                    └─────────────────────────┘
//! ```
//!
//! ## Жизненный цикл
//!
//! 1. Загрузка конфигурации (ENV + CLI)
//! 2. Запуск SCPSL процесса
//! 3. Запуск всех задач параллельно
//! 4. Ожидание завершения (SIGTERM/SIGINT или падение SCPSL)
//! 5. Graceful shutdown
//!
//! =============================================================================

// Подключаем наши модули.
// mod X; говорит компилятору: "в файле src/X.rs есть модуль X"
mod config;
mod messages;
mod process;
mod signal;
mod websocket;

// Импортируем нужные типы из стандартной библиотеки и наших модулей
use config::Config;
use messages::OutgoingMessage;
use signal::ShutdownCoordinator;
use tokio::sync::mpsc;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

// =============================================================================
// MAIN
// =============================================================================

/// Главная функция приложения.
///
/// #[tokio::main] — макрос, который создаёт tokio runtime и запускает
/// async main функцию внутри него. Это эквивалентно:
/// ```rust
/// fn main() {
///     tokio::runtime::Runtime::new().unwrap().block_on(async_main());
/// }
/// ```
///
/// Мы используем multi-threaded runtime (по умолчанию), который
/// распределяет задачи по нескольким потокам ОС.
#[tokio::main]
async fn main() {
    // =========================================================================
    // ШАГ 1: Загрузка конфигурации
    // =========================================================================

    // Загружаем конфиг из ENV + CLI аргументов.
    // Если обязательные параметры отсутствуют — программа завершится с ошибкой.
    let config = Config::load();

    // =========================================================================
    // ШАГ 2: Инициализация логирования
    // =========================================================================

    // Парсим уровень логирования из конфига
    let log_level = match config.server_name.as_str() {
        // Здесь мы не используем server_name для логирования,
        // это просто заглушка чтобы показать pattern matching.
        // Реальный уровень берётся из RUST_LOG env.
        _ => Level::INFO,
    };

    // FmtSubscriber выводит логи в красивом формате в stderr.
    // with_max_level ограничивает минимальный уровень логов.
    // with_env_filter читает RUST_LOG env для тонкой настройки.
    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        // with_env_filter позволяет задавать уровни для разных модулей:
        // RUST_LOG=info,scpsl_wrapper::websocket=debug
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        // with_target показывает модуль, откуда пришёл лог
        .with_target(true)
        // with_thread_ids полезно для отладки многопоточности
        .with_thread_ids(false)
        // with_file и with_line_number показывают место в коде
        .with_file(false)
        .with_line_number(false)
        // Формат времени
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .finish();

    // Устанавливаем subscriber как глобальный.
    // Все последующие вызовы tracing::info!/warn!/error! будут
    // обрабатываться этим subscriber'ом.
    tracing::subscriber::set_global_default(subscriber)
        .expect("Не удалось установить tracing subscriber");

    // =========================================================================
    // ШАГ 3: Логируем конфигурацию
    // =========================================================================

    info!(
        "=== Console RCON Wrapper v{} ===",
        env!("CARGO_PKG_VERSION")
    );
    info!("Server name: {}", config.server_name);
    info!("Server type: {}", config.server_type);
    info!("Server binary: {}", config.server_bin);
    info!("Server port: {}", config.port);
    info!("API URL: {}", config.api_url);
    info!("Reconnect interval: {} сек", config.reconnect_secs);
    info!("Strip ANSI: {}", config.strip_ansi);
    info!("Buffer size: {}", config.buffer_size);
    info!("Secret key: [HIDDEN]");

    // =========================================================================
    // ШАГ 4: Создание каналов для межзадачного взаимодействия
    // =========================================================================

    // mpsc::channel создаёт multi-producer single-consumer канал.
    // - Sender можно клонировать (много producers)
    // - Receiver нельзя клонировать (один consumer)
    // buffer_size — максимальное количество сообщений в буфере.
    // При переполнении try_send() вернёт ошибку.

    // Канал для исходящих сообщений (stdout/stderr → WebSocket)
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<OutgoingMessage>(config.buffer_size);

    // Канал для входящих команд (WebSocket → stdin)
    // Буфер меньше — команды приходят редко
    let (incoming_tx, incoming_rx) = mpsc::channel::<String>(100);

    // =========================================================================
    // ШАГ 5: Создание координатора shutdown
    // =========================================================================

    // ShutdownCoordinator позволяет уведомить все задачи о необходимости
    // завершения работы. В данном случае мы создаём только один receiver,
    // потому что WebSocket клиент — единственная задача, которая должна
    // реагировать на shutdown (остальные завершатся сами при падении процесса).
    let (mut shutdown_coordinator, shutdown_rx) = ShutdownCoordinator::new();

    // =========================================================================
    // ШАГ 6: Запуск SCPSL процесса
    // =========================================================================

    info!("Запускаю сервер...");

    // spawn_server запускает сервер как дочерний процесс.
    // Возвращает Child — handle для управления процессом.
    let mut child = match process::spawn_server(&config).await {
        Ok(child) => child,
        Err(e) => {
            error!("Не удалось запустить сервер: {}", e);
            error!(
                "Проверьте, что файл {} существует и исполняемый",
                config.server_bin
            );
            // std::process::exit(1) завершает процесс с кодом ошибки
            std::process::exit(1);
        }
    };

    // =========================================================================
    // ШАГ 7: Извлечение stdin/stdout/stderr из процесса
    // =========================================================================

    // take() извлекает Option<T> и заменяет его на None.
    // Это нужно потому что Child владеет этими handles, а нам нужно
    // передать их в отдельные задачи.
    // expect() паникует с сообщением если Option == None.
    let stdin = child.stdin.take().expect("stdin должен быть piped");
    let stdout = child.stdout.take().expect("stdout должен быть piped");
    let stderr = child.stderr.take().expect("stderr должен быть piped");

    // =========================================================================
    // ШАГ 8: Запуск задач
    // =========================================================================

    // tokio::spawn() создаёт новую задачу (task), которая выполняется
    // параллельно с текущей. Возвращает JoinHandle для ожидания завершения.

    // --- Задача чтения stdout ---
    // Клонируем то, что нужно передать в задачу.
    // Задача будет владеть этими данными (ownership transfer).
    let outgoing_tx_stdout = outgoing_tx.clone();
    let server_name_stdout = config.server_name.clone();
    let strip_ansi = config.strip_ansi;

    let stdout_handle = tokio::spawn(async move {
        // async move — замыкание, которое захватывает переменные по значению (move)
        // и является асинхронным (async)
        process::read_stdout(stdout, outgoing_tx_stdout, server_name_stdout, strip_ansi).await;
        info!("Задача stdout reader завершена");
    });

    // --- Задача чтения stderr ---
    let outgoing_tx_stderr = outgoing_tx.clone();
    let server_name_stderr = config.server_name.clone();

    let stderr_handle = tokio::spawn(async move {
        process::read_stderr(stderr, outgoing_tx_stderr, server_name_stderr, strip_ansi).await;
        info!("Задача stderr reader завершена");
    });

    // --- Задача записи в stdin ---
    let stdin_handle = tokio::spawn(async move {
        websocket::stdin_writer_task(stdin, incoming_rx).await;
        info!("Задача stdin writer завершена");
    });

    // --- Задача WebSocket клиента ---
    // drop(outgoing_tx) — явно удаляем оригинальный sender.
    // Теперь только stdout и stderr readers владеют клонами sender'а.
    // Когда они завершатся — канал закроется, и WebSocket клиент узнает об этом.
    drop(outgoing_tx);

    let mut ws_client = websocket::WebSocketClient::new(config.clone(), outgoing_rx, incoming_tx);

    let ws_handle = tokio::spawn(async move {
        ws_client.run(shutdown_rx).await;
        info!("Задача WebSocket клиента завершена");
    });

    // --- Задача обработки сигналов ---
    let signal_handle = tokio::spawn(async move {
        signal::wait_for_shutdown_signal().await;
        // Сигнал получен — отправляем shutdown
        shutdown_coordinator.shutdown();
    });

    // =========================================================================
    // ШАГ 9: Ожидание завершения
    // =========================================================================

    // Есть два сценария завершения:
    // 1. SCPSL процесс завершился (упал или остановлен)
    // 2. Получен сигнал SIGTERM/SIGINT

    // tokio::select! ждёт первого завершившегося future
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

            // Корректно завершаем дочерний процесс
            let code = process::terminate_child(&mut child, 10).await.unwrap_or(0);
            code
        }
    };

    // =========================================================================
    // ШАГ 10: Cleanup
    // =========================================================================

    info!("Завершаю работу wrapper'а...");

    // Ждём завершения всех задач (с таймаутом)
    let cleanup_timeout = tokio::time::Duration::from_secs(5);

    // Отменяем задачи, которые ещё не завершились
    // abort() немедленно останавливает задачу
    stdout_handle.abort();
    stderr_handle.abort();
    stdin_handle.abort();
    ws_handle.abort();

    // Ждём небольшой таймаут для cleanup
    tokio::time::sleep(cleanup_timeout).await;

    info!("Wrapper завершён с кодом {}", exit_code);

    // Выходим с кодом SCPSL процесса
    std::process::exit(exit_code);
}

// =============================================================================
// ДОПОЛНИТЕЛЬНЫЕ ЗАМЕТКИ ДЛЯ ИЗУЧАЮЩИХ RUST
// =============================================================================
//
// ## Ownership и Borrowing
//
// В Rust каждое значение имеет ровно одного "владельца" (owner).
// Когда владелец выходит из области видимости — значение уничтожается.
//
// - `let x = String::from("hello");` — x владеет строкой
// - `let y = x;` — ownership передан в y, x больше нельзя использовать
// - `let z = x.clone();` — создаётся копия, x и z владеют разными строками
//
// Borrowing позволяет временно "одолжить" значение:
// - `&x` — иммутабельная ссылка, можно читать
// - `&mut x` — мутабельная ссылка, можно изменять
//
// ## Async/Await
//
// `async fn` возвращает Future — ленивое вычисление.
// `.await` ставит текущую задачу на паузу пока Future не завершится.
// Это позволяет одному потоку обрабатывать много задач конкурентно.
//
// ## Result и Error Handling
//
// `Result<T, E>` — тип для операций, которые могут упасть.
// - `Ok(value)` — успех
// - `Err(error)` — ошибка
//
// `?` оператор — если Result == Err, возвращает ошибку из функции.
// `unwrap()` — извлекает Ok, паникует при Err.
// `expect("msg")` — как unwrap, но с кастомным сообщением паники.
//
// ## Option
//
// `Option<T>` — значение, которое может отсутствовать.
// - `Some(value)` — значение есть
// - `None` — значения нет
//
// Аналог null/nil в других языках, но безопаснее — компилятор заставляет
// обрабатывать оба случая.
//
// =============================================================================
