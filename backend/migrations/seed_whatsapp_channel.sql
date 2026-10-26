-- Seed sample channel_whatsapp dan inbox terkait
DO $$
DECLARE
    v_channel_id INTEGER;
    v_inbox_id INTEGER;
BEGIN
    -- Masukkan channel_whatsapp jika belum ada
    INSERT INTO channel_whatsapp (
        account_id,
        phone_number,
        provider,
        provider_config,
        created_at,
        updated_at
    )
    VALUES (
        1,
        '+6281234567890',
        'whatsapp_cloud',
        '{
            "phone_number_id": "100200300400500",
            "business_account_id": "900800700600",
            "api_key": "mock_whatsapp_api_key_secret",
            "webhook_verify_token": "chatwoot_wa_verify_token_123",
            "app_secret": "test_app_secret_123"
        }'::jsonb,
        NOW(),
        NOW()
    )
    ON CONFLICT (phone_number) DO UPDATE
    SET provider_config = EXCLUDED.provider_config, updated_at = NOW()
    RETURNING id INTO v_channel_id;

    -- Masukkan atau perbarui inbox untuk channel WhatsApp ini
    IF NOT EXISTS (SELECT 1 FROM inboxes WHERE account_id = 1 AND channel_type = 'Channel::Whatsapp' AND channel_id = v_channel_id) THEN
        INSERT INTO inboxes (
            channel_id,
            account_id,
            name,
            channel_type,
            created_at,
            updated_at
        )
        VALUES (
            v_channel_id,
            1,
            'WhatsApp Support (+6281234567890)',
            'Channel::Whatsapp',
            NOW(),
            NOW()
        );
    END IF;
END $$;
