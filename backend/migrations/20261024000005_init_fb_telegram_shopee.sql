-- Migration untuk Facebook Messenger, Telegram Bot, dan Shopee Chat
-- 1. Facebook Pages Channel
CREATE TABLE IF NOT EXISTS channel_facebook_pages (
    id SERIAL PRIMARY KEY,
    page_id VARCHAR NOT NULL,
    user_access_token VARCHAR NOT NULL,
    page_access_token VARCHAR NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    instagram_id VARCHAR,
    provider_name VARCHAR DEFAULT 'facebook',
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT index_channel_facebook_pages_on_page_id_and_account_id UNIQUE (page_id, account_id)
);
CREATE INDEX IF NOT EXISTS index_channel_facebook_pages_on_page_id ON channel_facebook_pages (page_id);

-- 2. Telegram Bot Channel
CREATE TABLE IF NOT EXISTS channel_telegram (
    id BIGSERIAL PRIMARY KEY,
    bot_name VARCHAR,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    bot_token VARCHAR NOT NULL UNIQUE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS index_channel_telegram_on_bot_token ON channel_telegram (bot_token);

-- 3. Shopee Chat Marketplace Channel (E-Commerce Indonesia)
CREATE TABLE IF NOT EXISTS channel_shopee (
    id BIGSERIAL PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    shop_id BIGINT NOT NULL,
    partner_id BIGINT NOT NULL DEFAULT 100001,
    partner_key VARCHAR(255) NOT NULL DEFAULT 'mock_shopee_partner_key_id_2026',
    access_token TEXT NOT NULL,
    refresh_token TEXT,
    shop_name VARCHAR(255) DEFAULT 'Shopee Official Store Indonesia',
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_channel_shopee_shop UNIQUE (shop_id, account_id)
);
CREATE INDEX IF NOT EXISTS index_channel_shopee_on_shop_id ON channel_shopee (shop_id);
