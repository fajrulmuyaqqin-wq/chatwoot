-- Migration untuk tabel channel_instagram (Instagram Direct Message)
CREATE TABLE IF NOT EXISTS channel_instagram (
    id BIGSERIAL PRIMARY KEY,
    access_token VARCHAR NOT NULL,
    expires_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT (NOW() + INTERVAL '60 days'),
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    instagram_id VARCHAR NOT NULL UNIQUE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    provider_name VARCHAR DEFAULT 'instagram'
);

CREATE INDEX IF NOT EXISTS index_channel_instagram_on_instagram_id ON channel_instagram (instagram_id);
CREATE INDEX IF NOT EXISTS index_channel_instagram_on_account_id ON channel_instagram (account_id);
