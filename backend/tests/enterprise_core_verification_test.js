const API_URL = process.env.API_URL || 'http://127.0.0.1:3000';

async function runTests() {
  console.log('===============================================================');
  console.log('   ENTERPRISE CORE RUST REWRITE: VERIFICATION TEST             ');
  console.log('===============================================================\n');

  // STEP 1: AUTHENTICATION
  console.log('=== STEP 1: AUTHENTICATION (Agent Login) ===');
  const loginRes = await fetch(`${API_URL}/auth/sign_in`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: 'john@acme.inc', password: 'Password123!' }),
  });

  if (!loginRes.ok) {
    throw new Error(`Login failed: ${loginRes.status} ${await loginRes.text()}`);
  }

  const token = loginRes.headers.get('access-token');
  const client = loginRes.headers.get('client');
  const uid = loginRes.headers.get('uid');
  console.log('✅ Agent login successful.');

  const headers = {
    'access-token': token,
    'client': client,
    'uid': uid,
    'Content-Type': 'application/json',
  };

  // STEP 2: SLA REPORTS API
  console.log('\n=== STEP 2: SLA REPORTS API ===');
  const slaListRes = await fetch(`${API_URL}/api/v1/accounts/1/applied_slas?page=1`, { headers });
  if (slaListRes.status === 200) {
    const data = await slaListRes.json();
    console.log(`✅ GET /applied_slas -> 200 OK (count: ${data.meta?.count ?? 0})`);
  } else {
    throw new Error(`GET /applied_slas failed: ${slaListRes.status}`);
  }

  const slaMetricsRes = await fetch(`${API_URL}/api/v1/accounts/1/applied_slas/metrics`, { headers });
  if (slaMetricsRes.status === 200) {
    const data = await slaMetricsRes.json();
    console.log(`✅ GET /applied_slas/metrics -> 200 OK (hit_rate: ${data.hit_rate ?? '0%'}, total: ${data.total_applied_slas ?? 0})`);
  } else {
    throw new Error(`GET /applied_slas/metrics failed: ${slaMetricsRes.status}`);
  }

  const slaDownloadRes = await fetch(`${API_URL}/api/v1/accounts/1/applied_slas/download`, { headers });
  if (slaDownloadRes.status === 200 && slaDownloadRes.headers.get('content-type')?.includes('text/csv')) {
    const csv = await slaDownloadRes.text();
    console.log(`✅ GET /applied_slas/download -> 200 OK (CSV rows: ${csv.split('\n').length})`);
  } else {
    throw new Error(`GET /applied_slas/download failed: ${slaDownloadRes.status}`);
  }

  // STEP 3: CAPTAIN TASKS (REWRITE, SUMMARIZE, REPLY SUGGESTION, FOLLOW UP)
  console.log('\n=== STEP 3: CAPTAIN INLINE AI TASKS ===');
  const rewriteRes = await fetch(`${API_URL}/api/v1/accounts/1/captain/tasks/rewrite`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      content: 'Halo kak sy mau tnya apakah produk ready ya?',
      operation: 'professional',
    }),
  });
  if (rewriteRes.status === 200) {
    const data = await rewriteRes.json();
    console.log(`✅ POST /captain/tasks/rewrite -> 200 OK: "${data.message?.slice(0, 50)}..."`);
  } else {
    throw new Error(`POST /captain/tasks/rewrite failed: ${rewriteRes.status}`);
  }

  const followUpRes = await fetch(`${API_URL}/api/v1/accounts/1/captain/tasks/follow_up`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      message: 'Persingkat lagi',
      follow_up_context: { operation: 'shorten' },
    }),
  });
  if (followUpRes.status === 200) {
    const data = await followUpRes.json();
    console.log(`✅ POST /captain/tasks/follow_up -> 200 OK: "${data.message?.slice(0, 50)}..."`);
  } else {
    throw new Error(`POST /captain/tasks/follow_up failed: ${followUpRes.status}`);
  }

  // STEP 4: CAPTAIN AGENT SESSIONS & COPILOT RUNS
  console.log('\n=== STEP 4: CAPTAIN SESSIONS & COPILOT RUNS ===');
  const sessionRes = await fetch(`${API_URL}/api/v1/accounts/1/captain/agent_sessions/1`, { headers });
  if (sessionRes.status === 200) {
    const data = await sessionRes.json();
    console.log(`✅ GET /captain/agent_sessions/:id -> 200 OK (citations: ${data.citations?.length ?? 0})`);
  } else {
    throw new Error(`GET /captain/agent_sessions/1 failed: ${sessionRes.status}`);
  }

  const approveRes = await fetch(`${API_URL}/api/v1/accounts/1/captain/copilot_runs/1/approve`, {
    method: 'POST',
    headers,
    body: JSON.stringify({ note: 'Approved by supervisor' }),
  });
  if (approveRes.status === 200) {
    const data = await approveRes.json();
    console.log(`✅ POST /captain/copilot_runs/:id/approve -> 200 OK (status: ${data.status})`);
  } else {
    throw new Error(`POST /captain/copilot_runs/1/approve failed: ${approveRes.status}`);
  }

  const rejectRes = await fetch(`${API_URL}/api/v1/accounts/1/captain/copilot_runs/1/reject`, {
    method: 'POST',
    headers,
    body: JSON.stringify({ reason: 'Not accurate enough' }),
  });
  if (rejectRes.status === 200) {
    const data = await rejectRes.json();
    console.log(`✅ POST /captain/copilot_runs/:id/reject -> 200 OK (status: ${data.status})`);
  } else {
    throw new Error(`POST /captain/copilot_runs/1/reject failed: ${rejectRes.status}`);
  }

  // STEP 5: MESSAGE TRANSLATION
  console.log('\n=== STEP 5: MESSAGE TRANSLATION ===');
  const autoTransRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/messages/auto_translate`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      message_ids: [1],
      target_language: 'id',
    }),
  });
  if (autoTransRes.status === 202 || autoTransRes.status === 200) {
    console.log(`✅ POST /conversations/:id/messages/auto_translate -> ${autoTransRes.status} ACCEPTED (async job enqueued)`);
  } else {
    throw new Error(`POST /conversations/1/messages/auto_translate failed: ${autoTransRes.status}`);
  }

  const singleTransRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/messages/1/translate`, {
    method: 'POST',
    headers,
    body: JSON.stringify({ target_language: 'en' }),
  });
  if (singleTransRes.status === 200) {
    const data = await singleTransRes.json();
    console.log(`✅ POST /conversations/:id/messages/:id/translate -> 200 OK`);
  } else {
    throw new Error(`POST /conversations/1/messages/1/translate failed: ${singleTransRes.status}`);
  }

  // STEP 6: CONVERSATION CAMPAIGN HISTORY
  console.log('\n=== STEP 6: CONVERSATION CAMPAIGN HISTORY ===');
  const campHistRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/campaign_history`, { headers });
  if (campHistRes.status === 200) {
    const data = await campHistRes.json();
    console.log(`✅ GET /conversations/:id/campaign_history -> 200 OK (campaigns: ${data.payload?.length ?? 0})`);
  } else {
    throw new Error(`GET /conversations/1/campaign_history failed: ${campHistRes.status}`);
  }

  // STEP 7: CSAT SURVEY REVIEW NOTES
  console.log('\n=== STEP 7: CSAT SURVEY REVIEW NOTES ===');
  const csatRes = await fetch(`${API_URL}/api/v1/accounts/1/csat_survey_responses/1`, {
    method: 'PATCH',
    headers,
    body: JSON.stringify({ review_notes: 'Supervisor verified: resolved customer query efficiently' }),
  });
  if (csatRes.status === 200) {
    const data = await csatRes.json();
    console.log(`✅ PATCH /csat_survey_responses/:id -> 200 OK (id: ${data.id})`);
  } else {
    throw new Error(`PATCH /csat_survey_responses/1 failed: ${csatRes.status}`);
  }

  // STEP 8: REPORTING EVENTS API
  console.log('\n=== STEP 8: REPORTING EVENTS API ===');
  const reportEventsRes = await fetch(`${API_URL}/api/v1/accounts/1/reporting_events`, { headers });
  if (reportEventsRes.status === 200) {
    const data = await reportEventsRes.json();
    console.log(`✅ GET /reporting_events -> 200 OK (count: ${data.meta?.count ?? 0})`);
  } else {
    throw new Error(`GET /reporting_events failed: ${reportEventsRes.status}`);
  }

  const convReportEventsRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/reporting_events`, { headers });
  if (convReportEventsRes.status === 200) {
    const data = await convReportEventsRes.json();
    console.log(`✅ GET /conversations/:id/reporting_events -> 200 OK (events: ${data.payload?.length ?? 0})`);
  } else {
    throw new Error(`GET /conversations/1/reporting_events failed: ${convReportEventsRes.status}`);
  }

  console.log('\n===============================================================');
  console.log('   🎉 ALL 8 ENTERPRISE FEATURES VERIFIED SUCCESSFULLY!         ');
  console.log('===============================================================\n');
}

runTests().catch((err) => {
  console.error('\n❌ Test execution failed:', err);
  process.exit(1);
});
