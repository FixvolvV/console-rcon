#!/bin/bash

set -e

install_steamcmd() {
    echo "=========================================="
    echo "Установка SteamCMD..."
    echo "=========================================="
    
    echo "steam steam/question select I AGREE" | debconf-set-selections
    echo "steam steam/license note ''" | debconf-set-selections

    if [ -f /etc/apt/sources.list.d/debian.sources ]; then
        sed -i 's/Components: main$/Components: main non-free/g' /etc/apt/sources.list.d/debian.sources
    fi

    dpkg --add-architecture i386
    
    apt-get update -y
    apt-get install -y steamcmd libcurl4
    
    ln -sf /usr/games/steamcmd /usr/bin/steamcmd
    
    echo "SteamCMD установлен успешно"
}

install_scpsl() {
    echo "=========================================="
    echo "Установка/обновление SCP: Secret Laboratory..."
    echo "=========================================="
    
    steamcmd \
        +force_install_dir /root/game \
        +login anonymous \
        +app_update 996560 validate \
        +quit
    
    echo "SCPSL установлен в /root/game"
}

echo "=========================================="
echo "SCPSL Wrapper Entrypoint"
echo "=========================================="
echo "Server name: ${WRAPPER_SERVER_NAME:-server1}"
echo "SCPSL port: ${SCPSL_PORT:-7777}"
echo "=========================================="

if [ ! -f /root/game/LocalAdmin ]; then
    echo "LocalAdmin не найден, устанавливаю SCPSL..."

    if ! command -v steamcmd &> /dev/null; then
        echo "SteamCMD не найден, устанавливаю..."
        install_steamcmd
    fi
    
    install_scpsl
else
    echo "LocalAdmin найден, пропускаю установку"
fi

if [ ! -x /root/game/LocalAdmin ]; then
    echo "ОШИБКА: /root/game/LocalAdmin не существует или не исполняемый!"
    exit 1
fi

cd /root/game

echo "=========================================="
echo "Запускаю SCPSL Wrapper..."
echo "=========================================="

exec /usr/local/bin/scpsl-wrapper "$@"
