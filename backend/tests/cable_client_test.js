// ActionCable WebSocket Integration Test Client
const WS_URL = process.env.WS_URL || 'ws://127.0.0.1:3333/cable';

console.log(`Connecting to ${WS_URL}...`);
const ws = new WebSocket(WS_URL);

let receivedWelcome = false;
let receivedConfirm = false;
let receivedPresence = false;
let receivedPing = false;

const identifier = JSON.stringify({
  channel: 'RoomChannel',
  pubsub_token: 'sample_pubsub_token_john',
  account_id: 1,
  user_id: 1,
});

ws.onopen = () => {
  console.log('[WS] Connected to /cable');
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log('[WS RX]', JSON.stringify(data));

  if (data.type === 'welcome') {
    receivedWelcome = true;
    console.log('[TEST] Welcome received! Sending subscribe command...');
    ws.send(JSON.stringify({
      command: 'subscribe',
      identifier: identifier,
    }));
  } else if (data.type === 'confirm_subscription') {
    receivedConfirm = true;
    console.log('[TEST] Subscription confirmed! Sending update_presence...');
    ws.send(JSON.stringify({
      command: 'message',
      identifier: identifier,
      data: JSON.stringify({ action: 'update_presence' }),
    }));
  } else if (data.message && data.message.event === 'presence.update') {
    receivedPresence = true;
    console.log('[TEST] Presence update received:', data.message.data);
  } else if (data.type === 'ping') {
    receivedPing = true;
    console.log('[TEST] Ping heartbeat received!');
  }

  // Once all ActionCable protocol milestones are hit, succeed
  if (receivedWelcome && receivedConfirm && receivedPresence && receivedPing) {
    console.log('\n>>> ALL ACTIONCABLE PROTOCOL TESTS PASSED! <<<');
    ws.close();
    process.exit(0);
  }
};

ws.onerror = (err) => {
  console.error('[WS ERROR]', err);
  process.exit(1);
};

// Timeout after 10s if test hangs
setTimeout(() => {
  console.error('[TIMEOUT] Test timed out waiting for all messages!');
  console.error({ receivedWelcome, receivedConfirm, receivedPresence, receivedPing });
  process.exit(1);
}, 10000);
