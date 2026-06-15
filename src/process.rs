//! =============================================================================
//! process.rs — Управление дочерним процессом сервера
//! =============================================================================
//!
//! Этот модуль отвечает за:
//! 1. Запуск сервера как дочернего процесса с piped stdin/stdout/stderr
//! 2. Чтение stdout/stderr построчно и отправка в канал сообщений
//! 3. Запись команд в stdin процесса
//! 4. Корректное завершение процесса при shutdown
//!
//! Основные концепции:
//! - tokio::process::Command — асинхронная версия std::process::Command
//! - tokio::io::BufReader — буферизованный reader для построчного чтения
//! - tokio::sync::mpsc — каналы для передачи сообщений между задачами
//!
//! =============================================================================

use crate::config::Config;
use crate::messages::OutgoingMessage;
use lazy_static::lazy_static;
use regex::Regex;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

// =============================================================================
// ANSI ESCAPE CODES REGEX
// =============================================================================

// lazy_static! создаёт статическую переменную, которая инициализируется
// при первом обращении. Это нужно потому что Regex::new() не const fn.
lazy_static! {
    /// Регулярное выражение для поиска ANSI escape-кодов.
    ///
    /// ANSI-коды имеют формат: ESC[...m (где ESC = \x1b = \033)
    /// Примеры:
    /// - \x1b[31m — красный цвет текста
    /// - \x1b[0m — сброс форматирования
    /// - \x1b[1;32m — жирный зелёный
    ///
    /// Регулярка ловит все варианты: \x1b[ + любые символы + буква в конце
    static ref ANSI_REGEX: Regex = Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
}

/// Удаляет ANSI escape-коды из строки.
///
/// # Аргументы
/// * `input` - Исходная строка (возможно с ANSI-кодами)
///
/// # Возвращает
/// Строка без ANSI-кодов
///
/// # Пример
/// ```rust
/// let colored = "\x1b[31mError\x1b[0m: something went wrong";
/// let clean = strip_ansi_codes(colored);
/// assert_eq!(clean, "Error: something went wrong");
/// ```
pub fn strip_ansi_codes(input: &str) -> String {
    // replace_all заменяет все совпадения на пустую строку
    // .to_string() конвертирует Cow<str> в String
    ANSI_REGEX.replace_all(input, "").to_string()
}

// =============================================================================
// ЗАПУСК ПРОЦЕССА
// =============================================================================

/// Запускает сервер как дочерний процесс.
///
/// Процесс запускается с piped stdin/stdout/stderr — это значит, что мы
/// получаем handles для чтения/записи в эти потоки программно.
///
/// # Аргументы
/// * `config` - Конфигурация с путём к бинарю и портом
///
/// # Возвращает
/// * `Ok(Child)` - Handle дочернего процесса
/// * `Err(std::io::Error)` - Ошибка запуска (файл не найден, нет прав и т.д.)
pub async fn spawn_server(config: &Config) -> std::io::Result<Child> {
    info!("Запускаю сервер: {} {}", config.server_bin, config.port);

    // Command::new() создаёт builder для запуска процесса
    let child = Command::new("bash")
        // .arg() добавляет аргумент командной строки
        // port.to_string() — конвертируем u16 в String
        .arg("-c")
        .arg("for i in {0..100}; do echo $i; sleep 1; done")
        // Stdio::piped() — stdin будет каналом, в который мы можем писать
        .stdin(Stdio::piped())
        // Stdio::piped() — stdout будет каналом, из которого мы можем читать
        .stdout(Stdio::piped())
        // Аналогично для stderr
        .stderr(Stdio::piped())
        // kill_on_drop(true) — если Child дропнется, процесс получит SIGKILL
        // Это предотвращает зомби-процессы если wrapper паникнет
        .kill_on_drop(true)
        // .spawn() запускает процесс и возвращает Result<Child>
        .spawn()?;

    info!(
        "Сервер {} запущен с PID: {:?}",
        config.server_type,
        child.id()
    );
    Ok(child)
}

// =============================================================================
// ЧТЕНИЕ STDOUT/STDERR
// =============================================================================

/// Читает stdout процесса построчно и отправляет сообщения в канал.
///
/// Эта функция запускается в отдельной tokio-задаче (spawn) и работает
/// до тех пор, пока процесс не завершится (stdout не закроется).
///
/// # Аргументы
/// * `stdout` - Handle на stdout процесса (piped)
/// * `tx` - Sender канала для исходящих сообщений
/// * `server_name` - Имя сервера для включения в сообщения
/// * `strip_ansi` - Удалять ли ANSI-коды
pub async fn read_stdio<R: AsyncRead + Unpin>(
    // tokio::process::ChildStdout — async reader для stdout процесса
    stdio: R,
    // mpsc::Sender — отправитель в канал. Clone позволяет иметь несколько отправителей.
    tx: mpsc::Sender<OutgoingMessage>,
    // String, а не &str, потому что эта функция будет жить в отдельной задаче
    // и должна владеть своими данными (ownership).
    server_name: String,
    strip_ansi: bool,
) {
    // BufReader добавляет буферизацию к reader'у.
    // Это эффективнее чем читать по байту, и позволяет использовать lines().
    let reader = BufReader::new(stdio);

    // .lines() возвращает AsyncLinesReader — итератор по строкам.
    // Это не настоящий итератор (не impl Iterator), а stream, поэтому
    // мы используем while let вместо for.
    let mut lines = reader.lines();

    // .next_line().await — асинхронно читает следующую строку.
    // Возвращает Ok(Some(line)) если есть строка, Ok(None) если EOF.
    // while let Some(line) = ... — паттерн для обработки Result<Option<T>>
    // ? внутри Ok(...) — пробрасывает ошибку, но здесь мы в async fn без Result,
    // поэтому используем match или if let.
    loop {
        // Читаем следующую строку
        match lines.next_line().await {
            Ok(Some(line)) => {
                // Пропускаем пустые строки
                if line.trim().is_empty() {
                    continue;
                }

                // Опционально удаляем ANSI-коды
                let content = if strip_ansi {
                    strip_ansi_codes(&line)
                } else {
                    line
                };

                // После strip_ansi строка может стать пустой
                if content.trim().is_empty() {
                    continue;
                }

                // Создаём сообщение
                let msg = OutgoingMessage::Stdout {
                    server: server_name.clone(),
                    content: content.clone(),
                };

                // Логируем на уровне debug (только если RUST_LOG=debug)
                debug!(target: "stdout", "{}", content);

                // Отправляем в канал (неблокирующая отправка)
                if let Err(e) = tx.try_send(msg) {
                    match e {
                        mpsc::error::TrySendError::Full(_) => {
                            warn!("Буфер сообщений переполнен, сообщение отброшено");
                        }
                        mpsc::error::TrySendError::Closed(_) => {
                            error!("Канал сообщений закрыт, ожидаю подключения websocket");
                        }
                    }
                }
            }
            Ok(None) => {
                info!("stdout закрыт (процесс завершился)");
                break;
            }
            Err(e) => {
                // Ошибка чтения — логируем и продолжаем
                // Это может быть из-за невалидного UTF-8, но BufReader::lines()
                // использует from_utf8_lossy внутри, так что это маловероятно.
                error!("Ошибка чтения stdout: {}", e);
                break;
            }
        }
    }
}

// =============================================================================
// ЗАПИСЬ В STDIN
// =============================================================================

/// Тип для stdin writer'а.
///
/// tokio::process::ChildStdin — async writer, в который можно писать команды.
/// Мы оборачиваем его в тип для удобства.
pub type StdinWriter = tokio::process::ChildStdin;

/// Записывает команду в stdin процесса.
///
/// Команда автоматически дополняется символом новой строки (\n).
pub async fn write_stdin(stdin: &mut StdinWriter, command: &str) -> std::io::Result<()> {
    let cmd = if command.ends_with('\n') {
        command.to_string()
    } else {
        format!("{}\n", command)
    };

    info!("Отправляю команду: {}", command.trim());

    stdin.write_all(cmd.as_bytes()).await?;
    stdin.flush().await?;

    Ok(())
}

// =============================================================================
// ЗАВЕРШЕНИЕ ПРОЦЕССА
// =============================================================================

/// Корректно завершает дочерний процесс.
///
/// Порядок действий:
/// 1. Отправляем SIGTERM (мягкое завершение)
/// 2. Ждём завершения с таймаутом
/// 3. Если не завершился — отправляем SIGKILL (принудительное)
///
/// # Аргументы
/// * `child` - Мутабельная ссылка на дочерний процесс
/// * `timeout_secs` - Таймаут ожидания завершения в секундах
///
/// # Возвращает
/// Exit code процесса (или None если не удалось получить)
pub async fn terminate_child(child: &mut Child, timeout_secs: u64) -> Option<i32> {
    // Получаем PID для логирования
    let pid = child.id();
    info!("Завершаю дочерний процесс (PID: {:?})", pid);

    // Шаг 1: Пытаемся мягко завершить через start_kill()
    // start_kill() отправляет SIGKILL на Unix, но мы сначала попробуем SIGTERM
    #[cfg(unix)]
    {
        // На Unix используем nix или libc для отправки SIGTERM
        // Но tokio::process::Child не предоставляет прямой способ отправить SIGTERM,
        // поэтому используем libc через std::process::Command
        if let Some(pid) = pid {
            info!("Отправляю SIGTERM процессу {}", pid);
            // unsafe потому что мы вызываем C-функцию напрямую
            // libc::kill() отправляет сигнал процессу
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
    }

    // Шаг 2: Ждём завершения с таймаутом
    let timeout = tokio::time::Duration::from_secs(timeout_secs);

    // tokio::time::timeout() оборачивает future в таймаут.
    // Возвращает Ok(result) если успели, Err(Elapsed) если таймаут.
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => {
            // Процесс завершился вовремя
            info!("Процесс завершился с кодом: {:?}", status.code());
            status.code()
        }
        Ok(Err(e)) => {
            // Ошибка ожидания (редко)
            error!("Ошибка ожидания завершения процесса: {}", e);
            None
        }
        Err(_) => {
            // Таймаут — процесс не завершился
            warn!(
                "Процесс не завершился за {} сек, отправляю SIGKILL",
                timeout_secs
            );

            // Шаг 3: SIGKILL
            if let Err(e) = child.kill().await {
                error!("Ошибка отправки SIGKILL: {}", e);
            }

            // Ждём завершения после SIGKILL
            match child.wait().await {
                Ok(status) => status.code(),
                Err(e) => {
                    error!("Ошибка ожидания после SIGKILL: {}", e);
                    None
                }
            }
        }
    }
}
