-- Add migration script here
CREATE TABLE users (
    id uuid PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_phc_hash TEXT NOT NULL
)