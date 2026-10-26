// Comprehensive End-to-End Integration Test for Minggu 3 (Meta WhatsApp Cloud API & Real-Time Sync)
const crypto = require('crypto');

const API_URL = 'http://127.0.0.1:3333';
const WS_URL = 'ws://127.0.0.1:3333/cable';
const WHATSAPP_PHONE = '%2B6281234567890'; // +6281234567890
const APP_SECRET = 'test_app_secret_123';
const VERIFY_TOKEN = 'chatwoot_wa_verify_token_123';

function computeSignature(secret, body) {
  const hmac = crypto.createHmac('sha256', secret);
  hmac.update(body);
  return 'sha256=' + hmac.digest('hex');
}

async function runTests() {
  console.log('====================================================');
  console.log('   MINGGU 3: WHATSAPP CLOUD API INTEGRATION TEST    ');
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

  // STEP 2: ACTIONCABLE WEBSOCKET CONNECTION
  console.log('\n=== STEP 2: ACTIONCABLE WEBSOCKET CONNECTION ===');
  const ws = new WebSocket(WS_URL);
  const receivedEvents = [];

  await new Promise((resolve, reject) => {
    ws.onopen = () => console.log('[WS] Connected to /cable');
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
        console.log('[WS EVENT]', msg.message.event, msg.message.data?.id || msg.message.data?.content || '');
        receivedEvents.push(msg.message);
      }
    };
    ws.onerror = reject;
    setTimeout(() => reject(new Error('WS connection timeout')), 5000);
  });

  // STEP 3: WEBHOOK HANDSHAKE VERIFICATION (GET)
  console.log('\n=== STEP 3: META WEBHOOK HANDSHAKE VERIFICATION (GET) ===');
  
  // 3a. Matching token
  const challenge = 'challenge_test_' + Date.now();
  const verifyRes = await fetch(`${API_URL}/webhooks/whatsapp/${WHATSAPP_PHONE}?hub.mode=subscribe&hub.challenge=${challenge}&hub.verify_token=${VERIFY_TOKEN}`);
  if (!verifyRes.ok) {
    throw new Error(`Webhook handshake failed: ${verifyRes.status}`);
  }
  const verifyBody = await verifyRes.text();
  if (verifyBody !== challenge) {
    throw new Error(`Expected challenge "${challenge}", got "${verifyBody}"`);
  }
  console.log('Handshake with valid verify_token PASSED! Challenge matched.');

  // 3b. Wrong token
  const wrongTokenRes = await fetch(`${API_URL}/webhooks/whatsapp/${WHATSAPP_PHONE}?hub.mode=subscribe&hub.challenge=${challenge}&hub.verify_token=wrong_token`);
  if (wrongTokenRes.status !== 401) {
    throw new Error(`Expected 401 for wrong token, got ${wrongTokenRes.status}`);
  }
  console.log('Handshake with wrong token rejected with 401 PASSED.');

  // 3c. Wrong mode
  const wrongModeRes = await fetch(`${API_URL}/webhooks/whatsapp/${WHATSAPP_PHONE}?hub.mode=unsubscribe&hub.challenge=${challenge}&hub.verify_token=${VERIFY_TOKEN}`);
  if (wrongModeRes.status !== 400) {
    throw new Error(`Expected 400 for wrong mode, got ${wrongModeRes.status}`);
  }
  console.log('Handshake with invalid mode rejected with 400 PASSED.');

  // STEP 4: INBOUND MESSAGE WITH HMAC SIGNATURE (POST)
  console.log('\n=== STEP 4: INBOUND MESSAGE VIA WEBHOOK (POST) ===');
  const wamid = 'wamid.TEST_' + Date.now();
  const inboundPayload = JSON.stringify({
    object: 'whatsapp_business_account',
    entry: [
      {
        id: '900800700600',
        changes: [
          {
            field: 'messages',
            value: {
              messaging_product: 'whatsapp',
              metadata: {
                display_phone_number: '6281234567890',
                phone_number_id: '100200300400500',
              },
              contacts: [
                {
                  profile: { name: 'Budi Santoso' },
                  wa_id: '6289876543210',
                },
              ],
              messages: [
                {
                  from: '6289876543210',
                  id: wamid,
                  timestamp: Math.floor(Date.now() / 1000).toString(),
                  type: 'text',
                  text: { body: 'Halo CS Chatwoot, saya butuh konfirmasi pesanan saya.' },
                },
              ],
            },
          },
        ],
      },
    ],
  });

  // 4a. Test invalid HMAC signature
  const invalidSigRes = await fetch(`${API_URL}/webhooks/whatsapp/${WHATSAPP_PHONE}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': 'sha256=invalidhex0123456789abcdef0123456789abcdef0123456789abcdef01234567',
    },
    body: inboundPayload,
  });
  if (invalidSigRes.status !== 401) {
    throw new Error(`Expected 401 for invalid signature, got ${invalidSigRes.status}`);
  }
  console.log('Invalid HMAC signature rejected with 401 PASSED.');

  // 4b. Test valid HMAC signature
  const validSig = computeSignature(APP_SECRET, inboundPayload);
  const validPostRes = await fetch(`${API_URL}/webhooks/whatsapp/${WHATSAPP_PHONE}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': validSig,
    },
    body: inboundPayload,
  });
  if (!validPostRes.ok) {
    throw new Error(`Inbound webhook post failed: ${validPostRes.status} ${await validPostRes.text()}`);
  }
  console.log('Inbound message webhook accepted with 200 OK PASSED.');

  // Wait 500ms for async processing & WS broadcast
  await new Promise((r) => setTimeout(r, 600));

  // STEP 5: VERIFY DATABASE PERSISTENCE VIA REST API
  console.log('\n=== STEP 5: VERIFY CONTACT & CONVERSATION IN REST API ===');
  
  // 5a. Contact check
  const contactsRes = await fetch(`${API_URL}/api/v1/accounts/1/contacts?search=6289876543210`, {
    headers: authHeaders,
  });
  const contactsData = await contactsRes.json();
  const contact = contactsData.payload.find((c) => c.phone_number?.includes('6289876543210'));
  if (!contact) {
    throw new Error('Contact Budi Santoso not found in contacts API');
  }
  console.log('Contact created in DB and verified:', { id: contact.id, name: contact.name, phone: contact.phone_number });

  // 5b. Conversation check
  const convsRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations`, {
    headers: authHeaders,
  });
  const convsData = await convsRes.json();
  const conv = convsData.data.payload.find((c) => c.meta.sender?.phone_number?.includes('6289876543210'));
  if (!conv) {
    throw new Error('WhatsApp conversation not found in conversations API');
  }
  console.log('Conversation created in DB and verified:', { display_id: conv.id, status: conv.status, channel: conv.meta.channel });

  // STEP 6: VERIFY ACTIONCABLE REALTIME BROADCAST
  console.log('\n=== STEP 6: VERIFY ACTIONCABLE EVENT (Realtime Pop-up) ===');
  const msgCreatedEvent = receivedEvents.find(
    (e) => e.event === 'message.created' && e.data?.content?.includes('konfirmasi pesanan')
  );
  if (!msgCreatedEvent) {
    throw new Error('ActionCable did not receive message.created event for inbound message');
  }
  console.log('ActionCable message.created received in realtime!', {
    id: msgCreatedEvent.data.id,
    content: msgCreatedEvent.data.content,
    sender: msgCreatedEvent.data.sender?.name,
  });

  // STEP 7: TEST MESSAGE DEDUPLICATION (IDEMPOTENCY)
  console.log('\n=== STEP 7: TEST MESSAGE DEDUPLICATION ===');
  const duplicateRes = await fetch(`${API_URL}/webhooks/whatsapp/${WHATSAPP_PHONE}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': validSig,
    },
    body: inboundPayload,
  });
  if (!duplicateRes.ok) {
    throw new Error(`Duplicate webhook post failed: ${duplicateRes.status}`);
  }
  console.log('Duplicate message webhook returned 200 OK.');

  const messagesRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${conv.id}/messages`, {
    headers: authHeaders,
  });
  const messagesList = await messagesRes.json();
  const duplicateMatches = messagesList.payload.filter((m) => m.source_id === wamid);
  if (duplicateMatches.length !== 1) {
    throw new Error(`Deduplication failed! Expected 1 message with source_id ${wamid}, found ${duplicateMatches.length}`);
  }
  console.log('Deduplication verified! Exactly 1 message stored in database.');

  // STEP 8: INBOUND STATUS UPDATE (DELIVERY & READ RECEIPTS)
  console.log('\n=== STEP 8: INBOUND STATUS UPDATES (Delivered & Read) ===');
  const deliveredPayload = JSON.stringify({
    object: 'whatsapp_business_account',
    entry: [
      {
        id: '900800700600',
        changes: [
          {
            field: 'messages',
            value: {
              messaging_product: 'whatsapp',
              metadata: {
                display_phone_number: '6281234567890',
                phone_number_id: '100200300400500',
              },
              statuses: [
                {
                  id: wamid,
                  status: 'delivered',
                  timestamp: Math.floor(Date.now() / 1000).toString(),
                  recipient_id: '6289876543210',
                },
              ],
            },
          },
        ],
      },
    ],
  });

  const deliveredSig = computeSignature(APP_SECRET, deliveredPayload);
  await fetch(`${API_URL}/webhooks/whatsapp/${WHATSAPP_PHONE}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': deliveredSig,
    },
    body: deliveredPayload,
  });

  await new Promise((r) => setTimeout(r, 400));
  const deliveredEvent = receivedEvents.find((e) => e.event === 'message.updated' && e.data?.status === 'delivered');
  if (!deliveredEvent) {
    throw new Error('ActionCable did not receive message.updated with status delivered');
  }
  console.log('Delivery receipt processed and broadcasted via ActionCable PASSED!');

  // STEP 9: OUTBOUND AGENT MESSAGE TO WHATSAPP
  console.log('\n=== STEP 9: OUTBOUND AGENT MESSAGE TO WHATSAPP ===');
  const outboundRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${conv.id}/messages`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      content: 'Halo Pak Budi, pesanan nomor #12345 sudah kami proses dan dikirim.',
    }),
  });

  if (!outboundRes.ok) {
    throw new Error(`Outbound message post failed: ${outboundRes.status} ${await outboundRes.text()}`);
  }
  const outboundData = await outboundRes.json();
  console.log('Outbound message created successfully:', { id: outboundData.id, content: outboundData.content });

  // Wait 500ms for async outbound dispatcher & WS broadcast
  await new Promise((r) => setTimeout(r, 600));

  const outboundEvent = receivedEvents.find((e) => e.event === 'message.created' && e.data?.content?.includes('#12345'));
  if (!outboundEvent) {
    throw new Error('ActionCable did not receive outbound message.created');
  }
  console.log('ActionCable received outbound message.created in realtime PASSED!');

  ws.close();
  console.log('\n====================================================');
  console.log('   ALL 9 MINGGU 3 INTEGRATION TESTS PASSED 100%!    ');
  console.log('====================================================\n');
}

runTests().catch((err) => {
  console.error('\nTEST FAILED:', err);
  process.exit(1);
});
