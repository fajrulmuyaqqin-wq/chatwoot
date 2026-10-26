// End-to-End Integration Test: Facebook Messenger, Telegram Bot, and Shopee Chat (Indonesia Channels)
const crypto = require('crypto');

const API_URL = 'http://127.0.0.1:3333';
const WS_URL = 'ws://127.0.0.1:3333/cable';

const FB_PAGE_ID = '109876543210987';
const FB_VERIFY_TOKEN = 'chatwoot_fb_token';
const FB_APP_SECRET = 'fb_app_secret_indonesia_2026';

const TG_BOT_TOKEN = '123456789:ABCdefGHIjklMNOpqrsTUVwxyz';

const SHOPEE_SHOP_ID = 99887766;
const SHOPEE_PARTNER_KEY = 'mock_shopee_partner_key_id_2026';

function computeHmac(secret, body) {
  const hmac = crypto.createHmac('sha256', secret);
  hmac.update(body);
  return 'sha256=' + hmac.digest('hex');
}

async function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runOmnichannelTests() {
  console.log('================================================================');
  console.log('   INDONESIA OMNICHANNEL: FACEBOOK, TELEGRAM & SHOPEE TESTS    ');
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
  console.log('Agent login successful.');

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
        console.log('[WS EVENT]', msg.message.event, msg.message.data?.id || msg.message.data?.content || '');
        receivedEvents.push(msg.message);
      }
    };
    ws.onerror = reject;
    setTimeout(() => reject(new Error('WS connection timeout')), 5000);
  });

  // STEP 3: FACEBOOK MESSENGER CHANNEL
  console.log('\n=== STEP 3: FACEBOOK MESSENGER CHANNEL ===');
  
  // 3a. Handshake GET
  const fbChallenge = 'challenge_fb_' + Date.now();
  const fbVerifyRes = await fetch(`${API_URL}/webhooks/facebook?hub.mode=subscribe&hub.challenge=${fbChallenge}&hub.verify_token=${FB_VERIFY_TOKEN}`);
  if (!fbVerifyRes.ok || (await fbVerifyRes.text()) !== fbChallenge) {
    throw new Error(`Facebook webhook verification failed`);
  }
  console.log('Facebook GET handshake verified PASSED!');

  // 3b. Rails fallback endpoint /bot
  const botVerifyRes = await fetch(`${API_URL}/bot?hub.mode=subscribe&hub.challenge=${fbChallenge}&hub.verify_token=${FB_VERIFY_TOKEN}`);
  if (!botVerifyRes.ok || (await botVerifyRes.text()) !== fbChallenge) {
    throw new Error(`Rails /bot fallback endpoint verification failed`);
  }
  console.log('Facebook Rails /bot fallback endpoint verified PASSED!');

  // 3c. Inbound Message POST
  const fbCustomerPsid = 'fb_psid_' + Date.now();
  const fbMid = 'mid.FB_INBOUND_' + Date.now();
  const fbPayload = {
    object: 'page',
    entry: [
      {
        id: FB_PAGE_ID,
        time: Date.now(),
        messaging: [
          {
            sender: { id: fbCustomerPsid },
            recipient: { id: FB_PAGE_ID },
            timestamp: Date.now(),
            message: {
              mid: fbMid,
              text: 'Halo admin, apakah promo ongkir gratis masih berlaku di halaman Facebook?',
            },
          },
        ],
      },
    ],
  };

  const fbPayloadStr = JSON.stringify(fbPayload);
  const fbSig = computeHmac(FB_APP_SECRET, fbPayloadStr);

  const fbInboundRes = await fetch(`${API_URL}/webhooks/facebook`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Hub-Signature-256': fbSig,
    },
    body: fbPayloadStr,
  });

  if (!fbInboundRes.ok) {
    throw new Error(`Facebook inbound POST failed: ${fbInboundRes.status}`);
  }
  console.log('Facebook inbound message accepted by server.');

  await sleep(1000);

  const fbEvent = receivedEvents.find((e) => e.event === 'message.created' && e.data?.source_id === fbMid);
  if (!fbEvent) {
    throw new Error(`Expected ActionCable message.created for Facebook mid ${fbMid}`);
  }
  console.log('ActionCable received Facebook message.created PASSED! Conv display_id:', fbEvent.data.conversation_id);

  // 3d. Agent reply via Facebook
  const fbReplyRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${fbEvent.data.conversation_id}/messages`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      content: 'Halo kak! Iya promo gratis ongkir masih berlaku untuk semua pembelian hari ini ya!',
      message_type: 'outgoing',
    }),
  });
  if (!fbReplyRes.ok) throw new Error(`Facebook agent reply failed: ${fbReplyRes.status}`);

  await sleep(1000);

  const fbCheckRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${fbEvent.data.conversation_id}/messages`, {
    headers: authHeaders,
  });
  const fbCheckData = await fbCheckRes.json();
  const fbOutgoing = fbCheckData.payload.find((m) => m.content.includes('promo gratis ongkir'));
  if (!fbOutgoing || !fbOutgoing.source_id?.startsWith('mid.FB_')) {
    throw new Error('Facebook outgoing message source_id missing or invalid');
  }
  console.log('Facebook Outbound dispatch verified with mid:', fbOutgoing.source_id, 'PASSED!');

  // STEP 4: TELEGRAM BOT CHANNEL
  console.log('\n=== STEP 4: TELEGRAM BOT CHANNEL ===');
  const tgUserChatId = 987650001;
  const tgMsgId = Math.floor(Date.now() / 1000);
  const tgPayload = {
    update_id: 10001,
    message: {
      message_id: tgMsgId,
      from: {
        id: tgUserChatId,
        is_bot: false,
        first_name: 'Budi',
        last_name: 'Santoso',
        username: 'budisantoso_id',
      },
      chat: {
        id: tgUserChatId,
        first_name: 'Budi',
        last_name: 'Santoso',
        username: 'budisantoso_id',
        type: 'private',
      },
      date: Math.floor(Date.now() / 1000),
      text: 'Halo Bot Telegram! Saya ingin menanyakan status pendaftaran akun saya.',
    },
  };

  const tgInboundRes = await fetch(`${API_URL}/webhooks/telegram/${TG_BOT_TOKEN}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(tgPayload),
  });

  if (!tgInboundRes.ok) {
    throw new Error(`Telegram inbound POST failed: ${tgInboundRes.status}`);
  }
  console.log('Telegram inbound message accepted by server.');

  await sleep(1000);

  const tgEvent = receivedEvents.find((e) => e.event === 'message.created' && e.data?.source_id === tgMsgId.toString());
  if (!tgEvent) {
    throw new Error(`Expected ActionCable message.created for Telegram message_id ${tgMsgId}`);
  }
  console.log('ActionCable received Telegram message.created PASSED! Conv display_id:', tgEvent.data.conversation_id);

  // 4b. Agent reply via Telegram
  const tgReplyRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${tgEvent.data.conversation_id}/messages`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      content: 'Halo Mas Budi! Akun Anda sudah aktif dan siap digunakan ya.',
      message_type: 'outgoing',
    }),
  });
  if (!tgReplyRes.ok) throw new Error(`Telegram agent reply failed: ${tgReplyRes.status}`);

  await sleep(1000);

  const tgCheckRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${tgEvent.data.conversation_id}/messages`, {
    headers: authHeaders,
  });
  const tgCheckData = await tgCheckRes.json();
  const tgOutgoing = tgCheckData.payload.find((m) => m.content.includes('Halo Mas Budi'));
  if (!tgOutgoing || !tgOutgoing.source_id?.startsWith('tg_msg_')) {
    throw new Error('Telegram outgoing message source_id missing or invalid');
  }
  console.log('Telegram Outbound dispatch verified with id:', tgOutgoing.source_id, 'PASSED!');

  // STEP 5: SHOPEE CHAT MARKETPLACE CHANNEL
  console.log('\n=== STEP 5: SHOPEE CHAT MARKETPLACE CHANNEL ===');
  const shopeeBuyerId = 88776655;
  const shopeeMsgId = 'shopee_mid_' + Date.now();
  const shopeePayload = {
    shop_id: SHOPEE_SHOP_ID,
    code: 10,
    timestamp: Math.floor(Date.now() / 1000),
    data: {
      message_id: shopeeMsgId,
      from_id: shopeeBuyerId,
      to_id: SHOPEE_SHOP_ID,
      content: {
        text: 'Halo kak, apakah pesanan Shopee saya nomor 241024XYZ sudah dipacking dan siap dikirim hari ini?',
      },
    },
  };

  const shopeePayloadStr = JSON.stringify(shopeePayload);
  const shopeeSig = computeHmac(SHOPEE_PARTNER_KEY, shopeePayloadStr);

  const shopeeInboundRes = await fetch(`${API_URL}/webhooks/shopee`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': shopeeSig,
    },
    body: shopeePayloadStr,
  });

  if (!shopeeInboundRes.ok) {
    throw new Error(`Shopee inbound POST failed: ${shopeeInboundRes.status} ${await shopeeInboundRes.text()}`);
  }
  console.log('Shopee inbound chat accepted by server.');

  await sleep(1000);

  const shopeeEvent = receivedEvents.find((e) => e.event === 'message.created' && e.data?.source_id === shopeeMsgId);
  if (!shopeeEvent) {
    throw new Error(`Expected ActionCable message.created for Shopee message_id ${shopeeMsgId}`);
  }
  console.log('ActionCable received Shopee message.created PASSED! Conv display_id:', shopeeEvent.data.conversation_id);

  // 5b. Agent reply via Shopee
  const shopeeReplyRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${shopeeEvent.data.conversation_id}/messages`, {
    method: 'POST',
    headers: authHeaders,
    body: JSON.stringify({
      content: 'Halo kak, pesanan Shopee nomor 241024XYZ sudah selesai dipacking rapi dan akan diserahkan ke kurir sore ini ya!',
      message_type: 'outgoing',
    }),
  });
  if (!shopeeReplyRes.ok) throw new Error(`Shopee agent reply failed: ${shopeeReplyRes.status}`);

  await sleep(1000);

  const shopeeCheckRes = await fetch(`${API_URL}/api/v1/accounts/1/conversations/${shopeeEvent.data.conversation_id}/messages`, {
    headers: authHeaders,
  });
  const shopeeCheckData = await shopeeCheckRes.json();
  const shopeeOutgoing = shopeeCheckData.payload.find((m) => m.content.includes('pesanan Shopee nomor 241024XYZ'));
  if (!shopeeOutgoing || !shopeeOutgoing.source_id?.startsWith('shopee_msg_')) {
    throw new Error('Shopee outgoing message source_id missing or invalid');
  }
  console.log('Shopee Outbound dispatch verified with id:', shopeeOutgoing.source_id, 'PASSED!');

  ws.close();

  console.log('\n================================================================');
  console.log('   ALL INDONESIA OMNICHANNEL TESTS PASSED 100%!                ');
  console.log('================================================================');
}

runOmnichannelTests().catch((err) => {
  console.error('\nTEST FAILED:', err);
  process.exit(1);
});
