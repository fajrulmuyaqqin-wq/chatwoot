-- Migration: 20261024000009_init_campaigns_and_csat.sql
-- Matching Chatwoot db/schema.rb for Campaigns and CSAT Survey Responses

-- 1. Campaigns Table
CREATE TABLE IF NOT EXISTS campaigns (
    id BIGSERIAL PRIMARY KEY,
    display_id INTEGER NOT NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    message TEXT NOT NULL,
    sender_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    enabled BOOLEAN DEFAULT TRUE,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    inbox_id BIGINT NOT NULL REFERENCES inboxes(id) ON DELETE CASCADE,
    trigger_rules JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    campaign_type INTEGER DEFAULT 0 NOT NULL,
    campaign_status INTEGER DEFAULT 0 NOT NULL,
    audience JSONB DEFAULT '[]'::jsonb,
    scheduled_at TIMESTAMP WITHOUT TIME ZONE,
    trigger_only_during_business_hours BOOLEAN DEFAULT FALSE,
    template_params JSONB,
    started_at TIMESTAMP WITHOUT TIME ZONE,
    completed_at TIMESTAMP WITHOUT TIME ZONE,
    whatsapp_cursor BIGINT
);

CREATE INDEX IF NOT EXISTS index_campaigns_on_account_id ON campaigns (account_id);
CREATE INDEX IF NOT EXISTS index_campaigns_on_campaign_status ON campaigns (campaign_status);
CREATE INDEX IF NOT EXISTS index_campaigns_on_campaign_type ON campaigns (campaign_type);
CREATE INDEX IF NOT EXISTS index_campaigns_on_inbox_id ON campaigns (inbox_id);
CREATE INDEX IF NOT EXISTS index_campaigns_on_scheduled_at ON campaigns (scheduled_at);

-- 2. CSAT Survey Responses Table
CREATE TABLE IF NOT EXISTS csat_survey_responses (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    conversation_id BIGINT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    message_id BIGINT NOT NULL UNIQUE REFERENCES messages(id) ON DELETE CASCADE,
    rating INTEGER NOT NULL,
    feedback_message TEXT,
    contact_id BIGINT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    assigned_agent_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    csat_review_notes TEXT,
    review_notes_updated_at TIMESTAMP WITHOUT TIME ZONE,
    review_notes_updated_by_id BIGINT REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS index_csat_survey_responses_on_account_id ON csat_survey_responses (account_id);
CREATE INDEX IF NOT EXISTS index_csat_survey_responses_on_assigned_agent_id ON csat_survey_responses (assigned_agent_id);
CREATE INDEX IF NOT EXISTS index_csat_survey_responses_on_contact_id ON csat_survey_responses (contact_id);
CREATE INDEX IF NOT EXISTS index_csat_survey_responses_on_conversation_id ON csat_survey_responses (conversation_id);
CREATE INDEX IF NOT EXISTS index_csat_survey_responses_on_review_notes_updated_by_id ON csat_survey_responses (review_notes_updated_by_id);
