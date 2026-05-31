#!/bin/bash

set -e

GAME_DIR="${GAME_DIR:-/root/game}"
STEAMCMD_PATH="/usr/bin/steamcmd"
SCPSL_APP_ID="996560"

# -----------------------------------------------------------------------------
# Установка SteamCMD
# -----------------------------------------------------------------------------
install_steamcmd() {
    echo "=== Установка SteamCMD ==="
    
    # Принимаем лицензию автоматически
    echo "steam steam/question select I AGREE" | debconf-set-selections
    echo "steam steam/license note ''" | debconf-set-selections
    
    # Добавляем non-free репозитории (нужны для steamcmd)
    if [ -f /etc/apt/sources.list.d/debian.sources ]; then
        sed -i 's/Components: main$/Components: main non-free/g' /etc/apt/sources.list.d/debian.sources
    fi
    
    # 32-bit архитектура для steamcmd
    dpkg --add-architecture i386
    
    apt-get update -y
    apt-get install -y --no-install-recommends steamcmd
    
    # Симлинк для удобства
    ln -sf /usr/games/steamcmd "$STEAMCMD_PATH"
    
    echo "SteamCMD установлен"
}

# -----------------------------------------------------------------------------
# Обновление SCPSL
# -----------------------------------------------------------------------------
update_scpsl() {
    echo "=== Обновление SCP: Secret Laboratory ==="
    echo "Директория: $GAME_DIR"
    
    mkdir -p "$GAME_DIR"
    
    "$STEAMCMD_PATH" \
        +force_install_dir "$GAME_DIR" \
        +login anonymous \
        +app_update "$SCPSL_APP_ID" validate \
        +quit
    
    # Делаем LocalAdmin исполняемым
    if [ -f "$GAME_DIR/LocalAdmin" ]; then
        chmod +x "$GAME_DIR/LocalAdmin"
        echo "SCPSL обновлён успешно"
    else
        echo "ОШИБКА: LocalAdmin не найден после установки!"
        exit 1
    fi
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
echo "========================================"
echo "SCPSL Update Script"
echo "========================================"

# Проверяем steamcmd
if [ ! -x "$STEAMCMD_PATH" ]; then
    echo "SteamCMD не найден, устанавливаю..."
    install_steamcmd
fi

# Обновляем игру
update_scpsl

echo "========================================"
echo "Готово!"
echo "========================================"