#!/usr/bin/env bash

set -xeuo pipefail

env_exists() {
    for i in "$@"; do
        # https://stackoverflow.com/a/1921337
        [[ ! -z ${!i+x} ]] || return 1
    done
}

command_exists() {
    for i in "$@"; do
        command -v "$i" >/dev/null
    done
}

command_exists sqlx psql
# Installing sqlx-cli:
# cargo install sqlx-cli --no-default-features --features postgres

# https://stackoverflow.com/a/11114547
ROOT_DIR=$(dirname $(realpath "$0"))
ENV_FILE=$(dirname "$ROOT_DIR")/.env
source "$ENV_FILE"

env_exists DB_USER DB_PASSWORD DB_PORT DB_NAME

docker image inspect 'postgres' >/dev/null
if [[ -z "$(docker ps -qf 'ancestor=postgres')" ]]; then
    docker run \
        -e POSTGRES_USER="$DB_USER" \
        -e POSTGRES_PASSWORD="$DB_PASSWORD" \
        -e POSTGRES_DB="$DB_NAME" \
        -p "$DB_PORT":5342 \
        --network host \
        -d postgres \
        postgres -N 1000
fi

until psql -U "$DB_USER" -h "localhost" -p "$DB_PORT" -d "$DB_NAME" -c '\q' 2>/dev/null; do
    echo 'Waiting on postgres...'
    sleep 1
done

export DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@localhost:${DB_PORT}/${DB_NAME}"
sqlx database create
sqlx migrate run
