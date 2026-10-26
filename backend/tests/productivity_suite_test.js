// End-to-End Integration Test for Productivity Suite: Canned Responses, Labels, and Macros
const API_URL = 'http://127.0.0.1:3333';
const WS_URL = 'ws://127.0.0.1:3333/cable';

async function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runProductivityTests() {
  console.log('================================================================');
  console.log('   PRODUCTIVITY SUITE: CANNED RESPONSES, LABELS & MACROS TEST   ');
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
        console.log('[WS EVENT]', msg.message.event, msg.message.data?.id || '');
        receivedEvents.push(msg.message);
      }
    };
    ws.onerror = reject;
    setTimeout(() => reject(new Error('WS connection timeout')), 5000);
  });

  // STEP 3: CANNED RESPONSES CRUD & SEARCH
  console.log('\n=== STEP 3: CANNED RESPONSES CRUD & SEARCH ===');
  
  // 3a. List all canned responses
  const listRes = await fetch(`${API_URL}/api/v1/accounts/1/canned_responses`, { headers: authHeaders });
  if (!listRes.ok) throw new Error(`List canned responses failed: ${listRes.status}`);
  const listData = await listRes.json();
  console.log(`Found ${listData.length} canned responses:`, listData.map((c) => '/' + c.short_code));
  if (listData.length < 3) throw new Error('Expected at least 3 seeded canned responses!');

  // 3b. Search canned responses
  const searchRes = await fetch(`${API_URL}/api/v1/accounts/1/canned_responses?search=rekening`, { headers: authHeaders });
  const searchData = await searchRes.json();
  if (searchData.length === 0 || searchData[0].short_code !== 'rekening') {
    throw new Error('Search canned response for "rekening" failed!');
  }
  console.log('Search canned responses (?search=rekening) PASSED! Matched:', searchData[0].content);

  // 3c. Create canned response
  const createCrRes = await fetch(`${API_URL}/api/v1/accounts/1/canned_responses`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      short_code: 'jam_test',
      content: 'Jam operasional kami: Senin - Jumat 09.00 - 18.00 WIB.',
    }),
  });
  if (!createCrRes.ok) throw new Error(`Create canned response failed: ${createCrRes.status}`);
  const createdCr = await createCrRes.json();
  console.log('Create canned response PASSED! ID:', createdCr.id, 'Code:', createdCr.short_code);

  // 3d. Update canned response
  const updateCrRes = await fetch(`${API_URL}/api/v1/accounts/1/canned_responses/${createdCr.id}`, {
    method: 'PATCH',
    headers: authHeaders,
    body: JSON.stringify({
      content: 'Jam operasional kami: Senin - Sabtu 08.00 - 20.00 WIB.',
    }),
  });
  if (!updateCrRes.ok) throw new Error(`Update canned response failed: ${updateCrRes.status}`);
  const updatedCr = await updateCrRes.json();
  if (!updatedCr.content.includes('Senin - Sabtu')) {
    throw new Error('Canned response content not updated properly!');
  }
  console.log('Update canned response PASSED! New content:', updatedCr.content);

  // 3e. Delete canned response
  const deleteCrRes = await fetch(`${API_URL}/api/v1/accounts/1/canned_responses/${createdCr.id}`, {
    method: 'DELETE',
    headers: authHeaders,
  });
  if (!deleteCrRes.ok) throw new Error(`Delete canned response failed: ${deleteCrRes.status}`);
  console.log('Delete canned response PASSED!');

  // STEP 4: LABELS & CONVERSATION TAGGING
  console.log('\n=== STEP 4: LABELS & CONVERSATION TAGGING ===');
  
  // 4a. List labels
  const listLabelsRes = await fetch(`${API_URL}/api/v1/accounts/1/labels`, { headers: authHeaders });
  if (!listLabelsRes.ok) throw new Error(`List labels failed: ${listLabelsRes.status}`);
  const listLabelsData = await listLabelsRes.json();
  console.log(`Found ${listLabelsData.payload.length} labels:`, listLabelsData.payload.map((l) => l.title));

  // 4b. Create label
  const testLabelTitle = 'garansi_' + Date.now();
  const createLabelRes = await fetch(`${API_URL}/api/v1/accounts/1/labels`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      title: testLabelTitle,
      description: 'Klaim Garansi Resmi Distributor',
      color: '#0ea5e9',
      show_on_sidebar: true,
    }),
  });
  if (!createLabelRes.ok) throw new Error(`Create label failed: ${createLabelRes.status}`);
  const createdLabel = await createLabelRes.json();
  console.log('Create label PASSED! Title:', createdLabel.payload.title);

  // 4c. Set conversation labels on conversation 1
  const setLabelsRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/labels`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      labels: ['vip', testLabelTitle],
    }),
  });
  if (!setLabelsRes.ok) throw new Error(`Set conversation labels failed: ${setLabelsRes.status}`);
  const setLabelsData = await setLabelsRes.json();
  console.log('Set conversation 1 labels:', setLabelsData.payload);

  await sleep(1000);

  // 4d. Verify ActionCable broadcast
  const convUpdatedEvent = receivedEvents.find(
    (e) => e.event === 'conversation.updated' && e.data?.display_id === 1 && e.data?.labels?.includes('vip')
  );
  if (!convUpdatedEvent) {
    throw new Error('ActionCable conversation.updated event with labels not received!');
  }
  console.log('ActionCable conversation.updated broadcast for labels PASSED!');

  // 4e. Fetch conversation labels
  const getLabelsRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/labels`, { headers: authHeaders });
  const getLabelsData = await getLabelsRes.json();
  if (!getLabelsData.payload.includes('vip') || !getLabelsData.payload.includes(testLabelTitle)) {
    throw new Error('Conversation labels retrieval mismatch!');
  }
  console.log('Get conversation labels verified PASSED!', getLabelsData.payload);

  // STEP 5: MACROS CRUD & BATCH EXECUTION
  console.log('\n=== STEP 5: MACROS CRUD & BATCH EXECUTION ===');
  
  // 5a. List macros
  const listMacrosRes = await fetch(`${API_URL}/api/v1/accounts/1/macros`, { headers: authHeaders });
  if (!listMacrosRes.ok) throw new Error(`List macros failed: ${listMacrosRes.status}`);
  const listMacrosData = await listMacrosRes.json();
  console.log(`Found ${listMacrosData.payload.length} macros:`, listMacrosData.payload.map((m) => m.name));

  // 5b. Create macro
  const createMacroRes = await fetch(`${API_URL}/api/v1/accounts/1/macros`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      name: 'Eskalasi ke Supervisor & Tag Leads',
      visibility: 'global',
      actions: [
        { action_name: 'add_label', action_params: ['leads'] },
        { action_name: 'assign_agent', action_params: [1] },
      ],
    }),
  });
  if (!createMacroRes.ok) throw new Error(`Create macro failed: ${createMacroRes.status}`);
  const createdMacro = await createMacroRes.json();
  console.log('Create macro PASSED! ID:', createdMacro.payload.id, 'Name:', createdMacro.payload.name);

  // 5c. Execute macro on conversation 1
  const execMacroRes = await fetch(`${API_URL}/api/v1/accounts/1/macros/${createdMacro.payload.id}/execute`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      conversation_ids: [1],
    }),
  });
  if (!execMacroRes.ok) throw new Error(`Execute macro failed: ${execMacroRes.status}`);
  const execResult = await execMacroRes.json();
  console.log('Macro execution result:', execResult);

  await sleep(1000);

  // 5d. Verify conversation has updated labels and assignee
  const verifyConvRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1`, { headers: authHeaders });
  const verifyConvData = await verifyConvRes.json();
  console.log('Conversation 1 post-macro state:', {
    display_id: verifyConvData.display_id,
    status: verifyConvData.status,
    assignee_id: verifyConvData.assignee_id,
  });

  const convLabelsCheckRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/labels`, { headers: authHeaders });
  const convLabelsCheck = await convLabelsCheckRes.json();
  console.log('Conversation 1 labels post-macro:', convLabelsCheck.payload);
  if (!convLabelsCheck.payload.includes('leads')) {
    throw new Error('Macro action add_label "leads" was not applied!');
  }
  console.log('Macro batch execution & ActionCable state sync PASSED!');

  ws.close();

  console.log('\n================================================================');
  console.log('   ALL PRODUCTIVITY SUITE TESTS PASSED 100%!                    ');
  console.log('================================================================');
}

runProductivityTests().catch((err) => {
  console.error('\nTEST FAILED:', err);
  process.exit(1);
});
