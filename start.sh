#!/bin/bash

# Запуск первого бинарника в фоне
cargo run --bin chat-application &

# Подождать 3 секунды
sleep 3

# Запуск второго бинарника
cargo run --bin client
