-- Migration: 20261024000008_init_automation_rules_and_notifications.sql
-- Matching Chatwoot db/schema.rb for Automation Rules and In-App Notifications

-- 1. Automation Rules Table
CREATE TABLE IF NOT EXISTS automation_rules (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    event_name VARCHAR(255) NOT NULL,
    conditions JSONB NOT NULL DEFAULT '[]'::jsonb,
    actions JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    execution_delay INTEGER,
    monitor_id BIGINT,
    monitor_event_activated_at TIMESTAMP WITHOUT TIME ZONE
);

CREATE INDEX IF NOT EXISTS index_automation_rules_on_account_id ON automation_rules (account_id);
CREATE INDEX IF NOT EXISTS index_automation_rules_on_event_name ON automation_rules (event_name);

-- 2. Notifications Table
CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type INTEGER NOT NULL,
    primary_actor_type VARCHAR(255) NOT NULL,
    primary_actor_id BIGINT NOT NULL,
    secondary_actor_type VARCHAR(255),
    secondary_actor_id BIGINT,
    read_at TIMESTAMP WITHOUT TIME ZONE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    snoozed_until TIMESTAMP WITHOUT TIME ZONE,
    last_activity_at TIMESTAMP WITHOUT TIME ZONE DEFAULT NOW(),
    meta JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS index_notifications_on_account_id ON notifications (account_id);
CREATE INDEX IF NOT EXISTS index_notifications_on_user_id ON notifications (user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_performance ON notifications (user_id, account_id, snoozed_until, read_at);
CREATE INDEX IF NOT EXISTS index_notifications_on_last_activity_at ON notifications (last_activity_at);
