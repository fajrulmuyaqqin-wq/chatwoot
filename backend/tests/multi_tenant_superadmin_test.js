// backend/tests/multi_tenant_superadmin_test.js
// Test suite komprehensif untuk Multi-Tenant, RBAC (Pundit-style Guard), Manajemen Agen, dan Super Admin Dashboard

const BASE_URL = 'http://127.0.0.1:3333';
const uniqueSuffix = Date.now();

async function getAuthHeaders(email, password) {
    const res = await fetch(`${BASE_URL}/auth/sign_in`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password })
    });
    if (!res.ok) throw new Error(`Login gagal untuk ${email}: ${res.status}`);
    return {
        'access-token': res.headers.get('access-token'),
        'client': res.headers.get('client'),
        'uid': res.headers.get('uid'),
        'Content-Type': 'application/json'
    };
}

async function main() {
    console.log("================================================================================");
    console.log("   TEST SUITE: MULTI-TENANT, RBAC PERMISSIONS & SUPER ADMIN DASHBOARD          ");
    console.log("================================================================================\n");

    // 1. UJI RBAC (PUNDIT-STYLE GUARD): AGEN BIASA DITOLAK UNTUK AKSI ADMIN
    console.log("=== STEP 1: VERIFIKASI PUNDIT RBAC GUARD (Agent Biasa Ditolak Akses Admin) ===");
    const agentHeaders = await getAuthHeaders('budi.agent@example.com', 'password123');
    console.log("-> Login berhasil sebagai Regular Agent (Budi Agent)");

    // Agent biasa mencoba membuat agen baru -> Harus 403 Forbidden
    const unauthCreateRes = await fetch(`${BASE_URL}/api/v1/accounts/1/agents`, {
        method: 'POST',
        headers: agentHeaders,
        body: JSON.stringify({
            name: "Hacker Agent",
            email: "hacker@example.com",
            role: "administrator"
        })
    });
    console.log(`-> Mencoba POST /agents sebagai Regular Agent: Status ${unauthCreateRes.status}`);
    if (unauthCreateRes.status !== 403) {
        throw new Error(`RBAC Bypass! Seharusnya 403 Forbidden, tapi didapat: ${unauthCreateRes.status}`);
    }
    const errBody = await unauthCreateRes.json();
    console.log(`-> Penolakan terverifikasi: "${errBody.error}"`);

    // Agent biasa mencoba akses Super Admin -> Harus 403 Forbidden
    const unauthSuperRes = await fetch(`${BASE_URL}/super_admin/api/v1/accounts`, {
        headers: agentHeaders
    });
    console.log(`-> Mencoba akses /super_admin/api/v1/accounts sebagai Regular Agent: Status ${unauthSuperRes.status}`);
    if (unauthSuperRes.status !== 403) {
        throw new Error(`SuperAdmin Guard Gagal! Seharusnya 403 Forbidden, tapi didapat: ${unauthSuperRes.status}`);
    }
    console.log("-> SuperAdmin Guard berhasil menolak pengguna non-SuperAdmin.");

    // 2. UJI MANAJEMEN AGEN OLEH ADMINISTRATOR AKUN
    console.log("\n=== STEP 2: MANAJEMEN AGEN OLEH ADMINISTRATOR (John Doe) ===");
    const adminHeaders = await getAuthHeaders('john@example.com', 'password123');
    console.log("-> Login berhasil sebagai Administrator (John Doe)");

    // Admin membuat agen baru
    const newAgentEmail = `siti.support.${uniqueSuffix}@example.com`;
    const createAgentRes = await fetch(`${BASE_URL}/api/v1/accounts/1/agents`, {
        method: 'POST',
        headers: adminHeaders,
        body: JSON.stringify({
            name: "Siti Rahmawati",
            email: newAgentEmail,
            role: "agent",
            auto_offline: true
        })
    });
    if (!createAgentRes.ok) throw new Error(`Gagal membuat agen baru: ${createAgentRes.status}`);
    const agentCreated = await createAgentRes.json();
    console.log(`-> Agen baru berhasil diundang: ID ${agentCreated.id} - ${agentCreated.name} (${agentCreated.role})`);

    // Ambil daftar agen akun
    const listAgentsRes = await fetch(`${BASE_URL}/api/v1/accounts/1/agents`, {
        headers: adminHeaders
    });
    const agentsList = await listAgentsRes.json();
    console.log(`-> Total agen di Account 1: ${agentsList.length} orang`);

    // Update agen: Promosikan menjadi administrator
    const updateAgentRes = await fetch(`${BASE_URL}/api/v1/accounts/1/agents/${agentCreated.id}`, {
        method: 'PATCH',
        headers: adminHeaders,
        body: JSON.stringify({
            name: "Siti Rahmawati (Lead)",
            role: "administrator"
        })
    });
    if (!updateAgentRes.ok) throw new Error(`Gagal update agen: ${updateAgentRes.status}`);
    const agentUpdated = await updateAgentRes.json();
    console.log(`-> Agen berhasil dipromosikan: ${agentUpdated.name} -> Role: ${agentUpdated.role}`);

    // Hapus agen dari akun
    const deleteAgentRes = await fetch(`${BASE_URL}/api/v1/accounts/1/agents/${agentCreated.id}`, {
        method: 'DELETE',
        headers: adminHeaders
    });
    if (!deleteAgentRes.ok) throw new Error(`Gagal menghapus agen: ${deleteAgentRes.status}`);
    console.log("-> Agen berhasil dihapus dari akun.");

    // 3. UJI MULTI-TENANT LIFECYCLE (PEMBUATAN TENANT AKUN BARU)
    console.log("\n=== STEP 3: MULTI-TENANT LIFECYCLE (Pembuatan Tenant Baru) ===");
    const createAccRes = await fetch(`${BASE_URL}/api/v1/accounts`, {
        method: 'POST',
        headers: adminHeaders,
        body: JSON.stringify({
            name: `PT Nusantara Niaga Digital ${uniqueSuffix}`,
            domain: `nusantara-${uniqueSuffix}.com`,
            support_email: `bantuan@nusantara-${uniqueSuffix}.com`,
            locale: 0
        })
    });
    if (!createAccRes.ok) throw new Error(`Gagal membuat tenant akun: ${createAccRes.status}`);
    const accCreated = await createAccRes.json();
    console.log(`-> Tenant Akun Baru Dibuat: ID ${accCreated.id} - ${accCreated.name} (Role: ${accCreated.role})`);

    // Update konfigurasi akun
    const updateAccRes = await fetch(`${BASE_URL}/api/v1/accounts/${accCreated.id}`, {
        method: 'PATCH',
        headers: adminHeaders,
        body: JSON.stringify({
            domain: `store-${uniqueSuffix}.id`,
            support_email: `cs@store-${uniqueSuffix}.id`
        })
    });
    if (!updateAccRes.ok) throw new Error(`Gagal update tenant: ${updateAccRes.status}`);
    const accUpdated = await updateAccRes.json();
    console.log(`-> Konfigurasi Akun Diperbarui: Domain: ${accUpdated.domain}, Email: ${accUpdated.support_email}`);

    // 4. UJI SUPER ADMIN DASHBOARD & PROVISIONING
    console.log("\n=== STEP 4: SUPER ADMIN DASHBOARD & GLOBAL PROVISIONING ===");
    const superHeaders = await getAuthHeaders('superadmin@chatwoot.dev', 'password123');
    console.log("-> Login berhasil sebagai SuperAdmin (Super Admin Antigravity)");

    // List semua akun global
    const superListAccRes = await fetch(`${BASE_URL}/super_admin/api/v1/accounts`, {
        headers: superHeaders
    });
    if (!superListAccRes.ok) throw new Error(`SuperAdmin gagal list akun: ${superListAccRes.status}`);
    const allAccounts = await superListAccRes.json();
    console.log(`-> SuperAdmin melihat total ${allAccounts.length} akun tenant di sistem:`);
    allAccounts.slice(0, 3).forEach(a => {
        console.log(`   - [ID ${a.id}] ${a.name} | ${a.users_count} users | ${a.conversations_count} percakapan`);
    });

    // SuperAdmin provision akun enterprise baru dengan admin
    const superCreateAccRes = await fetch(`${BASE_URL}/super_admin/api/v1/accounts`, {
        method: 'POST',
        headers: superHeaders,
        body: JSON.stringify({
            name: `Enterprise Bank Mandiri ${uniqueSuffix}`,
            domain: `mandiri-${uniqueSuffix}.co.id`,
            support_email: `prioritas@mandiri-${uniqueSuffix}.co.id`,
            admin_name: "Head of Digital Care",
            admin_email: `care.head.${uniqueSuffix}@mandiri.co.id`
        })
    });
    if (!superCreateAccRes.ok) throw new Error(`SuperAdmin gagal provision akun: ${superCreateAccRes.status}`);
    const enterpriseAcc = await superCreateAccRes.json();
    console.log(`-> SuperAdmin berhasil provision akun Enterprise: ID ${enterpriseAcc.id} - ${enterpriseAcc.name}`);

    // SuperAdmin seed data sample ke akun enterprise baru
    const seedRes = await fetch(`${BASE_URL}/super_admin/api/v1/accounts/${enterpriseAcc.id}/seed`, {
        method: 'POST',
        headers: superHeaders
    });
    if (!seedRes.ok) throw new Error(`SuperAdmin gagal seed akun: ${seedRes.status}`);
    const seedResult = await seedRes.json();
    console.log(`-> SuperAdmin Seeder Berhasil: "${seedResult.message}"`);

    // SuperAdmin list users global
    const superListUsersRes = await fetch(`${BASE_URL}/super_admin/api/v1/users`, {
        headers: superHeaders
    });
    const allUsers = await superListUsersRes.json();
    console.log(`-> SuperAdmin melihat total ${allUsers.length} pengguna terdaftar di seluruh sistem.`);

    console.log("\n================================================================================");
    console.log(">>> SEMUA TEST MULTI-TENANT, RBAC & SUPER ADMIN BERHASIL 100%! <<<");
    console.log("================================================================================\n");
}

main().catch(err => {
    console.error("\n❌ TEST FAILED:", err);
    process.exit(1);
});
