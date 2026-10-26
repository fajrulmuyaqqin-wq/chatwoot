-- Seed Data untuk Paket Produktivitas Agen
DO $$
DECLARE
    v_account_id INTEGER;
    v_user_id BIGINT;
BEGIN
    SELECT id INTO v_account_id FROM accounts LIMIT 1;
    IF v_account_id IS NULL THEN
        v_account_id := 1;
    END IF;

    SELECT id INTO v_user_id FROM users LIMIT 1;

    -- 1. Seed Canned Responses
    INSERT INTO canned_responses (account_id, short_code, content, created_at, updated_at)
    VALUES
        (v_account_id, 'halo', 'Halo kak! Terima kasih telah menghubungi CS kami. Ada yang bisa kami bantu?', NOW(), NOW()),
        (v_account_id, 'rekening', 'Rekening pembayaran resmi kami: Bank BCA 123-456-7890 a/n PT Chatwoot Indonesia.', NOW(), NOW()),
        (v_account_id, 'ongkir', 'Pengiriman kami menggunakan JNE/J&T/SiCepat dengan gratis ongkir ke seluruh Indonesia!', NOW(), NOW())
    ON CONFLICT (short_code, account_id) DO UPDATE
    SET content = EXCLUDED.content, updated_at = NOW();

    -- 2. Seed Labels
    INSERT INTO labels (account_id, title, description, color, show_on_sidebar, created_at, updated_at)
    VALUES
        (v_account_id, 'vip', 'Pelanggan Prioritas / VIP', '#e11d48', true, NOW(), NOW()),
        (v_account_id, 'leads', 'Potensi Penjualan Baru', '#16a34a', true, NOW(), NOW()),
        (v_account_id, 'komplain', 'Kendala Pengiriman / Produk', '#ea580c', true, NOW(), NOW())
    ON CONFLICT (title, account_id) DO UPDATE
    SET description = EXCLUDED.description, color = EXCLUDED.color, updated_at = NOW();

    -- 3. Seed Macro
    INSERT INTO macros (account_id, name, visibility, created_by_id, updated_by_id, actions, created_at, updated_at)
    VALUES (
        v_account_id,
        'Selesaikan & Beri Label VIP',
        1, -- global
        v_user_id,
        v_user_id,
        '[{"action_name": "add_label", "action_params": ["vip"]}, {"action_name": "resolve_conversation", "action_params": []}]'::jsonb,
        NOW(),
        NOW()
    )
    ON CONFLICT DO NOTHING;

    RAISE NOTICE 'Productivity Suite seed data successfully inserted.';
END $$;
