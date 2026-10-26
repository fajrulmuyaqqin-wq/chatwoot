-- Migration untuk tabel channel_whatsapp
CREATE TABLE IF NOT EXISTS channel_whatsapp (
    id SERIAL PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    business_management_token TEXT,
    phone_number VARCHAR(255) NOT NULL UNIQUE,
    provider VARCHAR(255) DEFAULT 'whatsapp_cloud',
    provider_config JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    message_templates JSONB DEFAULT '{}'::jsonb,
    message_templates_last_updated TIMESTAMP WITHOUT TIME ZONE,
    phone_number_health JSONB NOT NULL DEFAULT '{}'::jsonb,
    phone_number_health_checked_at TIMESTAMP WITHOUT TIME ZONE,
    phone_number_health_error VARCHAR(500)
);

CREATE INDEX IF NOT EXISTS index_channel_whatsapp_on_phone_number ON channel_whatsapp (phone_number);
CREATE INDEX IF NOT EXISTS index_channel_whatsapp_on_phone_number_health_checked_at ON channel_whatsapp (phone_number_health_checked_at);
