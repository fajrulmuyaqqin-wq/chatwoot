-- Seed sample data untuk fitur Enterprise (Articles & SLA Policies)
DO $$
BEGIN
    -- Masukkan artikel sampel jika belum ada
    IF NOT EXISTS (SELECT 1 FROM articles WHERE account_id = 1 AND title = 'Kebijakan Pengembalian & Refund Barang') THEN
        INSERT INTO articles (
            account_id,
            title,
            description,
            content,
            status,
            views,
            created_at,
            updated_at
        )
        VALUES (
            1,
            'Kebijakan Pengembalian & Refund Barang',
            'Tata cara refund dan klaim garansi produk rusak dalam 14 hari.',
            'Barang dapat dikembalikan maksimal 14 hari kerja setelah diterima dengan menyertakan bukti resi pembelian dan foto kondisi barang.',
            1,
            15,
            NOW(),
            NOW()
        );
    END IF;

    -- Masukkan SLA Policy sampel (ambang batas 2 detik untuk demonstrasi cepat)
    IF NOT EXISTS (SELECT 1 FROM sla_policies WHERE account_id = 1 AND name = 'Fast Response SLA (2s)') THEN
        INSERT INTO sla_policies (
            account_id,
            name,
            description,
            first_response_time_threshold,
            resolution_time_threshold,
            created_at,
            updated_at
        )
        VALUES (
            1,
            'Fast Response SLA (2s)',
            'Kebijakan SLA pengujian cepat dengan ambang batas respon pertama 2 detik',
            2.0,
            3600.0,
            NOW(),
            NOW()
        );
    END IF;
END $$;
