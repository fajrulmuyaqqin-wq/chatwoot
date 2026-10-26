// End-to-End Integration Test: Meta Instagram Direct Message Channel (Sprint 1)
const crypto = require('crypto');

const API_URL = 'http://127.0.0.1:3333';
const WS_URL = 'ws://127.0.0.1:3333/cable';
const INSTAGRAM_ACCOUNT_ID = '17841400000000001';
const VERIFY_TOKEN = 'chatwoot_instagram_token';
const APP_SECRET = 'ig_app_secret_indonesia_2026';

function computeSignature(secret, body) {
  const hmac = crypto.createHmac('sha256', secret);
  hmac.update(body);
  return 'sha256=' + hmac.digest('hex');
}

async function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runInstagramTests() {
  console.log('====================================================');
  console.log('   SPRINT 1: META INSTAGRAM DIRECT MESSAGE TEST     ');
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
  const challenge = 'challenge_ig_' + Date.now();
  const verifyRes = await fetch(`${API_URL}/webhooks/instagram?hub.mode=subscribe&hub.challenge=${challenge}&hub.verify_token=${VERIFY_TOKEN}`);
  if (!verifyRes.ok) {
    throw new Error(`Instagram webhook handshake failed: ${verifyRes.status}`);
  }
  const verifyBody = await verifyRes.text();
  if (verifyBody !== challenge) {
    throw new Error(`Expected challenge "${challenge}", got "${verifyBody}"`);
  }
  console.log('Handshake with valid verify_token PASSED! Challenge returned correctly.');

  // 3b. Wrong token
  const wrongTokenRes = await fetch(`${API_URL}/webhooks/instagram?hub.mode=subscribe&hub.challenge=${challenge}&hub.verify_token=wrong_ig_token`);
  if (wrongTokenRes.status !== 401) {
    throw new Error(`Expected 401 for wrong token, got ${wrongTokenRes.status}`);
  }
  console.log('Handshake with wrong token rejected with 401 PASSED.');

  // 3c. Wrong mode
  const wrongModeRes = await fetch(`${API_URL}/webhooks/instagram?hub.mode=unsubscribe&hub.challenge=${challenge}&hub.verify_token=${VERIFY_TOKEN}`);
  if (wrongModeRes.status !== 400) {
    throw new Error(`Expected 400 for wrong mode, got ${wrongModeRes.status}`);
  }
  console.log('Handshake with wrong mode rejected with 400 PASSED.');

  // STEP 4: INBOUND INSTAGRAM DIRECT MESSAGE WITH HMAC-SHA256
  console.log('\n=== STEP 4: INBOUND INSTAGRAM DIRECT MESSAGE WITH HMAC-SHA256 ===');
  const testCustomerIgsid = '1784149999888877';
  const testMid = 'mid.IG_INBOUND_' + Date.now();
  const testText = 'Halo kak, apakah sepatu sneakers ukuran 42 warna hitam masih ready stock?';

  const inboundPayload = {
    object: 'instagram',
    entry: [
      {
        id: INSTAGRAM_ACCOUNT_ID,
        time: Math.floor(Date.now() / 1000),
        messaging: [
          {
            sender: {
              id: testCustomerIgsid,
            },
            recipient: {
              id: INSTAGRAM_ACCOUNT_ID,
            },
            timestamp: Date.now(),
            message: {
              mid: testMid,
              text: testText,
            },
          },
        ],
      },
    ],
  };

  const payloadString = JSON.stringify(inboundPayload);
  const signature = computeSignature(APP_SECRET, payloadString);

  const inboundRes = await fetch(`${API_URL}/webhooks/instagram`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': signature,
    },
    body: payloadString,
  });

  if (!inboundRes.ok) {
    throw new Error(`Inbound Instagram webhook failed: ${inboundRes.status} ${await inboundRes.text()}`);
  }

  const inboundResult = await inboundRes.json();
  console.log('Inbound Instagram webhook accepted:', inboundResult);

  // Tunggu broadcast ActionCable
  await sleep(1000);

  const msgEvent = receivedEvents.find(
    (e) => e.event === 'message.created' && e.data?.source_id === testMid
  );
  if (!msgEvent) {
    throw new Error(`Expected ActionCable message.created for mid ${testMid}, but none received!`);
  }
  console.log('ActionCable message.created received for Instagram message PASSED!');
  console.log('Message Content:', msgEvent.data.content);
  console.log('Conversation Display ID:', msgEvent.data.conversation_id);

  const convDisplayId = msgEvent.data.conversation_id;

  // STEP 5: DEDUPLICATION CHECK
  console.log('\n=== STEP 5: DEDUPLICATION CHECK ===');
  // Resend the exact same webhook payload with identical testMid
  const dupRes = await fetch(`${API_URL}/webhooks/instagram`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': signature,
    },
    body: payloadString,
  });

  if (!dupRes.ok) {
    throw new Error(`Duplicate webhook request failed: ${dupRes.status}`);
  }

  // Fetch messages from conversation via REST API
  const listMsgRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${convDisplayId}/messages`, {
    headers: authHeaders,
  });
  const listMsgData = await listMsgRes.json();
  const duplicateMatches = listMsgData.payload.filter((m) => m.source_id === testMid);
  if (duplicateMatches.length !== 1) {
    throw new Error(`Expected exactly 1 message with mid ${testMid}, but found ${duplicateMatches.length}! Deduplication failed.`);
  }
  console.log(`Deduplication verified: exactly 1 message exists for mid ${testMid} PASSED!`);

  // STEP 6: AGENT OUTBOUND REPLY VIA INSTAGRAM CHANNEL
  console.log('\n=== STEP 6: AGENT OUTBOUND REPLY VIA INSTAGRAM CHANNEL ===');
  const replyContent = 'Halo kak! Iya sepatu sneakers ukuran 42 warna hitam masih ready stock ya, siap kirim hari ini!';
  
  const sendRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${convDisplayId}/messages`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      content: replyContent,
      message_type: 'outgoing',
    }),
  });

  if (!sendRes.ok) {
    throw new Error(`Agent reply failed: ${sendRes.status} ${await sendRes.text()}`);
  }

  const sendResult = await sendRes.json();
  console.log('Agent reply posted via API:', sendResult.content);

  // Tunggu background task mengirim ke Instagram Graph API & update status
  await sleep(1500);

  // Verifikasi pesan di database sudah memiliki source_id (mid.IG_MOCK_...)
  const checkReplyRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${convDisplayId}/messages`, {
    headers: authHeaders,
  });
  const checkReplyData = await checkReplyRes.json();
  const outgoingMsg = checkReplyData.payload.find((m) => m.content === replyContent);

  if (!outgoingMsg) {
    throw new Error('Outgoing message not found in conversation messages!');
  }

  console.log('Outgoing Instagram message record:', {
    id: outgoingMsg.id,
    content: outgoingMsg.content,
    status: outgoingMsg.status,
    source_id: outgoingMsg.source_id,
  });

  if (!outgoingMsg.source_id || !outgoingMsg.source_id.startsWith('mid.IG_')) {
    throw new Error(`Expected source_id starting with mid.IG_, got ${outgoingMsg.source_id}`);
  }
  console.log('Instagram Outbound auto-dispatch & message_id update PASSED!');

  // Clean up
  ws.close();

  console.log('\n====================================================');
  console.log('   ALL SPRINT 1 INSTAGRAM TESTS PASSED 100%!        ');
  console.log('====================================================');
}

runInstagramTests().catch((err) => {
  console.error('\nTEST FAILED:', err);
  process.exit(1);
});
