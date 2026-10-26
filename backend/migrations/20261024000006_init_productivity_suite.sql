-- Migration untuk Paket Produktivitas Agen: Canned Responses, Labels, Taggings, dan Macros
-- 1. Canned Responses
CREATE TABLE IF NOT EXISTS canned_responses (
    id SERIAL PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    short_code VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_canned_responses_short_code_account UNIQUE (short_code, account_id)
);
CREATE INDEX IF NOT EXISTS index_canned_responses_on_account_id ON canned_responses (account_id);

-- 2. Labels
CREATE TABLE IF NOT EXISTS labels (
    id BIGSERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    color VARCHAR(255) NOT NULL DEFAULT '#1f93ff',
    show_on_sidebar BOOLEAN DEFAULT TRUE,
    account_id BIGINT REFERENCES accounts(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT index_labels_on_title_and_account_id UNIQUE (title, account_id)
);
CREATE INDEX IF NOT EXISTS index_labels_on_account_id ON labels (account_id);

-- 3. Tags & Taggings (Conversation Label Associations)
CREATE TABLE IF NOT EXISTS tags (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    taggings_count INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS taggings (
    id SERIAL PRIMARY KEY,
    tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    taggable_type VARCHAR(255) NOT NULL DEFAULT 'Conversation',
    taggable_id INTEGER NOT NULL,
    tagger_type VARCHAR(255),
    tagger_id INTEGER,
    context VARCHAR(128) DEFAULT 'labels',
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS index_taggings_on_taggable_id_and_taggable_type ON taggings (taggable_id, taggable_type);

-- 4. Macros
CREATE TABLE IF NOT EXISTS macros (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    visibility INTEGER NOT NULL DEFAULT 0, -- 0: personal, 1: global
    created_by_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    updated_by_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    actions JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS index_macros_on_account_id ON macros (account_id);
