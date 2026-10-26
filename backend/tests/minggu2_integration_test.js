// Comprehensive End-to-End Integration Test for Minggu 2
const API_URL = 'http://127.0.0.1:3333';
const WS_URL = 'ws://127.0.0.1:3333/cable';

async function runTests() {
  console.log('=== STEP 1: AUTHENTICATION ===');
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
  console.log('Login success! Devise Headers:', { token, client, uid });

  const authHeaders = {
    'access-token': token,
    'client': client,
    'uid': uid,
    'Content-Type': 'application/json',
  };

  console.log('\n=== STEP 2: WEBSOCKET ACTIONCABLE SETUP ===');
  const ws = new WebSocket(WS_URL);
  const receivedEvents = [];

  await new Promise((resolve, reject) => {
    ws.onopen = () => {
      console.log('[WS] Connected to /cable');
    };
    ws.onmessage = (e) => {
      const msg = JSON.parse(e.data);
      if (msg.type === 'welcome') {
        console.log('[WS] Welcome received. Subscribing to RoomChannel...');
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
        console.log('[WS EVENT]', msg.message.event, JSON.stringify(msg.message.data));
        receivedEvents.push(msg.message.event);
      }
    };
    ws.onerror = reject;
    setTimeout(() => reject(new Error('WS subscription timeout')), 5000);
  });

  console.log('\n=== STEP 3: GET INBOXES ===');
  const inboxesRes = await fetch(`${API_URL}/api/v1/accounts/1/inboxes`, { headers: authHeaders });
  const inboxesData = await inboxesRes.json();
  console.log('Inboxes count:', inboxesData.payload.length);
  if (inboxesData.payload.length === 0 || inboxesData.payload[0].name !== 'Support Inbox') {
    throw new Error('Inboxes payload invalid');
  }

  console.log('\n=== STEP 4: GET CONTACTS ===');
  const contactsRes = await fetch(`${API_URL}/api/v1/accounts/1/contacts`, { headers: authHeaders });
  const contactsData = await contactsRes.json();
  console.log('Contacts meta:', contactsData.meta);
  const sarah = contactsData.payload.find((c) => c.name === 'Sarah Connor');
  console.log('Found contact:', sarah?.name);
  if (!sarah) {
    throw new Error('Contact Sarah Connor not found');
  }

  console.log('\n=== STEP 5: GET CONVERSATIONS ===');
  const convsRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations`, { headers: authHeaders });
  const convsData = await convsRes.json();
  console.log('Conversations meta counts:', convsData.data.meta);
  console.log('Conversations count:', convsData.data.payload.length);
  if (convsData.data.payload.length === 0) {
    throw new Error('Conversations payload empty');
  }

  console.log('\n=== STEP 6: GET MESSAGES ===');
  const msgsRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/messages`, { headers: authHeaders });
  const msgsData = await msgsRes.json();
  console.log('Messages count in conversation 1:', msgsData.payload.length);
  console.log('First message content:', msgsData.payload[0]?.content);

  console.log('\n=== STEP 7: CREATE OUTGOING MESSAGE & VERIFY WEBSOCKET BROADCAST ===');
  const newMsgContent = `Hello Sarah, this is a test reply at ${Date.now()}`;
  const sendRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/messages`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      content: newMsgContent,
      private: false,
    }),
  });
  const sendData = await sendRes.json();
  console.log('Message created ID:', sendData.id, 'content:', sendData.content);

  // Wait a moment for WS event
  await new Promise((r) => setTimeout(r, 800));
  if (!receivedEvents.includes('message.created')) {
    throw new Error('ActionCable did not receive message.created event!');
  }
  console.log('CONFIRMED: message.created received via WebSocket!');

  console.log('\n=== STEP 8: TOGGLE CONVERSATION STATUS ===');
  const toggleRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/toggle_status`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({ status: 'resolved' }),
  });
  const toggleData = await toggleRes.json();
  console.log('Toggle status result:', toggleData.payload);
  if (toggleData.payload.current_status !== 'resolved') {
    throw new Error('Toggle status failed');
  }

  await new Promise((r) => setTimeout(r, 800));
  if (!receivedEvents.includes('conversation.status_changed')) {
    throw new Error('ActionCable did not receive conversation.status_changed event!');
  }
  console.log('CONFIRMED: conversation.status_changed received via WebSocket!');

  console.log('\n=== STEP 9: FILE DIRECT UPLOAD & STATIC SERVE ===');
  const formData = new FormData();
  const fileBlob = new Blob(['Hello Chatwoot File Attachment Content!'], { type: 'text/plain' });
  formData.append('file', fileBlob, 'sample_attachment.txt');

  const uploadRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/1/direct_uploads`, {
    method: 'POST',
    headers: {
      'access-token': token,
      'client': client,
      'uid': uid,
    },
    body: formData,
  });
  const uploadData = await uploadRes.json();
  console.log('Direct upload response:', uploadData);
  if (!uploadData.file_url) {
    throw new Error('Direct upload failed to return file_url');
  }

  // Verify static serve
  const staticFileRes = await fetch(`${API_URL}${uploadData.file_url}`);
  const staticText = await staticFileRes.text();
  console.log('Downloaded uploaded file content:', staticText);
  if (staticText !== 'Hello Chatwoot File Attachment Content!') {
    throw new Error('Static file download content mismatch');
  }

  ws.close();
  console.log('\n=========================================');
  console.log('>>> ALL MINGGU 2 TESTS PASSED 100%! <<<');
  console.log('=========================================');
}

runTests().catch((err) => {
  console.error('\n[TEST FAILED]', err);
  process.exit(1);
});
