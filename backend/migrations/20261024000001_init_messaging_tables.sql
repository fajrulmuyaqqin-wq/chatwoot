-- Migration untuk tabel messaging inti Chatwoot: inboxes, contacts, contact_inboxes, conversations, messages, attachments

CREATE TABLE IF NOT EXISTS inboxes (
    id SERIAL PRIMARY KEY,
    channel_id INTEGER NOT NULL DEFAULT 0,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    channel_type VARCHAR(255) DEFAULT 'Channel::Api',
    enable_auto_assignment BOOLEAN DEFAULT TRUE,
    greeting_enabled BOOLEAN DEFAULT FALSE,
    greeting_message VARCHAR(255),
    email_address VARCHAR(255),
    working_hours_enabled BOOLEAN DEFAULT FALSE,
    out_of_office_message VARCHAR(255),
    timezone VARCHAR(255) DEFAULT 'UTC',
    enable_email_collect BOOLEAN DEFAULT TRUE,
    csat_survey_enabled BOOLEAN DEFAULT FALSE,
    allow_messages_after_resolved BOOLEAN DEFAULT TRUE,
    auto_assignment_config JSONB DEFAULT '{}'::jsonb,
    lock_to_single_conversation BOOLEAN NOT NULL DEFAULT FALSE,
    portal_id BIGINT,
    sender_name_type INTEGER NOT NULL DEFAULT 0,
    business_name VARCHAR(255),
    csat_config JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS index_inboxes_on_account_id ON inboxes (account_id);

CREATE TABLE IF NOT EXISTS contacts (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) DEFAULT '',
    email VARCHAR(255),
    phone_number VARCHAR(255),
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    additional_attributes JSONB DEFAULT '{}'::jsonb,
    identifier VARCHAR(255),
    custom_attributes JSONB DEFAULT '{}'::jsonb,
    last_activity_at TIMESTAMP WITHOUT TIME ZONE,
    contact_type INTEGER DEFAULT 0,
    middle_name VARCHAR(255) DEFAULT '',
    last_name VARCHAR(255) DEFAULT '',
    location VARCHAR(255) DEFAULT '',
    country_code VARCHAR(255) DEFAULT '',
    blocked BOOLEAN NOT NULL DEFAULT FALSE,
    company_id BIGINT
);

CREATE INDEX IF NOT EXISTS index_contacts_on_account_id ON contacts (account_id);

CREATE TABLE IF NOT EXISTS contact_inboxes (
    id SERIAL PRIMARY KEY,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    inbox_id INTEGER NOT NULL REFERENCES inboxes(id) ON DELETE CASCADE,
    source_id VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    hmac_verified BOOLEAN DEFAULT FALSE,
    pubsub_token VARCHAR(255)
);

CREATE UNIQUE INDEX IF NOT EXISTS index_contact_inboxes_on_inbox_id_and_source_id ON contact_inboxes (inbox_id, source_id);

CREATE TABLE IF NOT EXISTS conversations (
    id SERIAL PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    inbox_id INTEGER NOT NULL REFERENCES inboxes(id) ON DELETE CASCADE,
    status INTEGER NOT NULL DEFAULT 0, -- 0 = open, 1 = resolved, 2 = pending, 3 = snoozed
    assignee_id INTEGER,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    contact_id BIGINT REFERENCES contacts(id) ON DELETE SET NULL,
    display_id INTEGER NOT NULL,
    contact_last_seen_at TIMESTAMP WITHOUT TIME ZONE,
    agent_last_seen_at TIMESTAMP WITHOUT TIME ZONE,
    additional_attributes JSONB DEFAULT '{}'::jsonb,
    contact_inbox_id BIGINT REFERENCES contact_inboxes(id) ON DELETE SET NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    identifier VARCHAR(255),
    last_activity_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    team_id BIGINT,
    campaign_id BIGINT,
    snoozed_until TIMESTAMP WITHOUT TIME ZONE,
    custom_attributes JSONB DEFAULT '{}'::jsonb,
    assignee_last_seen_at TIMESTAMP WITHOUT TIME ZONE,
    first_reply_created_at TIMESTAMP WITHOUT TIME ZONE,
    priority INTEGER,
    sla_policy_id BIGINT,
    waiting_since TIMESTAMP WITHOUT TIME ZONE,
    cached_label_list TEXT,
    assignee_agent_bot_id BIGINT,
    ai_assignee_type VARCHAR(255),
    status_changed_at TIMESTAMP WITHOUT TIME ZONE
);

CREATE UNIQUE INDEX IF NOT EXISTS index_conversations_on_account_id_and_display_id ON conversations (account_id, display_id);
CREATE INDEX IF NOT EXISTS index_conversations_on_account_id ON conversations (account_id);
CREATE INDEX IF NOT EXISTS index_conversations_on_inbox_id ON conversations (inbox_id);

CREATE TABLE IF NOT EXISTS messages (
    id SERIAL PRIMARY KEY,
    content TEXT,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    inbox_id INTEGER NOT NULL REFERENCES inboxes(id) ON DELETE CASCADE,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    message_type INTEGER NOT NULL, -- 0 = incoming, 1 = outgoing, 2 = activity, 3 = template
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    private BOOLEAN NOT NULL DEFAULT FALSE,
    status INTEGER DEFAULT 0, -- 0 = sent, 1 = delivered, 2 = read, 3 = failed
    source_id TEXT,
    content_type INTEGER NOT NULL DEFAULT 0, -- 0 = text, 1 = input_text, etc.
    content_attributes JSONB DEFAULT '{}'::jsonb,
    sender_type VARCHAR(255), -- 'User' or 'Contact'
    sender_id BIGINT,
    external_source_ids JSONB DEFAULT '{}'::jsonb,
    additional_attributes JSONB DEFAULT '{}'::jsonb,
    processed_message_content TEXT,
    sentiment JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS index_messages_on_conversation_id ON messages (conversation_id);
CREATE INDEX IF NOT EXISTS index_messages_on_account_id ON messages (account_id);

CREATE TABLE IF NOT EXISTS attachments (
    id SERIAL PRIMARY KEY,
    file_type INTEGER DEFAULT 0, -- 0 = image, 1 = audio, 2 = video, 3 = file, 4 = location, 5 = fallback, 6 = share
    external_url VARCHAR(255),
    coordinates_lat DOUBLE PRECISION DEFAULT 0.0,
    coordinates_long DOUBLE PRECISION DEFAULT 0.0,
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    fallback_title VARCHAR(255),
    extension VARCHAR(255),
    meta JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS index_attachments_on_message_id ON attachments (message_id);
CREATE INDEX IF NOT EXISTS index_attachments_on_account_id ON attachments (account_id);
