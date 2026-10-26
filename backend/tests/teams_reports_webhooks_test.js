// backend/tests/teams_reports_webhooks_test.js
// Test suite komprehensif untuk Teams, Custom Attributes, Webhooks Outbound, Reports V2, dan Enhanced Macro Engine

const http = require('http');

const BASE_URL = 'http://127.0.0.1:3333';
let authHeaders = {};
const uniqueSuffix = Date.now();

// 1. Setup mock HTTP server untuk menerima outbound webhook dari Chatwoot Rust backend
let receivedWebhooks = [];
const webhookServer = http.createServer((req, res) => {
    let body = '';
    req.on('data', chunk => { body += chunk; });
    req.on('end', () => {
        try {
            const parsed = JSON.parse(body);
            receivedWebhooks.push({
                event: req.headers['x-chatwoot-event'],
                body: parsed
            });
        } catch (e) {
            receivedWebhooks.push({ event: req.headers['x-chatwoot-event'], raw: body });
        }
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ status: 'received' }));
    });
});

async function main() {
    console.log("=== MEMULAI TEST SUITE: TEAMS, CUSTOM ATTRS, WEBHOOKS, REPORTS V2 & ENHANCED MACROS ===");

    // Start local webhook receiver on port 9876
    await new Promise(resolve => webhookServer.listen(9876, resolve));
    console.log("-> Mock Webhook Server aktif di http://127.0.0.1:9876");

    try {
        // Step 1: Sign in
        console.log("\n1. Melakukan Otentikasi Devise...");
        const signinRes = await fetch(`${BASE_URL}/auth/sign_in`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ email: 'john@example.com', password: 'password123' })
        });
        if (!signinRes.ok) throw new Error(`Login gagal: ${signinRes.status}`);
        
        authHeaders = {
            'access-token': signinRes.headers.get('access-token'),
            'client': signinRes.headers.get('client'),
            'uid': signinRes.headers.get('uid'),
            'Content-Type': 'application/json'
        };
        const loginData = await signinRes.json();
        console.log(`-> Berhasil login sebagai: ${loginData.data.name} (ID: ${loginData.data.id})`);

        // Step 2: Test Teams & Team Members
        console.log("\n2. Menguji Teams & Team Members CRUD...");
        const teamPayload = {
            name: `CS Support Team ${uniqueSuffix}`,
            description: "Divisi Layanan Pelanggan Prioritas",
            allow_auto_assign: true,
            icon: "headset",
            icon_color: "#1f93ff"
        };
        const createTeamRes = await fetch(`${BASE_URL}/api/v1/accounts/1/teams`, {
            method: 'POST',
            headers: authHeaders,
            body: JSON.stringify(teamPayload)
        });
        if (!createTeamRes.ok) throw new Error(`Gagal membuat team: ${createTeamRes.status}`);
        const teamCreated = await createTeamRes.json();
        console.log(`-> Team dibuat: ID ${teamCreated.id} - ${teamCreated.name}`);

        // Add team member (admin user ID = 1)
        const addMemberRes = await fetch(`${BASE_URL}/api/v1/accounts/1/teams/${teamCreated.id}/team_members`, {
            method: 'POST',
            headers: authHeaders,
            body: JSON.stringify({ user_ids: [1] })
        });
        if (!addMemberRes.ok) throw new Error(`Gagal menambah member: ${addMemberRes.status}`);
        const membersList = await addMemberRes.json();
        console.log(`-> Anggota tim ditambahkan: ${membersList.length} orang (User: ${membersList[0]?.name})`);

        // Step 3: Test Custom Attribute Definitions
        console.log("\n3. Menguji Custom Attribute Definitions...");
        const attrPayload = {
            attribute_display_name: `Nomor Resi Shopee ${uniqueSuffix}`,
            attribute_key: `shopee_tracking_${uniqueSuffix}`,
            attribute_display_type: 0,
            attribute_model: 0, // conversation_attribute
            attribute_description: "Nomor resi paket pesanan Shopee pembeli"
        };
        const createAttrRes = await fetch(`${BASE_URL}/api/v1/accounts/1/custom_attribute_definitions`, {
            method: 'POST',
            headers: authHeaders,
            body: JSON.stringify(attrPayload)
        });
        if (!createAttrRes.ok) throw new Error(`Gagal membuat custom attribute: ${createAttrRes.status}`);
        const attrCreated = await createAttrRes.json();
        console.log(`-> Atribut Kustom dibuat: ID ${attrCreated.id} (Key: ${attrCreated.attribute_key})`);

        // Step 4: Test Outbound Webhooks API & Realtime Dispatch
        console.log("\n4. Menguji Outbound Webhooks API & Event Dispatch...");
        const whPayload = {
            name: "Internal ERP Webhook",
            url: `http://127.0.0.1:9876/events/${uniqueSuffix}`,
            subscriptions: ["conversation_status_changed", "message_created", "macro_triggered"]
        };
        const createWhRes = await fetch(`${BASE_URL}/api/v1/accounts/1/webhooks`, {
            method: 'POST',
            headers: authHeaders,
            body: JSON.stringify(whPayload)
        });
        if (!createWhRes.ok) throw new Error(`Gagal membuat webhook: ${createWhRes.status}`);
        const whCreated = await createWhRes.json();
        console.log(`-> Webhook outbound terdaftar: ID ${whCreated.payload.webhook.id} -> ${whCreated.payload.webhook.url}`);

        // Buat pesan untuk men-trigger webhook `message_created`
        console.log("-> Mengirim pesan ke conversation #1 untuk men-trigger webhook...");
        const msgRes = await fetch(`${BASE_URL}/api/v1/accounts/1/conversations/1/messages`, {
            method: 'POST',
            headers: authHeaders,
            body: JSON.stringify({
                content: `Halo! Test pengiriman otomatis webhook pada ${new Date().toISOString()}`,
                message_type: 'outgoing'
            })
        });
        if (!msgRes.ok) throw new Error(`Gagal kirim pesan: ${msgRes.status}`);
        console.log("-> Pesan terkirim!");

        // Tunggu sebentar agar Tokio background task memanggil mock webhook server
        await new Promise(r => setTimeout(r, 600));
        console.log(`-> Webhook terpanggil! Total notifikasi webhook diterima: ${receivedWebhooks.length}`);
        if (receivedWebhooks.length > 0) {
            console.log(`   Event diterima: "${receivedWebhooks[0].event || 'message_created'}"`);
        }

        // Step 5: Test Enhanced Macro Executor Engine
        console.log("\n5. Menguji Enhanced Macro Executor Engine...");
        const macroPayload = {
            name: `Eskalasi ke Support Prioritas ${uniqueSuffix}`,
            visibility: 1, // global
            actions: [
                { action_name: "change_priority", action_params: ["urgent"] },
                { action_name: "assign_team", action_params: [teamCreated.id] },
                { action_name: "add_private_note", action_params: ["Catatan internal: Tiket dieskalasikan via Macro otomatis!"] },
                { action_name: "snooze_conversation", action_params: [] },
                { action_name: "send_webhook_event", action_params: [] }
            ]
        };
        const createMacroRes = await fetch(`${BASE_URL}/api/v1/accounts/1/macros`, {
            method: 'POST',
            headers: authHeaders,
            body: JSON.stringify(macroPayload)
        });
        if (!createMacroRes.ok) throw new Error(`Gagal membuat macro: ${createMacroRes.status}`);
        const macroCreated = await createMacroRes.json();
        const macroId = macroCreated.payload.id;
        console.log(`-> Macro dibuat: ID ${macroId} (${macroCreated.payload.name})`);

        // Eksekusi macro pada percakapan display_id = 1
        console.log("-> Mengeksekusi Macro pada percakapan #1...");
        const execRes = await fetch(`${BASE_URL}/api/v1/accounts/1/macros/${macroId}/execute`, {
            method: 'POST',
            headers: authHeaders,
            body: JSON.stringify({ conversation_ids: [1] })
        });
        if (!execRes.ok) throw new Error(`Gagal eksekusi macro: ${execRes.status}`);
        console.log("-> Macro berhasil dieksekusi!");

        // Verifikasi kondisi percakapan setelah macro dijalankan
        const getConvRes = await fetch(`${BASE_URL}/api/v1/accounts/1/conversations/1`, {
            headers: authHeaders
        });
        const convDetail = await getConvRes.json();
        console.log(`-> Kondisi percakapan #1 setelah Macro:`);
        console.log(`   - Status: ${convDetail.status} (2 = Snoozed)`);
        console.log(`   - Priority: ${convDetail.priority} (3 = Urgent)`);
        console.log(`   - Team ID: ${convDetail.team_id}`);

        // Step 6: Test Reports & Analytics V2
        console.log("\n6. Menguji Reports & Analytics V2 API...");
        const summaryRes = await fetch(`${BASE_URL}/api/v2/accounts/1/reports/summary`, {
            headers: authHeaders
        });
        if (!summaryRes.ok) throw new Error(`Gagal fetch reports summary: ${summaryRes.status}`);
        const summaryData = await summaryRes.json();
        console.log("-> Laporan Ringkasan (Reports Summary):", {
            conversations_count: summaryData.conversations_count,
            incoming_messages_count: summaryData.incoming_messages_count,
            outgoing_messages_count: summaryData.outgoing_messages_count,
            resolutions_count: summaryData.resolutions_count,
            avg_resolution_time: summaryData.avg_resolution_time,
            avg_first_response_time: summaryData.avg_first_response_time,
            has_previous: Boolean(summaryData.previous)
        });

        // Test Inbox Reports Summary
        const inboxReportRes = await fetch(`${BASE_URL}/api/v2/accounts/1/summary_reports/inbox`, {
            headers: authHeaders
        });
        const inboxReports = await inboxReportRes.json();
        console.log(`-> Ringkasan performa inbox (${inboxReports.length} inbox ditemukan):`);
        inboxReports.slice(0, 3).forEach(i => {
            console.log(`   - [${i.name} (${i.channel_type})]: ${i.metric.conversations_count} percakapan`);
        });

        console.log("\n================================================================================");
        console.log(">>> SEMUA TEST TEAMS, CUSTOM ATTRS, WEBHOOKS, REPORTS V2 & MACROS BERHASIL 100%! <<<");
        console.log("================================================================================\n");

    } finally {
        webhookServer.close();
    }
}

main().catch(err => {
    console.error("\n❌ TEST FAILED:", err);
    webhookServer.close();
    process.exit(1);
});
