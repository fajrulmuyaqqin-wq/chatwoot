const http = require('http');

function request(options, bodyData) {
  return new Promise((resolve, reject) => {
    const req = http.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => {
        try {
          resolve({
            statusCode: res.statusCode,
            headers: res.headers,
            body: data ? JSON.parse(data) : {},
          });
        } catch (e) {
          resolve({
            statusCode: res.statusCode,
            headers: res.headers,
            rawBody: data,
          });
        }
      });
    });
    req.on('error', reject);
    if (bodyData) {
      req.write(typeof bodyData === 'string' ? bodyData : JSON.stringify(bodyData));
    }
    req.end();
  });
}

async function login(email, password) {
  const payload = JSON.stringify({ email, password });
  const res = await request(
    {
      hostname: 'localhost',
      port: 3000,
      path: '/auth/sign_in',
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(payload),
      },
    },
    payload
  );
  if (res.statusCode !== 200) {
    throw new Error(`Login failed for ${email}: ${res.statusCode} ${JSON.stringify(res.body)}`);
  }
  return {
    'access-token': res.headers['access-token'],
    client: res.headers['client'],
    uid: res.headers['uid'],
  };
}

async function main() {
  console.log('=== MULTI-TENANT TIER & CHAT ADMIN VERIFICATION TEST ===\n');

  // 1. Login John (Admin of Account 1)
  console.log('1. Logging in as john@acme.inc (Admin Account 1)...');
  const authHeaders1 = await login('john@acme.inc', 'Password123!');
  console.log('   -> Logged in successfully. Client:', authHeaders1.client);

  // 2. Query available tiers catalog and current tier
  console.log('\n2. Querying Tier info (GET /api/v1/accounts/1/tier)...');
  const tierRes = await request({
    hostname: 'localhost',
    port: 3000,
    path: '/api/v1/accounts/1/tier',
    method: 'GET',
    headers: { ...authHeaders1, 'Content-Type': 'application/json' },
  });
  console.log('   -> Status:', tierRes.statusCode);
  console.log('   -> Current Tier:', tierRes.body.current_tier);
  console.log('   -> Is Registered:', tierRes.body.is_registered);
  console.log('   -> Available Tiers count:', tierRes.body.available_tiers?.length);
  if (tierRes.statusCode !== 200 || !tierRes.body.available_tiers) {
    throw new Error('Tier catalog query failed');
  }

  // 3. Test Tier Selection: Free
  console.log('\n3. Testing Tier Selection: "free"');
  let setRes = await request(
    {
      hostname: 'localhost',
      port: 3000,
      path: '/api/v1/accounts/1/tier',
      method: 'POST',
      headers: { ...authHeaders1, 'Content-Type': 'application/json' },
    },
    { tier: 'free' }
  );
  console.log('   -> Status:', setRes.statusCode);
  console.log('   -> Returned Tier:', setRes.body.tier);
  console.log('   -> Agent limit:', setRes.body.limits?.agents);
  console.log('   -> Inbox limit:', setRes.body.limits?.inboxes);
  if (setRes.body.limits?.agents !== 2 || setRes.body.limits?.inboxes !== 2) {
    throw new Error('Limits mismatch for free tier');
  }

  // 4. Test Tier Selection: Small
  console.log('\n4. Testing Tier Selection: "small"');
  setRes = await request(
    {
      hostname: 'localhost',
      port: 3000,
      path: '/api/v1/accounts/1/tier',
      method: 'POST',
      headers: { ...authHeaders1, 'Content-Type': 'application/json' },
    },
    { tier: 'small' }
  );
  console.log('   -> Status:', setRes.statusCode);
  console.log('   -> Returned Tier:', setRes.body.tier);
  console.log('   -> Agent limit:', setRes.body.limits?.agents);
  console.log('   -> Inbox limit:', setRes.body.limits?.inboxes);
  if (setRes.body.limits?.agents !== 5 || setRes.body.limits?.inboxes !== 5) {
    throw new Error('Limits mismatch for small tier');
  }

  // 5. Test Tier Selection: Max
  console.log('\n5. Testing Tier Selection: "max"');
  setRes = await request(
    {
      hostname: 'localhost',
      port: 3000,
      path: '/api/v1/accounts/1/tier',
      method: 'POST',
      headers: { ...authHeaders1, 'Content-Type': 'application/json' },
    },
    { tier: 'max' }
  );
  console.log('   -> Status:', setRes.statusCode);
  console.log('   -> Returned Tier:', setRes.body.tier);
  console.log('   -> Agent limit:', setRes.body.limits?.agents);
  console.log('   -> Inbox limit:', setRes.body.limits?.inboxes);
  if (setRes.body.limits?.agents !== 15 || setRes.body.limits?.inboxes !== 20) {
    throw new Error('Limits mismatch for max tier');
  }

  // 6. Test Tier Selection: Enterprise (Chat Admin)
  console.log('\n6. Testing Tier Selection: "enterprise" (Chat Admin)');
  setRes = await request(
    {
      hostname: 'localhost',
      port: 3000,
      path: '/api/v1/accounts/1/tier',
      method: 'POST',
      headers: { ...authHeaders1, 'Content-Type': 'application/json' },
    },
    { tier: 'enterprise' }
  );
  console.log('   -> Status:', setRes.statusCode);
  console.log('   -> Returned Tier:', setRes.body.tier);
  console.log('   -> Badge:', setRes.body.badge);
  console.log('   -> Agent limit:', setRes.body.limits?.agents);
  console.log('   -> Inbox limit:', setRes.body.limits?.inboxes);
  if (setRes.body.limits?.agents !== -1 || setRes.body.badge !== 'Chat Admin') {
    throw new Error('Limits mismatch for enterprise tier');
  }

  // 7. Verify Dynamic Feature Flags on GET /api/v1/accounts/1
  console.log('\n7. Verifying dynamic feature flags on Account 1 (GET /api/v1/accounts/1)...');
  const acc1Res = await request({
    hostname: 'localhost',
    port: 3000,
    path: '/api/v1/accounts/1',
    method: 'GET',
    headers: { ...authHeaders1, 'Content-Type': 'application/json' },
  });
  console.log('   -> Status:', acc1Res.statusCode);
  console.log('   -> features.chat_admin:', acc1Res.body.features?.chat_admin);
  console.log('   -> features.custom_roles:', acc1Res.body.features?.custom_roles);
  console.log('   -> features.audit_logs:', acc1Res.body.features?.audit_logs);
  console.log('   -> custom_attributes.plan_name:', acc1Res.body.custom_attributes?.plan_name);
  if (acc1Res.body.features?.chat_admin !== true) {
    throw new Error('chat_admin feature flag should be true for Enterprise');
  }

  // 8. Multi-Tenant Isolation: Login Alex (Account 3)
  console.log('\n8. Multi-Tenant Isolation: Logging in as alex@nexus.org (Account 3)...');
  const authHeaders3 = await login('alex@nexus.org', 'Password123!');
  console.log('   -> Logged in successfully.');

  const acc3Tier = await request({
    hostname: 'localhost',
    port: 3000,
    path: '/api/v1/accounts/3/tier',
    method: 'GET',
    headers: { ...authHeaders3, 'Content-Type': 'application/json' },
  });
  console.log('   -> Account 3 Tier:', acc3Tier.body.current_tier);
  console.log('   -> Account 3 is_registered:', acc3Tier.body.is_registered);

  // Set Account 3 to "small"
  await request(
    {
      hostname: 'localhost',
      port: 3000,
      path: '/api/v1/accounts/3/tier',
      method: 'POST',
      headers: { ...authHeaders3, 'Content-Type': 'application/json' },
    },
    { tier: 'small' }
  );

  const acc3Res = await request({
    hostname: 'localhost',
    port: 3000,
    path: '/api/v1/accounts/3',
    method: 'GET',
    headers: { ...authHeaders3, 'Content-Type': 'application/json' },
  });
  console.log('   -> Account 3 plan:', acc3Res.body.custom_attributes?.plan_name);
  console.log('   -> Account 3 features.chat_admin:', acc3Res.body.features?.chat_admin);
  console.log('   -> Account 3 limits.agents:', acc3Res.body.limits?.agents);

  // Re-verify Account 1 is STILL Enterprise
  const acc1Check = await request({
    hostname: 'localhost',
    port: 3000,
    path: '/api/v1/accounts/1',
    method: 'GET',
    headers: { ...authHeaders1, 'Content-Type': 'application/json' },
  });
  console.log('   -> Account 1 re-check plan:', acc1Check.body.custom_attributes?.plan_name);
  console.log('   -> Account 1 features.chat_admin:', acc1Check.body.features?.chat_admin);
  if (acc1Check.body.custom_attributes?.plan_name !== 'enterprise' || acc3Res.body.custom_attributes?.plan_name !== 'small') {
    throw new Error('Tenant isolation violated! Tiers leaked across accounts.');
  }

  // 9. RBAC Test: Non-member or unauthorized access
  console.log('\n9. RBAC Security: Alex (Account 3) attempting to change Account 1 tier...');
  const tamperRes = await request(
    {
      hostname: 'localhost',
      port: 3000,
      path: '/api/v1/accounts/1/tier',
      method: 'POST',
      headers: { ...authHeaders3, 'Content-Type': 'application/json' },
    },
    { tier: 'free' }
  );
  console.log('   -> Status:', tamperRes.statusCode);
  if (tamperRes.statusCode !== 401 && tamperRes.statusCode !== 403) {
    throw new Error(`Tampering succeeded! Expected 401/403, got ${tamperRes.statusCode}`);
  }
  console.log('   -> Tampering correctly blocked by RBAC!');

  console.log('\n>>> ALL 9 MULTI-TENANT TIER & CHAT ADMIN VERIFICATIONS PASSED SUCCESSFULLY! <<<\n');
}

main().catch((err) => {
  console.error('\n[TEST FAILED]:', err);
  process.exit(1);
});
