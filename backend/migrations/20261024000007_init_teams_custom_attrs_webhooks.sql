-- Migration: 20261024000007_init_teams_custom_attrs_webhooks.sql
-- Matching Chatwoot db/schema.rb for Teams, Team Members, Custom Attribute Definitions, and Webhooks

-- 1. Teams Table
CREATE TABLE IF NOT EXISTS teams (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    allow_auto_assign BOOLEAN DEFAULT TRUE,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    icon VARCHAR(255) DEFAULT '',
    icon_color VARCHAR(255) DEFAULT ''
);

CREATE INDEX IF NOT EXISTS index_teams_on_account_id ON teams (account_id);
CREATE UNIQUE INDEX IF NOT EXISTS index_teams_on_name_and_account_id ON teams (name, account_id);

-- 2. Team Members Table
CREATE TABLE IF NOT EXISTS team_members (
    id BIGSERIAL PRIMARY KEY,
    team_id BIGINT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS index_team_members_on_team_id ON team_members (team_id);
CREATE INDEX IF NOT EXISTS index_team_members_on_user_id ON team_members (user_id);
CREATE UNIQUE INDEX IF NOT EXISTS index_team_members_on_team_id_and_user_id ON team_members (team_id, user_id);

-- 3. Custom Attribute Definitions Table
CREATE TABLE IF NOT EXISTS custom_attribute_definitions (
    id BIGSERIAL PRIMARY KEY,
    attribute_display_name VARCHAR(255),
    attribute_key VARCHAR(255),
    attribute_display_type INTEGER DEFAULT 0,
    default_value INTEGER,
    attribute_model INTEGER DEFAULT 0,
    account_id BIGINT REFERENCES accounts(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    attribute_description TEXT,
    attribute_values JSONB DEFAULT '[]'::jsonb,
    regex_pattern VARCHAR(255),
    regex_cue VARCHAR(255)
);

CREATE INDEX IF NOT EXISTS index_custom_attribute_definitions_on_account_id ON custom_attribute_definitions (account_id);
CREATE UNIQUE INDEX IF NOT EXISTS attribute_key_model_index ON custom_attribute_definitions (attribute_key, attribute_model, account_id);

-- 4. Webhooks Table
CREATE TABLE IF NOT EXISTS webhooks (
    id BIGSERIAL PRIMARY KEY,
    account_id INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
    inbox_id INTEGER REFERENCES inboxes(id) ON DELETE CASCADE,
    url TEXT,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    webhook_type INTEGER DEFAULT 0,
    subscriptions JSONB DEFAULT '["conversation_status_changed", "conversation_updated", "conversation_created", "contact_created", "contact_updated", "message_created", "message_updated", "webwidget_triggered"]'::jsonb,
    name VARCHAR(255),
    secret VARCHAR(255)
);

CREATE UNIQUE INDEX IF NOT EXISTS index_webhooks_on_account_id_and_url ON webhooks (account_id, url);
