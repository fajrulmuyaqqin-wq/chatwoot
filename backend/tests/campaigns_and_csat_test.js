// End-to-End Integration Test for Outbound Campaigns & CSAT Survey System
const API_URL = 'http://127.0.0.1:3333';
const WS_URL = 'ws://127.0.0.1:3333/cable';

async function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runCampaignsAndCsatTests() {
  console.log('================================================================');
  console.log('     OUTBOUND CAMPAIGNS & CSAT SURVEY SYSTEM TEST SUITE         ');
  console.log('================================================================\n');

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

  // STEP 2: ACTIONCABLE WEBSOCKET CONNECTION
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
        console.log(`[WS Event Received] -> ${msg.message.event}`);
        receivedEvents.push(msg.message);
      }
    };
    ws.onerror = (err) => reject(err);
    setTimeout(() => resolve(), 2000);
  });

  // STEP 3: ONGOING & ONE-OFF CAMPAIGNS CRUD
  console.log('\n=== STEP 3: CAMPAIGNS CRUD ===');
  // 1. Create Ongoing Campaign (e.g. Website URL greeting)
  const ongoingPayload = {
    title: 'Website Greeting URL Promo',
    description: 'Ongoing trigger when visitor views /promo page',
    message: 'Halo! Selamat datang di Toko Kami, ada yang bisa kami bantu seputar promo hari ini?',
    inbox_id: 1,
    campaign_type: 'ongoing',
    trigger_rules: {
      url: 'https://myshop.co.id/promo',
      time_on_page: 10,
    },
    enabled: true,
  };

  const createOngoingRes = await fetch(`${API_URL}/api/v1/accounts/1/campaigns`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify(ongoingPayload),
  });

  if (!createOngoingRes.ok) {
    throw new Error(`Failed to create ongoing campaign: ${createOngoingRes.status} ${await createOngoingRes.text()}`);
  }

  const ongoingCamp = await createOngoingRes.json();
  console.log(`Created Ongoing Campaign ID: ${ongoingCamp.id}, Type: ${ongoingCamp.campaign_type}, Status: ${ongoingCamp.campaign_status}`);
  if (ongoingCamp.campaign_type !== 'ongoing') {
    throw new Error('Expected campaign_type to be ongoing');
  }

  // 2. Create One-off Broadcast Blast Campaign
  const oneoffPayload = {
    title: 'Flash Sale Broadcast Blast 2026',
    description: 'Broadcast massal diskon gajian 50%',
    message: 'PROMO SPESIAL: Gunakan kode GAJIAN50 untuk diskon 50% di checkout hari ini!',
    inbox_id: 1,
    campaign_type: 'one_off',
    enabled: true,
  };

  const createOneoffRes = await fetch(`${API_URL}/api/v1/accounts/1/campaigns`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify(oneoffPayload),
  });

  if (!createOneoffRes.ok) {
    throw new Error(`Failed to create one-off campaign: ${createOneoffRes.status} ${await createOneoffRes.text()}`);
  }

  const oneoffCamp = await createOneoffRes.json();
  console.log(`Created One-off Campaign ID: ${oneoffCamp.id}, Type: ${oneoffCamp.campaign_type}, Status: ${oneoffCamp.campaign_status}`);
  if (oneoffCamp.campaign_type !== 'one_off') {
    throw new Error('Expected campaign_type to be one_off');
  }

  // List all campaigns
  const listCampRes = await fetch(`${API_URL}/api/v1/accounts/1/campaigns`, {
    headers: authHeaders,
  });
  const listCamps = await listCampRes.json();
  console.log(`Total campaigns found in account: ${listCamps.length}`);
  if (listCamps.length < 2) {
    throw new Error('Expected at least 2 campaigns in account list');
  }

  // Get single campaign
  const getCampRes = await fetch(`${API_URL}/api/v1/accounts/1/campaigns/${oneoffCamp.id}`, {
    headers: authHeaders,
  });
  const singleCamp = await getCampRes.json();
  if (singleCamp.title !== oneoffPayload.title) {
    throw new Error('Campaign title mismatch on get_campaign');
  }
  console.log('Verified get_campaign details.');

  // Update campaign
  const updateCampRes = await fetch(`${API_URL}/api/v1/accounts/1/campaigns/${oneoffCamp.id}`, {
    method: 'PATCH',
    headers: authHeaders,
    body: JSON.stringify({ description: 'Updated Broadcast Description 2026' }),
  });
  const updatedCamp = await updateCampRes.json();
  if (updatedCamp.description !== 'Updated Broadcast Description 2026') {
    throw new Error('Update campaign description failed');
  }
  console.log('Verified update_campaign.');

  // STEP 4: TRIGGER ONE-OFF BROADCAST BLAST ENGINE
  console.log('\n=== STEP 4: TRIGGER BROADCAST BLAST ENGINE ===');
  console.log(`Triggering broadcast blast for Campaign ID: ${oneoffCamp.id}...`);
  const blastRes = await fetch(`${API_URL}/api/v1/accounts/1/campaigns/${oneoffCamp.id}/trigger`, {
    method: 'POST',
    headers: authHeaders,
  });

  if (!blastRes.ok) {
    throw new Error(`Failed to trigger broadcast blast: ${blastRes.status} ${await blastRes.text()}`);
  }

  const blastData = await blastRes.json();
  console.log(`Broadcast trigger response: ${JSON.stringify(blastData)}`);

  console.log('Waiting for Tokio async broadcast execution to complete...');
  await sleep(1000);

  // Verifikasi status campaign berubah menjadi 'completed' dan completed_at terisi
  const checkCampRes = await fetch(`${API_URL}/api/v1/accounts/1/campaigns/${oneoffCamp.id}`, {
    headers: authHeaders,
  });
  const completedCamp = await checkCampRes.json();
  console.log(`Campaign status after blast: ${completedCamp.campaign_status}, completed_at: ${completedCamp.completed_at}`);
  if (completedCamp.campaign_status !== 'completed') {
    throw new Error(`Expected campaign status to be completed, got: ${completedCamp.campaign_status}`);
  }
  if (!completedCamp.completed_at) {
    throw new Error('Expected campaign completed_at to be non-null');
  }
  console.log('Broadcast blast execution completed successfully!');

  // STEP 5: CSAT SURVEY SYSTEM & METRICS
  console.log('\n=== STEP 5: CSAT SURVEY SYSTEM & METRICS ===');

  // 1. Submit CSAT Survey Rating 5 Bintang
  console.log('Submitting customer CSAT 5-star rating...');
  const csatSubmitRes = await fetch(`${API_URL}/api/v1/accounts/1/csat_survey_responses`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      rating: 5,
      feedback_message: 'Pelayanan CS sangat memuaskan, respons ramah dan secepat kilat!',
    }),
  });

  if (!csatSubmitRes.ok) {
    throw new Error(`Failed to submit CSAT response: ${csatSubmitRes.status} ${await csatSubmitRes.text()}`);
  }

  const csatResp = await csatSubmitRes.json();
  console.log(`CSAT Response recorded: ID=${csatResp.id}, rating=${csatResp.rating}, feedback="${csatResp.feedback_message}"`);
  if (csatResp.rating !== 5) {
    throw new Error(`Expected rating to be 5, got: ${csatResp.rating}`);
  }

  // 2. List CSAT responses
  const listCsatRes = await fetch(`${API_URL}/api/v1/accounts/1/csat_survey_responses`, {
    headers: authHeaders,
  });
  const listCsat = await listCsatRes.json();
  console.log(`CSAT survey responses count: ${listCsat.length}`);
  if (listCsat.length === 0) {
    throw new Error('Expected at least 1 CSAT response in list');
  }

  // 3. Get CSAT Metrics aggregation
  const csatMetricsRes = await fetch(`${API_URL}/api/v1/accounts/1/csat_survey_responses/metrics`, {
    headers: authHeaders,
  });
  const csatMetrics = await csatMetricsRes.json();
  console.log(`CSAT Metrics: Total=${csatMetrics.total_count}, Ratings Breakdown=${JSON.stringify(csatMetrics.ratings_count)}, Sent Count=${csatMetrics.total_sent_messages_count}`);
  if (csatMetrics.total_count < 1) {
    throw new Error('Expected csatMetrics.total_count >= 1');
  }
  if (!csatMetrics.ratings_count['5'] || csatMetrics.ratings_count['5'] < 1) {
    throw new Error('Expected csatMetrics.ratings_count for rating 5 to be >= 1');
  }

  // 4. Download CSAT CSV Report
  console.log('Testing CSAT CSV Export download...');
  const csvRes = await fetch(`${API_URL}/api/v1/accounts/1/csat_survey_responses/download`, {
    headers: authHeaders,
  });
  if (!csvRes.ok) {
    throw new Error(`Failed to download CSAT CSV: ${csvRes.status}`);
  }
  const csvText = await csvRes.text();
  console.log(`CSV report header & preview:\n${csvText.trim().split('\n').slice(0, 3).join('\n')}`);
  if (!csvText.startsWith('id,rating,feedback_message')) {
    throw new Error('Invalid CSV header format');
  }

  // STEP 6: CLEANUP
  console.log('\n=== STEP 6: CLEANUP ===');
  await fetch(`${API_URL}/api/v1/accounts/1/campaigns/${ongoingCamp.id}`, {
    method: 'DELETE',
    headers: authHeaders,
  });
  await fetch(`${API_URL}/api/v1/accounts/1/campaigns/${oneoffCamp.id}`, {
    method: 'DELETE',
    headers: authHeaders,
  });
  console.log('Cleaned up test campaigns.');

  ws.close();
  console.log('\n================================================================');
  console.log('   CAMPAIGNS & CSAT SURVEY SYSTEM TEST PASSED 100%!             ');
  console.log('================================================================\n');
}

runCampaignsAndCsatTests().catch((err) => {
  console.error('\nTEST FAILED:', err);
  process.exit(1);
});
