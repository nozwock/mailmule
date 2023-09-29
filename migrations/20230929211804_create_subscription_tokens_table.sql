-- Add migration script here
CREATE TABLE subscription_tokens(
    subscription_token TEXT NOT NULL,
    subscriber_id uuid NOT NULL REFERENCES subscribers (id),
    PRIMARY KEY (subscription_token)
)