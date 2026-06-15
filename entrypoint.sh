#!/bin/bash

set -e

if [ "$UPDATE_ON_START" = "true" ]; then
    if [ -x /scripts/update-scpsl.sh ]; then
        echo "Запускаю обновление сервера..."
        /scripts/update-scpsl.sh

    else
        echo "WARN: UPDATE_ON_START=true, но скрипт обновления не найден"
    fi
fi

/scripts/start-scpsl.sh