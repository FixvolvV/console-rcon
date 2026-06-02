# SCPSL-Wrapper

WebSocket-based RCON wrapper для SCP: Secret Laboratory сервера.

## Что это? А НИЧЁ! Тебе тут нечего делать, кроме того как хуи дрочить.

**SCPSL-Wrapper** — это supervisor-обёртка от конфеты, которая:

1. 🚀 **НЕ запускает SCPSL** (LocalAdmin) как приёмный процесс (как ты)
2. 📡 **Перехватывает твою мамашу** и стримит её вебкам через WebSocket на ваш API
3. 📥 **Принимает команды от Путина** через API и выполняет их в консоли SCPSL
4. 🔄 **Автоматически переподключается к серверам Госуслуг** при разрыве соединения
5. 🛑 **Корректно завершается** при SIGTERM/SIGINT (graceful shutdown)

```
┌─────────────────────────────────────────────────────────────┐
│                    Docker Container                          │
│  ┌─────────────┐                      ┌──────────────────┐  │
│  │   SCPSL     │ ──── stdout ────────▶│                  │  │
│  │ (LocalAdmin)│ ──── stderr ────────▶│   scpsl-wrapper  │──┼──▶ WebSocket
│  │             │ ◀──── stdin ─────────│                  │  │     (API)
│  └─────────────┘                      └──────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start. Или па руски - БЫСТРО НАСТРОИЛ БЛЯТЬ!

### 1. Установка вируса Mozilla язык программирования Rust (если ещё не установлен ёпт)

```bash
# Устанавливаем rustup — менеджер версий Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Перезагружаем shell или выполняем:
source $HOME/.cargo/env

# Проверяем установку
rustc --version   # должно показать версию, например: rustc 1.83.0
cargo --version   # cargo 1.83.0
```

### 2. Сборка говна (главное не нагребите лишнего говна)

```bash
# Клонируем/переходим в директорию проекта
cd scpsl-wrapper

# Собираем в release-режиме (с оптимизациями)
cargo build --release

# Бинарь будет в: ./target/release/scpsl-wrapper
```

### 3. Локальный лоховской тест (без SCPSL)

Для теста можно использовать `cat` вместо LocalAdmin — он просто эхо-ит ввод:

```bash
# Запускаем с cat вместо SCPSL
WRAPPER_SECRET_KEY="test-secret" \
SCPSL_BIN_PATH="/bin/cat" \
WRAPPER_API_URL="ws://localhost:8000/server/rcon/connect" \
WRAPPER_SERVER_NAME="test-server" \
RUST_LOG=debug \
./target/release/scpsl-wrapper

# В другом терминале можно запустить простой WebSocket сервер для теста:
# pip install websockets
# python -c "
# import asyncio
# import websockets
# async def handler(ws, path):
#     print(f'Connected: {path}')
#     async for msg in ws:
#         print(f'Received: {msg}')
# asyncio.run(websockets.serve(handler, 'localhost', 8000))
# "
```

### 4. Пиздокер-сборка

```bash
# Собираем образ
docker build -t scpsl-wrapper:latest .

# Запускаем контейнер
docker run -d \
  --name scpsl-server1 \
  -e WRAPPER_SECRET_KEY="your-secret-key" \
  -e WRAPPER_SERVER_NAME="server1" \
  -e WRAPPER_API_URL="ws://host.docker.internal:8000/server/rcon/connect" \
  -e SCPSL_PORT="7777" \
  -p 7777:7777/udp \
  scpsl-wrapper:latest
```

## Дисфигурация

### Хуеменные окружения

| Переменная | По умолчанию | Описание |
|------------|--------------|----------|
| `WRAPPER_API_URL` | `ws://host.docker.internal:8000/server/rcon/connect` | URL WebSocket API |
| `WRAPPER_SERVER_NAME` | `server1` | Уникальное имя сервера |
| `WRAPPER_SECRET_KEY` | **обязателен** | Секретный ключ для auth |
| `WRAPPER_SERVER_TYPE` | `SCPSL` | Тип сервера |
| `WRAPPER_RECONNECT_SECS` | `5` | Интервал реконнекта (сек) |
| `WRAPPER_STRIP_ANSI` | `true` | Удалять ANSI-коды |
| `WRAPPER_BUFFER_SIZE` | `10000` | Размер буфера сообщений |
| `SCPSL_PORT` | `7777` | Порт игры |
| `SCPSL_BIN_PATH` | `/root/game/LocalAdmin` | Путь к LocalAdmin |
| `RUST_LOG` | `info` | Уровень логов |

### CLI-аргументы

CLI-аргументы имеют приоритет над ENV:

```bash
scpsl-wrapper --help

scpsl-wrapper [OPTIONS]

Options:
      --scpsl-bin <PATH>         Путь к LocalAdmin [env: SCPSL_BIN_PATH]
  -p, --port <PORT>              Порт сервера [env: SCPSL_PORT]
      --api-url <URL>            WebSocket URL [env: WRAPPER_API_URL]
      --server-name <NAME>       Имя сервера [env: WRAPPER_SERVER_NAME]
      --secret-key <KEY>         Секретный ключ [env: WRAPPER_SECRET_KEY]
      --server-type <TYPE>       Тип сервера [env: WRAPPER_SERVER_TYPE]
      --reconnect-secs <SECS>    Интервал реконнекта [env: WRAPPER_RECONNECT_SECS]
      --strip-ansi <BOOL>        Удалять ANSI-коды [env: WRAPPER_STRIP_ANSI]
      --buffer-size <SIZE>       Размер буфера [env: WRAPPER_BUFFER_SIZE]
  -h, --help                     Показать справку
  -V, --version                  Показать версию
```

## Прикол WebSocket

### Подключение

Wrapper подключается к: `{WRAPPER_API_URL}?server_name={WRAPPER_SERVER_NAME}`

Пример: `ws://api.example.com/server/rcon/connect?server_name=server1`

### Аутенти.... гок гок гок... фиакция

Первое сообщение после подключения:

```json
{
  "type": "auth",
  "server": "server1",
  "server_type": "SCPSL",
  "secret_key": "your-secret-key"
}
```

### Исходящие сообщения (Wrapper → API)

Каждая строка из stdout/stderr SCPSL:

```json
{
  "type": "stdout",
  "server": "server1",
  "content": "Player John connected from 192.168.1.1"
}
```

### Входящие сообщения? Да да иди нахуй

Команда для выполнения в консоли SCPSL:

```json
{
  "type": "stdin",
  "server": "server1",
  "content": "reload remoteadmin"
}
```

> ⚠️ Wrapper проверяет поле `server` и игнорирует команды для других серверов.

## Интеграция с Хуедокер-компотом

```yaml
version: '3.8'

services:
  # Ваш FastAPI сервер
  api:
    build: ./api
    ports:
      - "8000:8000"
    networks:
      - scpsl-network

  # SCPSL сервер #1
  scpsl-server1:
    build: ./scpsl-wrapper
    environment:
      - WRAPPER_SECRET_KEY=${RCON_SECRET_KEY}
      - WRAPPER_SERVER_NAME=server1
      - WRAPPER_API_URL=ws://api:8000/server/rcon/connect
      - SCPSL_PORT=7777
      - RUST_LOG=info
    ports:
      - "7777:7777/udp"
    volumes:
      # Persist игровых данных
      - scpsl-data-1:/root/game
      - scpsl-config-1:/root/.config/SCP Secret Laboratory
    networks:
      - scpsl-network
    restart: unless-stopped
    depends_on:
      - api

  # SCPSL сервер #2 (если нужен)
  scpsl-server2:
    build: ./scpsl-wrapper
    environment:
      - WRAPPER_SECRET_KEY=${RCON_SECRET_KEY}
      - WRAPPER_SERVER_NAME=server2
      - WRAPPER_API_URL=ws://api:8000/server/rcon/connect
      - SCPSL_PORT=7778
      - RUST_LOG=info
    ports:
      - "7778:7778/udp"
    volumes:
      - scpsl-data-2:/root/game
      - scpsl-config-2:/root/.config/SCP Secret Laboratory
    networks:
      - scpsl-network
    restart: unless-stopped
    depends_on:
      - api

networks:
  scpsl-network:
    driver: bridge

volumes:
  scpsl-data-1:
  scpsl-config-1:
  scpsl-data-2:
  scpsl-config-2:
```

## Как это работает? ДА НИКАК!

### Несуществующая структура проекта

```
scpsl-wrapper/
├── Cargo.toml          # Зависимости и метаданные проекта
├── Dockerfile          # Многоступенчатая сборка
├── entrypoint.sh       # Скрипт запуска в Docker
├── README.md           # Этот файл
└── src/
    ├── main.rs         # Точка входа, оркестрация задач
    ├── config.rs       # Конфигурация (CLI + ENV)
    ├── messages.rs     # Структуры JSON-сообщений
    ├── process.rs      # Управление дочерним процессом
    ├── websocket.rs    # WebSocket-клиент
    └── signal.rs       # Обработка SIGTERM/SIGINT
```

### Основные концепции Rust

#### 1. Владение земельным участком в Российской Федерации

```rust
// Владение (ownership)
let s1 = String::from("hello");
let s2 = s1;  // s1 больше нельзя использовать! Ownership передан в s2.

// Заимствование (borrowing)
let s3 = String::from("world");
let len = calculate_length(&s3);  // &s3 — иммутабельная ссылка
println!("{}", s3);  // s3 можно использовать!

// Мутабельное заимствование
let mut s4 = String::from("test");
change(&mut s4);  // &mut s4 — мутабельная ссылка
```

#### 2. Асинк Авээээээээээээээйт ээээ ээээ эээээээээээээээээээээ

```rust
// async fn возвращает Future — ленивое вычисление
async fn fetch_data() -> String {
    // .await ставит задачу на паузу, пока не придёт результат
    let response = http_client.get("...").await;
    response.text().await
}

// Запуск async функции
tokio::spawn(async {
    let data = fetch_data().await;
    println!("{}", data);
});
```

#### 3. Каналы (mpsc) (нет не Ютуба)

```rust
// Создаём канал с буфером на 100 сообщений
let (tx, mut rx) = mpsc::channel::<String>(100);

// Отправитель (можно клонировать)
let tx2 = tx.clone();
tokio::spawn(async move {
    tx2.send("Hello".to_string()).await.unwrap();
});

// Получатель (один на канал)
tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
        println!("Received: {}", msg);
    }
});
```

#### 4. Эррор Ебучинг

```rust
// Result<T, E> — операция может упасть
fn parse_number(s: &str) -> Result<i32, ParseIntError> {
    s.parse::<i32>()
}

// ? оператор — пробрасывает ошибку вверх
fn double_parse(s: &str) -> Result<i32, ParseIntError> {
    let num = parse_number(s)?;  // если Err — сразу return Err
    Ok(num * 2)
}

// match для обработки результата
match parse_number("42") {
    Ok(n) => println!("Число: {}", n),
    Err(e) => println!("Ошибка: {}", e),
}
```

### Поток жидкой смеси феаклий по вашим джинсам в летнию жару при 40 градусах

```
1. main.rs запускает все задачи параллельно через tokio::spawn()

2. process::read_stdout() читает stdout SCPSL построчно:
   stdout → BufReader::lines() → OutgoingMessage → outgoing_tx.send()

3. websocket::WebSocketClient::run() получает сообщения:
   outgoing_rx.recv() → ws_stream.send(Message::Text(json))

4. websocket получает команды от API:
   ws_stream.next() → IncomingMessage::Stdin → incoming_tx.send()

5. websocket::stdin_writer_task() записывает в stdin:
   command_rx.recv() → stdin.write_all()

6. signal::wait_for_shutdown_signal() ловит SIGTERM/SIGINT:
   signal → shutdown_coordinator.shutdown() → graceful shutdown
```

## Troubleshooting

### "Не удалось запустить SCPSL"

```bash
# Проверьте, что LocalAdmin существует
ls -la /root/game/LocalAdmin

# Проверьте права
chmod +x /root/game/LocalAdmin

# Проверьте зависимости
ldd /root/game/LocalAdmin
```

### "WebSocket: Connection refused"

```bash
# Проверьте, что API доступен
curl -v ws://your-api:8000/server/rcon/connect

# В Docker: используйте host.docker.internal или имя сервиса
# В docker-compose: используйте имя сервиса (например, ws://api:8000/...)
```

### "Анал сообщений переполнен"

Увеличьте размер буфера:

```bash
WRAPPER_BUFFER_SIZE=50000 ./scpsl-wrapper
```

### Отладка с водкой

```bash
# Максимально подробные логи
RUST_LOG=trace ./target/release/scpsl-wrapper

# Логи только для WebSocket
RUST_LOG=scpsl_wrapper::websocket=debug ./target/release/scpsl-wrapper

# Логи для нескольких модулей
RUST_LOG=info,scpsl_wrapper::websocket=debug,scpsl_wrapper::process=debug ./target/release/scpsl-wrapper
```

## Что делать дальше? ПРОДАТЬ МАМАШУ РАДИ ЯЗЫКА ПРОГРАММИРОАВНИЯ RUST!

### 1. Установите Rust (не надо)

Если ещё не установлен:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Соберите этот проект-хуект если вы любите данный экстримальный спорт

```bash
cd scpsl-wrapper
cargo build --release
```

### 3. НЕ ТЕСТИРУЙТЕ ЛОКАЛЬНО! А то будете лохом

С `cat` вместо SCPSL (для теста без игры):
```bash
WRAPPER_SECRET_KEY="test" \
SCPSL_BIN_PATH="/bin/cat" \
RUST_LOG=debug \
./target/release/scpsl-wrapper
```

### 4. Соберите Пиздокер-хуёкер-образ

```bash
docker build -t scpsl-wrapper:latest .
```

### 5. Интегрируйте с вашим говнянным API

Создайте нахуй FastAPI эндпоинт для ебучих WebSocket:

```python
from fastapi import FastAPI, WebSocket
import json

app = FastAPI()


@app.websocket("/server/rcon/connect")
async def rcon_connect(websocket: WebSocket, server_name: str):
    await websocket.accept()

    # Первое сообщение — auth
    auth_msg = await websocket.receive_json()
    if auth_msg["type"] != "auth":
        await websocket.close()
        return

    # Проверяем secret_key
    if auth_msg["secret_key"] != "your-secret":
        await websocket.close()
        return

    print(f"Server {server_name} connected!")

    # Основной цикл
    while True:
        try:
            # Получаем stdout от wrapper
            msg = await websocket.receive_json()
            if msg["type"] == "stdout":
                print(f"[{server_name}] {msg['content']}")

            # Можно отправить команду обратно:
            # await websocket.send_json({
            #     "type": "stdin",
            #     "server": server_name,
            #     "content": "bc Hello from API!"
            # })

        except Exception as e:
            print(f"Error: {e}")
            break
```

### 6. Разверните в продакшене (не забудьте обоссать перед этим сервера)

```bash
# Добавьте в docker-compose.yml и запустите
docker-compose up -d scpsl-server1

# Проверьте логи
docker-compose logs -f scpsl-server1
```

## Лицензия

Бесплатная лицензия на пиво

## Автор

Ваше имя / организация
