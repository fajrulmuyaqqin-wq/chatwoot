// Multi-Tenant Architecture & Embedded Signup End-to-End Verification Test
const crypto = require('crypto');
const { execSync } = require('child_process');

const API_URL = 'http://127.0.0.1:3000';
const APP_SECRET = 'test_app_secret_123';
const VERIFY_TOKEN = 'chatwoot_wa_verify_token_123';

function computeSignature(secret, body) {
  const hmac = crypto.createHmac('sha256', secret);
  hmac.update(body);
  return 'sha256=' + hmac.digest('hex');
}

function queryDb(sql) {
  const cleanSql = sql.replace(/"/g, '\\"');
  const out = execSync(`docker exec boson-db psql -U postgres -d chatwoot_dev -t -A -c "${cleanSql}"`, {
    encoding: 'utf-8',
  });
  return out.trim();
}

async function runMultiTenantTests() {
  console.log('================================================================');
  console.log('   MULTI-TENANT EMBEDDED SIGNUP & SECURITY END-TO-END SUITE     ');
  console.log('================================================================\n');

  // 1. TENANT 1 AUTHENTICATION (john@example.com -> Account 1)
  console.log('--- TEST 1: Tenant 1 Authentication (Account 1) ---');
  const t1Res = await fetch(`${API_URL}/auth/sign_in`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: 'john@example.com', password: 'password123' }),
  });
  if (!t1Res.ok) throw new Error(`Tenant 1 login failed: ${t1Res.status} ${await t1Res.text()}`);
  const t1Headers = {
    'access-token': t1Res.headers.get('access-token'),
    'client': t1Res.headers.get('client'),
    'uid': t1Res.headers.get('uid'),
    'Content-Type': 'application/json',
  };
  console.log('✓ Tenant 1 authenticated successfully');

  // 2. TENANT 2 AUTHENTICATION (care.head...@mandiri.co.id -> Account 3)
  console.log('\n--- TEST 2: Tenant 2 Authentication (Account 3) ---');
  const t2Res = await fetch(`${API_URL}/auth/sign_in`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: 'care.head.1792834943645@mandiri.co.id', password: 'password123' }),
  });
  if (!t2Res.ok) throw new Error(`Tenant 2 login failed: ${t2Res.status} ${await t2Res.text()}`);
  const t2Headers = {
    'access-token': t2Res.headers.get('access-token'),
    'client': t2Res.headers.get('client'),
    'uid': t2Res.headers.get('uid'),
    'Content-Type': 'application/json',
  };
  console.log('✓ Tenant 2 authenticated successfully');

  // 3. BOLA / IDOR CROSS-TENANT ISOLATION CHECK
  console.log('\n--- TEST 3: BOLA / IDOR Cross-Tenant Access Rejection ---');
  // Tenant 2 attempts to list Tenant 1's inboxes
  const bolaInboxRes = await fetch(`${API_URL}/api/v1/accounts/1/inboxes`, {
    headers: t2Headers,
  });
  console.log(`Tenant 2 -> Tenant 1 Inboxes: Status ${bolaInboxRes.status}`);
  if (bolaInboxRes.status !== 403) {
    throw new Error(`Expected 403 Forbidden for cross-tenant access, got ${bolaInboxRes.status}`);
  }
  const bolaInboxBody = await bolaInboxRes.json();
  console.log(`✓ BOLA Rejection verified: "${bolaInboxBody.error}"`);

  // Tenant 2 attempts to call Tenant 1 WhatsApp Authorization endpoint
  const bolaAuthRes = await fetch(`${API_URL}/api/v1/accounts/1/whatsapp/authorization`, {
    method: 'POST',
    headers: t2Headers,
    body: JSON.stringify({
      code: 'mock_code',
      waba_id: 'mock_waba',
    }),
  });
  console.log(`Tenant 2 -> Tenant 1 WA Auth: Status ${bolaAuthRes.status}`);
  if (bolaAuthRes.status !== 403) {
    throw new Error(`Expected 403 Forbidden for cross-tenant auth, got ${bolaAuthRes.status}`);
  }
  console.log('✓ Cross-tenant WhatsApp authorization attempt blocked with 403 Forbidden');

  // 4. TENANT 1 EMBEDDED SIGNUP AUTHORIZATION (AES-256-GCM Token Encryption)
  console.log('\n--- TEST 4: Tenant 1 WhatsApp Authorization with Token Encryption ---');
  const t1Phone = '+6281111111111';
  const t1Pid = 'pid_tenant_1';
  const t1Waba = 'waba_tenant_1';

  // Clean old test channel if any
  queryDb(`DELETE FROM channel_whatsapp WHERE phone_number = '${t1Phone}' OR phone_number = '+6282222222222'`);

  const t1AuthRes = await fetch(`${API_URL}/api/v1/accounts/1/whatsapp/authorization`, {
    method: 'POST',
    headers: t1Headers,
    body: JSON.stringify({
      code: 'mock_auth_code_tenant_1',
      waba_id: t1Waba,
      phone_number_id: t1Pid,
      phone_number: t1Phone,
      business_id: 'biz_tenant_1',
    }),
  });
  if (!t1AuthRes.ok) {
    throw new Error(`Tenant 1 WhatsApp authorization failed: ${t1AuthRes.status} ${await t1AuthRes.text()}`);
  }
  const t1AuthData = await t1AuthRes.json();
  console.log('✓ Tenant 1 Channel & Inbox authorized successfully:');
  console.log(`  Inbox ID: ${t1AuthData.id}, Name: "${t1AuthData.name}", Phone: ${t1AuthData.phone_number}`);

  // Verify in PostgreSQL that access token is stored ENCRYPTED with enc:
  const t1DbToken = queryDb(`SELECT business_management_token FROM channel_whatsapp WHERE phone_number = '${t1Phone}'`);
  const t1DbConfig = queryDb(`SELECT provider_config->>'api_key' FROM channel_whatsapp WHERE phone_number = '${t1Phone}'`);
  console.log(`  Database stored business_management_token: ${t1DbToken.substring(0, 20)}...`);
  console.log(`  Database stored provider_config.api_key: ${t1DbConfig.substring(0, 20)}...`);
  if (!t1DbToken.startsWith('enc:') || !t1DbConfig.startsWith('enc:')) {
    throw new Error('Expected tokens in PostgreSQL to be encrypted with AES-256-GCM (enc: prefix)!');
  }
  console.log('✓ AES-256-GCM symmetric token encryption verified in PostgreSQL storage!');

  // 5. CROSS-TENANT PHONE NUMBER HIJACKING PROTECTION
  console.log('\n--- TEST 5: Cross-Tenant Phone Hijacking Protection ---');
  // Tenant 2 attempts to register Tenant 1's phone number
  const hijackRes = await fetch(`${API_URL}/api/v1/accounts/3/whatsapp/authorization`, {
    method: 'POST',
    headers: t2Headers,
    body: JSON.stringify({
      code: 'mock_code_hijack',
      waba_id: 'waba_evil',
      phone_number_id: 'pid_evil',
      phone_number: t1Phone, // Target Tenant 1's number!
    }),
  });
  console.log(`Tenant 2 attempting to claim Tenant 1 phone: Status ${hijackRes.status}`);
  if (hijackRes.status !== 422) {
    throw new Error(`Expected 422 Unprocessable Entity for phone hijacking, got ${hijackRes.status}`);
  }
  const hijackBody = await hijackRes.json();
  console.log(`✓ Hijacking blocked: "${hijackBody.error}"`);

  // 6. TENANT 2 LEGITIMATE EMBEDDED SIGNUP AUTHORIZATION
  console.log('\n--- TEST 6: Tenant 2 Legitimate WhatsApp Authorization ---');
  const t2Phone = '+6282222222222';
  const t2Pid = 'pid_tenant_2';
  const t2Waba = 'waba_tenant_2';

  const t2AuthRes = await fetch(`${API_URL}/api/v1/accounts/3/whatsapp/authorization`, {
    method: 'POST',
    headers: t2Headers,
    body: JSON.stringify({
      code: 'mock_auth_code_tenant_2',
      waba_id: t2Waba,
      phone_number_id: t2Pid,
      phone_number: t2Phone,
      business_id: 'biz_tenant_2',
    }),
  });
  if (!t2AuthRes.ok) {
    throw new Error(`Tenant 2 WhatsApp authorization failed: ${t2AuthRes.status} ${await t2AuthRes.text()}`);
  }
  const t2AuthData = await t2AuthRes.json();
  console.log('✓ Tenant 2 Channel & Inbox authorized successfully:');
  console.log(`  Inbox ID: ${t2AuthData.id}, Name: "${t2AuthData.name}", Phone: ${t2AuthData.phone_number}`);

  // 7. GLOBAL CENTRALIZED WEBHOOK HANDSHAKE
  console.log('\n--- TEST 7: Centralized Global Webhook Handshake ---');
  const handshakeRes = await fetch(
    `${API_URL}/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=${VERIFY_TOKEN}&hub.challenge=test_multi_tenant_challenge_999`
  );
  if (!handshakeRes.ok) throw new Error(`Handshake failed: ${handshakeRes.status}`);
  const challenge = await handshakeRes.text();
  if (challenge !== 'test_multi_tenant_challenge_999') {
    throw new Error(`Challenge mismatch: expected test_multi_tenant_challenge_999, got ${challenge}`);
  }
  console.log('✓ Global Webhook Handshake verified successfully (200 OK)');

  // 8. CENTRALIZED WEBHOOK MULTI-TENANT MESSAGE INGESTION & ROUTING
  console.log('\n--- TEST 8: Centralized Webhook Dynamic Multi-Tenant Ingestion ---');
  
  // 8a. Incoming message for Tenant 1
  const t1WebhookPayload = {
    object: 'whatsapp_business_account',
    entry: [
      {
        id: t1Waba,
        changes: [
          {
            field: 'messages',
            value: {
              messaging_product: 'whatsapp',
              metadata: {
                display_phone_number: t1Phone,
                phone_number_id: t1Pid,
              },
              contacts: [{ profile: { name: 'Customer Tenant 1' }, wa_id: '62899990001' }],
              messages: [
                {
                  from: '62899990001',
                  id: `wamid.HBgL_${Date.now()}_t1`,
                  timestamp: Math.floor(Date.now() / 1000).toString(),
                  text: { body: 'Halo Tenant 1, saya ingin order produk digital!' },
                  type: 'text',
                },
              ],
            },
          },
        ],
      },
    ],
  };

  const t1RawBody = JSON.stringify(t1WebhookPayload);
  const t1Sig = computeSignature(APP_SECRET, t1RawBody);
  const t1PostRes = await fetch(`${API_URL}/webhooks/whatsapp`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': t1Sig,
    },
    body: t1RawBody,
  });
  if (!t1PostRes.ok) throw new Error(`Tenant 1 Webhook failed: ${t1PostRes.status}`);
  console.log('✓ Ingested message for Tenant 1 via global webhook');

  // Verify DB record for Tenant 1
  const t1MessageAccount = queryDb(
    `SELECT account_id FROM messages WHERE content LIKE '%Halo Tenant 1%' ORDER BY id DESC LIMIT 1`
  );
  console.log(`  Message Account ID in DB: ${t1MessageAccount} (Expected: 1)`);
  if (t1MessageAccount !== '1') {
    throw new Error(`Expected message to be routed to Account 1, but got Account ${t1MessageAccount}`);
  }

  // 8b. Incoming message for Tenant 2
  const t2WebhookPayload = {
    object: 'whatsapp_business_account',
    entry: [
      {
        id: t2Waba,
        changes: [
          {
            field: 'messages',
            value: {
              messaging_product: 'whatsapp',
              metadata: {
                display_phone_number: t2Phone,
                phone_number_id: t2Pid,
              },
              contacts: [{ profile: { name: 'Customer Tenant 2' }, wa_id: '62899990002' }],
              messages: [
                {
                  from: '62899990002',
                  id: `wamid.HBgL_${Date.now()}_t2`,
                  timestamp: Math.floor(Date.now() / 1000).toString(),
                  text: { body: 'Halo Tenant 2, saya butuh bantuan perbankan!' },
                  type: 'text',
                },
              ],
            },
          },
        ],
      },
    ],
  };

  const t2RawBody = JSON.stringify(t2WebhookPayload);
  const t2Sig = computeSignature(APP_SECRET, t2RawBody);
  const t2PostRes = await fetch(`${API_URL}/webhooks/whatsapp`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': t2Sig,
    },
    body: t2RawBody,
  });
  if (!t2PostRes.ok) throw new Error(`Tenant 2 Webhook failed: ${t2PostRes.status}`);
  console.log('✓ Ingested message for Tenant 2 via global webhook');

  // Verify DB record for Tenant 2
  const t2MessageAccount = queryDb(
    `SELECT account_id FROM messages WHERE content LIKE '%Halo Tenant 2%' ORDER BY id DESC LIMIT 1`
  );
  console.log(`  Message Account ID in DB: ${t2MessageAccount} (Expected: 3)`);
  if (t2MessageAccount !== '3') {
    throw new Error(`Expected message to be routed to Account 3, but got Account ${t2MessageAccount}`);
  }

  console.log('\n================================================================');
  console.log('   🎉 ALL MULTI-TENANT TESTS & SECURITY VERIFICATIONS PASSED!    ');
  console.log('================================================================');
}

runMultiTenantTests().catch((err) => {
  console.error('\n❌ Test execution failed:', err);
  process.exit(1);
});
