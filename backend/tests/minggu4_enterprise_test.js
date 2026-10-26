// Comprehensive End-to-End Integration Test for Minggu 4 (Enterprise Features: Audit, SLA, AI/RAG)
const API_URL = 'http://127.0.0.1:3333';
const WS_URL = 'ws://127.0.0.1:3333/cable';

async function runTests() {
  console.log('====================================================');
  console.log('   MINGGU 4: ENTERPRISE FEATURES INTEGRATION TEST   ');
  console.log('====================================================\n');

  // STEP 1: AUTHENTICATION
  console.log('=== STEP 1: AUTHENTICATION (Agent Login) ===');
  const loginRes = await fetch(`${API_URL}/auth/sign_in`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: 'john@example.com', password: 'password123' }),
  });

  if (!loginRes.ok) {
    throw new Error(`Login failed: ${loginRes.status} ${await loginRes.text()}`);
  }

  const token = loginRes.headers.get('access-token');
  const client = loginRes.headers.get('client');
  const uid = loginRes.headers.get('uid');
  console.log('Agent login successful. Devise headers obtained.');

  const authHeaders = {
    'access-token': token,
    'client': client,
    'uid': uid,
    'Content-Type': 'application/json',
  };

  // STEP 2: WEBSOCKET ACTIONCABLE SETUP
  console.log('\n=== STEP 2: ACTIONCABLE WEBSOCKET CONNECTION ===');
  const ws = new WebSocket(WS_URL);
  const receivedEvents = [];

  await new Promise((resolve, reject) => {
    ws.onopen = () => console.log('[WS] Connected to /cable');
    ws.onmessage = (e) => {
      const msg = JSON.parse(e.data);
      if (msg.type === 'welcome') {
        ws.send(JSON.stringify({
          command: 'subscribe',
          identifier: JSON.stringify({
            channel: 'RoomChannel',
            pubsub_token: 'sample_pubsub_token_john',
            account_id: 1,
            user_id: 1,
          }),
        }));
      } else if (msg.type === 'confirm_subscription') {
        console.log('[WS] Subscription confirmed!');
        resolve();
      } else if (msg.message && msg.message.event) {
        console.log('[WS EVENT]', msg.message.event, msg.message.data?.event_type || '');
        receivedEvents.push(msg.message);
      }
    };
    ws.onerror = reject;
    setTimeout(() => reject(new Error('WS timeout')), 5000);
  });

  // STEP 3: KNOWLEDGE BASE ARTICLES & CAPTAIN AI REPLY SUGGESTIONS
  console.log('\n=== STEP 3: KNOWLEDGE BASE & AI REPLY SUGGESTIONS ===');
  
  // 3a. Create a new article
  const articleRes = await fetch(`${API_URL}/api/v1/accounts/1/articles`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      title: 'Panduan Pengiriman dan Ekspedisi',
      description: 'Estimasi waktu pengiriman barang reguler 2-3 hari kerja.',
      content: 'Kami bekerja sama dengan JNE dan SiCepat. Resi akan otomatis diupdate di dashboard pemesanan.',
    }),
  });
  if (!articleRes.ok) {
    throw new Error(`Create article failed: ${articleRes.status}`);
  }
  const createdArticle = await articleRes.json();
  console.log('Article created successfully:', { id: createdArticle.id, title: createdArticle.title });

  // 3b. List articles
  const listArticlesRes = await fetch(`${API_URL}/api/v1/accounts/1/articles`, {
    headers: authHeaders,
  });
  const articlesList = await listArticlesRes.json();
  console.log('Articles list count:', articlesList.payload.length);
  if (articlesList.payload.length < 2) {
    throw new Error('Articles list incomplete');
  }

  // 3c. Request AI Reply Suggestion based on customer question
  const aiRes = await fetch(`${API_URL}/api/v1/accounts/1/ai/reply_suggestions`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      message: 'Halo, bagaimana prosedur jika saya ingin refund barang rusak?',
    }),
  });
  if (!aiRes.ok) {
    throw new Error(`AI suggestion failed: ${aiRes.status}`);
  }
  const aiData = await aiRes.json();
  console.log('AI Reply Suggestion generated:');
  console.log('----------------------------------------------------');
  console.log(aiData.reply_suggestion);
  console.log('----------------------------------------------------');
  if (!aiData.reply_suggestion || aiData.reply_suggestion.length < 10) {
    throw new Error('Invalid AI reply suggestion');
  }

  // STEP 4: AUDIT LOG MIDDLEWARE VERIFICATION
  console.log('\n=== STEP 4: AUDIT LOG MIDDLEWARE VERIFICATION ===');
  // Wait 400ms for async audit insert
  await new Promise((r) => setTimeout(r, 400));

  const auditRes = await fetch(`${API_URL}/api/v1/accounts/1/audit_logs`, {
    headers: authHeaders,
  });
  if (!auditRes.ok) {
    throw new Error(`Get audit logs failed: ${auditRes.status}`);
  }
  const auditData = await auditRes.json();
  console.log('Total audit logs recorded:', auditData.meta.count);
  const articleAudit = auditData.payload.find((a) => a.auditable_type === 'Article');
  if (!articleAudit) {
    throw new Error('Audit log for Article creation was not recorded');
  }
  console.log('Audit log successfully captured:', {
    action: articleAudit.action,
    username: articleAudit.username,
    auditable_type: articleAudit.auditable_type,
  });

  // STEP 5: SLA POLICY & REAL-TIME EVALUATOR BREACH
  console.log('\n=== STEP 5: SLA POLICY & REAL-TIME EVALUATOR BREACH ===');
  
  // 5a. List SLA policies
  const slasRes = await fetch(`${API_URL}/api/v1/accounts/1/sla_policies`, {
    headers: authHeaders,
  });
  const slasData = await slasRes.json();
  const fastSla = slasData.payload.find((p) => p.name.includes('Fast Response'));
  if (!fastSla) {
    throw new Error('Fast Response SLA policy not found');
  }
  console.log('Found SLA policy:', { id: fastSla.id, name: fastSla.name, threshold: fastSla.first_response_time_threshold });

  // 5b. Apply SLA policy to conversation 2
  const applyRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/2/apply_sla`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({ sla_policy_id: fastSla.id }),
  });
  if (!applyRes.ok) {
    throw new Error(`Apply SLA failed: ${applyRes.status} ${await applyRes.text()}`);
  }
  console.log('SLA successfully applied to conversation 2.');

  // 5c. Wait for Tokio background SLA evaluator to detect breach (threshold is 2 seconds, wait 4s)
  console.log('Waiting for SLA evaluator background worker to detect breach...');
  await new Promise((r) => setTimeout(r, 4000));

  const slaBreachedEvent = receivedEvents.find((e) => e.event === 'sla.breached');
  if (!slaBreachedEvent) {
    throw new Error('ActionCable did not receive sla.breached event from background evaluator');
  }
  console.log('ActionCable received sla.breached in realtime!', slaBreachedEvent.data);

  ws.close();
  console.log('\n====================================================');
  console.log('   ALL MINGGU 4 ENTERPRISE TESTS PASSED 100%!       ');
  console.log('====================================================\n');
}

runTests().catch((err) => {
  console.error('\nTEST FAILED:', err);
  process.exit(1);
});
