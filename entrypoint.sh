#!/bin/bash

set -e

SERVER_BIN="${SERVER_BIN_PATH:-/root/game/LocalAdmin}"
SERVER_PORT="${SERVER_PORT:-7777}"

# -----------------------------------------------------------------------------
# Опциональное обновление при старте
# -----------------------------------------------------------------------------
if [ "$UPDATE_ON_START" = "true" ] || [ "$UPDATE_ON_START" = "1" ]; then
    if [ -x /scripts/update-scpsl.sh ]; then
        echo "Запускаю обновление сервера..."
        /scripts/update-scpsl.sh
    else
        echo "WARN: UPDATE_ON_START=true, но скрипт обновления не найден"
    fi
fi

# -----------------------------------------------------------------------------
# Проверка бинаря сервера
# -----------------------------------------------------------------------------
if [ ! -f "$SERVER_BIN" ]; then
    echo "ERROR: Server binary not found: $SERVER_BIN"
    echo ""
    echo "Варианты решения:"
    echo "  1. Смонтируйте игру в /root/game"
    echo "  2. Установите UPDATE_ON_START=true для автоустановки"
    echo "  3. Запустите вручную: docker exec <container> /scripts/update-scpsl.sh"
    exit 1
fi

if [ ! -x "$SERVER_BIN" ]; then
    chmod +x "$SERVER_BIN"
fi

# -----------------------------------------------------------------------------
# Запуск wrapper
# -----------------------------------------------------------------------------
cd /root/game
exec /usr/local/bin/rcon-console "$@"
