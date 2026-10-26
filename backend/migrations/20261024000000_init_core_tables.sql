-- Inisialisasi skema tabel inti Chatwoot untuk Users, Accounts, dan AccountUsers

CREATE TABLE IF NOT EXISTS accounts (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    locale INTEGER DEFAULT 0,
    domain VARCHAR(100),
    support_email VARCHAR(100),
    feature_flags BIGINT NOT NULL DEFAULT 0,
    auto_resolve_duration INTEGER,
    limits JSONB DEFAULT '{}'::jsonb,
    custom_attributes JSONB DEFAULT '{}'::jsonb,
    status INTEGER DEFAULT 0,
    internal_attributes JSONB NOT NULL DEFAULT '{}'::jsonb,
    settings JSONB DEFAULT '{}'::jsonb,
    feature_flags_ext_1 BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    provider VARCHAR(255) NOT NULL DEFAULT 'email',
    uid VARCHAR(255) NOT NULL DEFAULT '',
    encrypted_password VARCHAR(255) NOT NULL DEFAULT '',
    reset_password_token VARCHAR(255),
    reset_password_sent_at TIMESTAMP WITHOUT TIME ZONE,
    remember_created_at TIMESTAMP WITHOUT TIME ZONE,
    sign_in_count INTEGER NOT NULL DEFAULT 0,
    current_sign_in_at TIMESTAMP WITHOUT TIME ZONE,
    last_sign_in_at TIMESTAMP WITHOUT TIME ZONE,
    current_sign_in_ip VARCHAR(255),
    last_sign_in_ip VARCHAR(255),
    confirmation_token VARCHAR(255),
    confirmed_at TIMESTAMP WITHOUT TIME ZONE,
    confirmation_sent_at TIMESTAMP WITHOUT TIME ZONE,
    unconfirmed_email VARCHAR(255),
    name VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    email VARCHAR(255),
    tokens JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    pubsub_token VARCHAR(255),
    availability INTEGER DEFAULT 0,
    ui_settings JSONB DEFAULT '{}'::jsonb,
    custom_attributes JSONB DEFAULT '{}'::jsonb,
    type VARCHAR(255),
    message_signature TEXT,
    otp_secret VARCHAR(255),
    consumed_timestep INTEGER,
    otp_required_for_login BOOLEAN DEFAULT FALSE,
    otp_backup_codes TEXT,
    device_trust_version INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS index_users_on_email ON users (email);
CREATE UNIQUE INDEX IF NOT EXISTS index_users_on_pubsub_token ON users (pubsub_token);
CREATE UNIQUE INDEX IF NOT EXISTS index_users_on_uid_and_provider ON users (uid, provider);

CREATE TABLE IF NOT EXISTS account_users (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT REFERENCES accounts(id) ON DELETE CASCADE,
    user_id BIGINT REFERENCES users(id) ON DELETE CASCADE,
    role INTEGER DEFAULT 0,
    inviter_id BIGINT,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    active_at TIMESTAMP WITHOUT TIME ZONE,
    availability INTEGER NOT NULL DEFAULT 0,
    auto_offline BOOLEAN NOT NULL DEFAULT TRUE,
    custom_role_id BIGINT,
    agent_capacity_policy_id BIGINT
);

CREATE UNIQUE INDEX IF NOT EXISTS uniq_user_id_per_account_id ON account_users (account_id, user_id);
