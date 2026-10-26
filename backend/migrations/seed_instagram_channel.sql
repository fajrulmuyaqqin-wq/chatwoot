-- Seed Instagram Channel & Inbox untuk Account 1
DO $$
DECLARE
    v_account_id INTEGER;
    v_channel_id BIGINT;
    v_inbox_id INTEGER;
BEGIN
    SELECT id INTO v_account_id FROM accounts LIMIT 1;
    IF v_account_id IS NULL THEN
        v_account_id := 1;
    END IF;

    -- 1. Insert Instagram Channel
    INSERT INTO channel_instagram (id, account_id, access_token, expires_at, instagram_id, provider_name, created_at, updated_at)
    VALUES (
        1,
        v_account_id,
        'IG_TEST_ACCESS_TOKEN_INDONESIA_2026',
        NOW() + INTERVAL '60 days',
        '17841400000000001',
        'instagram',
        NOW(),
        NOW()
    )
    ON CONFLICT (instagram_id) DO UPDATE
    SET access_token = EXCLUDED.access_token,
        updated_at = NOW()
    RETURNING id INTO v_channel_id;

    -- Sinkronisasi sequence channel_instagram
    PERFORM setval('channel_instagram_id_seq', (SELECT COALESCE(MAX(id), 1) FROM channel_instagram));

    -- 2. Insert Inbox untuk Instagram
    INSERT INTO inboxes (id, channel_id, account_id, name, channel_type, greeting_enabled, enable_auto_assignment, created_at, updated_at)
    VALUES (
        3,
        v_channel_id,
        v_account_id,
        'Instagram DM @toko_indonesia',
        'Channel::Instagram',
        false,
        true,
        NOW(),
        NOW()
    )
    ON CONFLICT (id) DO UPDATE
    SET name = EXCLUDED.name,
        channel_type = EXCLUDED.channel_type,
        channel_id = EXCLUDED.channel_id,
        updated_at = NOW()
    RETURNING id INTO v_inbox_id;

    -- Sinkronisasi sequence inboxes
    PERFORM setval('inboxes_id_seq', (SELECT COALESCE(MAX(id), 1) FROM inboxes));

    RAISE NOTICE 'Instagram Channel ID % and Inbox ID % successfully seeded.', v_channel_id, v_inbox_id;
END $$;
