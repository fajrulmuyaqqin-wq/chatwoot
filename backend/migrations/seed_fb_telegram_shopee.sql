-- Seed Facebook Page, Telegram, dan Shopee Channels untuk Account 1
DO $$
DECLARE
    v_account_id INTEGER;
    v_fb_channel_id INTEGER;
    v_tg_channel_id BIGINT;
    v_shopee_channel_id BIGINT;
BEGIN
    SELECT id INTO v_account_id FROM accounts LIMIT 1;
    IF v_account_id IS NULL THEN
        v_account_id := 1;
    END IF;

    -- 1. Insert Facebook Page Channel
    INSERT INTO channel_facebook_pages (id, account_id, page_id, user_access_token, page_access_token, provider_name, created_at, updated_at)
    VALUES (
        1,
        v_account_id,
        '109876543210987',
        'FB_USER_TOKEN_TEST_2026',
        'FB_PAGE_TOKEN_TEST_2026',
        'facebook',
        NOW(),
        NOW()
    )
    ON CONFLICT (page_id, account_id) DO UPDATE
    SET page_access_token = EXCLUDED.page_access_token,
        updated_at = NOW()
    RETURNING id INTO v_fb_channel_id;

    PERFORM setval('channel_facebook_pages_id_seq', (SELECT COALESCE(MAX(id), 1) FROM channel_facebook_pages));

    INSERT INTO inboxes (id, channel_id, account_id, name, channel_type, greeting_enabled, enable_auto_assignment, created_at, updated_at)
    VALUES (
        4,
        v_fb_channel_id,
        v_account_id,
        'Facebook Messenger Page',
        'Channel::FacebookPage',
        false,
        true,
        NOW(),
        NOW()
    )
    ON CONFLICT (id) DO UPDATE
    SET name = EXCLUDED.name,
        channel_type = EXCLUDED.channel_type,
        channel_id = EXCLUDED.channel_id,
        updated_at = NOW();

    -- 2. Insert Telegram Bot Channel
    INSERT INTO channel_telegram (id, account_id, bot_name, bot_token, created_at, updated_at)
    VALUES (
        1,
        v_account_id,
        'ChatwootIndoBot',
        '123456789:ABCdefGHIjklMNOpqrsTUVwxyz',
        NOW(),
        NOW()
    )
    ON CONFLICT (bot_token) DO UPDATE
    SET bot_name = EXCLUDED.bot_name,
        updated_at = NOW()
    RETURNING id INTO v_tg_channel_id;

    PERFORM setval('channel_telegram_id_seq', (SELECT COALESCE(MAX(id), 1) FROM channel_telegram));

    INSERT INTO inboxes (id, channel_id, account_id, name, channel_type, greeting_enabled, enable_auto_assignment, created_at, updated_at)
    VALUES (
        5,
        v_tg_channel_id,
        v_account_id,
        'Telegram @ChatwootIndoBot',
        'Channel::Telegram',
        false,
        true,
        NOW(),
        NOW()
    )
    ON CONFLICT (id) DO UPDATE
    SET name = EXCLUDED.name,
        channel_type = EXCLUDED.channel_type,
        channel_id = EXCLUDED.channel_id,
        updated_at = NOW();

    -- 3. Insert Shopee Chat Marketplace Channel
    INSERT INTO channel_shopee (id, account_id, shop_id, partner_id, partner_key, access_token, shop_name, created_at, updated_at)
    VALUES (
        1,
        v_account_id,
        99887766,
        100001,
        'mock_shopee_partner_key_id_2026',
        'shopee_test_access_token_indonesia_2026',
        'Shopee Mall Official Store ID',
        NOW(),
        NOW()
    )
    ON CONFLICT (shop_id, account_id) DO UPDATE
    SET access_token = EXCLUDED.access_token,
        updated_at = NOW()
    RETURNING id INTO v_shopee_channel_id;

    PERFORM setval('channel_shopee_id_seq', (SELECT COALESCE(MAX(id), 1) FROM channel_shopee));

    INSERT INTO inboxes (id, channel_id, account_id, name, channel_type, greeting_enabled, enable_auto_assignment, created_at, updated_at)
    VALUES (
        6,
        v_shopee_channel_id,
        v_account_id,
        'Shopee Chat Toko Resmi',
        'Channel::Shopee',
        false,
        true,
        NOW(),
        NOW()
    )
    ON CONFLICT (id) DO UPDATE
    SET name = EXCLUDED.name,
        channel_type = EXCLUDED.channel_type,
        channel_id = EXCLUDED.channel_id,
        updated_at = NOW();

    -- Sinkronisasi sequence inboxes
    PERFORM setval('inboxes_id_seq', (SELECT COALESCE(MAX(id), 1) FROM inboxes));

    RAISE NOTICE 'FB Page Channel %, Telegram Channel %, Shopee Channel % and Inboxes 4, 5, 6 successfully seeded.', v_fb_channel_id, v_tg_channel_id, v_shopee_channel_id;
END $$;
