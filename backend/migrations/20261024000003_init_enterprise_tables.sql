-- Migration untuk tabel fitur Enterprise Chatwoot: audits, sla_policies, applied_slas, sla_events, articles

CREATE TABLE IF NOT EXISTS audits (
    id SERIAL PRIMARY KEY,
    auditable_id BIGINT,
    auditable_type VARCHAR(255),
    associated_id BIGINT,
    associated_type VARCHAR(255),
    user_id BIGINT,
    user_type VARCHAR(255),
    username VARCHAR(255),
    action VARCHAR(255),
    audited_changes JSONB,
    version INTEGER DEFAULT 0,
    comment VARCHAR(255),
    remote_address VARCHAR(255),
    request_uuid VARCHAR(255),
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS index_audits_on_created_at ON audits (created_at);
CREATE INDEX IF NOT EXISTS index_audits_on_user_id ON audits (user_id);

CREATE TABLE IF NOT EXISTS sla_policies (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    first_response_time_threshold FLOAT,
    next_response_time_threshold FLOAT,
    only_during_business_hours BOOLEAN DEFAULT FALSE,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    description VARCHAR(255),
    resolution_time_threshold FLOAT
);

CREATE INDEX IF NOT EXISTS index_sla_policies_on_account_id ON sla_policies (account_id);

CREATE TABLE IF NOT EXISTS applied_slas (
    id SERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    sla_policy_id BIGINT NOT NULL REFERENCES sla_policies(id) ON DELETE CASCADE,
    conversation_id BIGINT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    sla_status INTEGER DEFAULT 0,
    completed_at TIMESTAMP WITHOUT TIME ZONE,
    CONSTRAINT unique_account_sla_conversation UNIQUE (account_id, sla_policy_id, conversation_id)
);

CREATE INDEX IF NOT EXISTS index_applied_slas_on_conversation_id ON applied_slas (conversation_id);

CREATE TABLE IF NOT EXISTS sla_events (
    id SERIAL PRIMARY KEY,
    applied_sla_id BIGINT NOT NULL REFERENCES applied_slas(id) ON DELETE CASCADE,
    conversation_id BIGINT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    sla_policy_id BIGINT NOT NULL REFERENCES sla_policies(id) ON DELETE CASCADE,
    inbox_id BIGINT NOT NULL,
    event_type INTEGER,
    meta JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS articles (
    id SERIAL PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    portal_id BIGINT,
    category_id BIGINT,
    folder_id BIGINT,
    author_id BIGINT,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    content TEXT,
    status INTEGER DEFAULT 1,
    views INTEGER DEFAULT 0,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS index_articles_on_account_id ON articles (account_id);
