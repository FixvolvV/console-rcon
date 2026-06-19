#!/bin/bash

set -e

SERVER_BIN="${SERVER_BIN_PATH:-/root/game/LocalAdmin}"
SERVER_PORT="${SERVER_PORT:-7777}"

if [ ! -f "$SERVER_BIN" ]; then
    BINARE_FOUND="false"
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

cd /root/game
exec /usr/local/bin/rcon-console "$@"