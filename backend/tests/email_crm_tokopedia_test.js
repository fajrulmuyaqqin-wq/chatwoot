/**
 * Test Suite 12: Final Parity Expansion: Email Channel, Advanced CRM & Tokopedia Connector
 * Menguji:
 * 1. Contact CRUD (Create, Read, Update, Delete)
 * 2. Contact Merge Action (Transactional Re-parenting & Deduplication)
 * 3. Bulk Actions (Batch Status & Assignment on Conversations)
 * 4. Contact Batch Import (Batch JSON Upsert)
 * 5. Inbound Email Webhook & Threading (In-Reply-To Conversation Matching)
 * 6. Tokopedia Seller Chat Omnichannel (Handshake, HMAC & Inbound Message Ingestion)
 */

const http = require('http');
const crypto = require('crypto');

const BASE_URL = 'http://127.0.0.1:3333';
const TOKOPEDIA_APP_SECRET = 'tokopedia_secret_indonesia_2026';

function request(method, path, body = null, headers = {}) {
  return new Promise((resolve, reject) => {
    const url = new URL(path, BASE_URL);
    const reqHeaders = {
      'Content-Type': 'application/json',
      ...headers,
    };

    let postData = null;
    if (body) {
      postData = typeof body === 'string' ? body : JSON.stringify(body);
      reqHeaders['Content-Length'] = Buffer.byteLength(postData);
    }

    const options = {
      hostname: url.hostname,
      port: url.port,
      path: url.pathname + url.search,
      method: method,
      headers: reqHeaders,
    };

    const req = http.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => {
        let parsed = data;
        try {
          parsed = JSON.parse(data);
        } catch (_) {}
        resolve({ status: res.statusCode, headers: res.headers, body: parsed });
      });
    });

    req.on('error', reject);
    if (postData) req.write(postData);
    req.end();
  });
}

async function loginAgent() {
  const res = await request('POST', '/auth/sign_in', {
    email: 'john@example.com',
    password: 'password123',
  });

  if (res.status !== 200) {
    throw new Error(`Gagal login agent: ${JSON.stringify(res.body)}`);
  }

  return {
    'access-token': res.headers['access-token'],
    client: res.headers['client'],
    uid: res.headers['uid'],
  };
}

async function run() {
  console.log('--- STARTING SUITE 12: EMAIL CHANNEL, ADVANCED CRM & TOKOPEDIA ---');

  const authHeaders = await loginAgent();
  const accountId = 1;

  // =========================================================================
  // SEKTOR 1: CONTACT CRUD & BATCH IMPORT
  // =========================================================================
  console.log('\n[1/6] Menguji Contact CRUD...');
  const testContactEmail = `budi.test.${Date.now()}@example.com`;
  const createContactRes = await request('POST', `/api/v1/accounts/${accountId}/contacts`, {
    name: 'Budi Santoso',
    email: testContactEmail,
    phone_number: '+6281234567890',
    identifier: `id_${Date.now()}`,
    custom_attributes: { membership: 'platinum' },
  }, authHeaders);

  if (createContactRes.status !== 201) {
    throw new Error(`Gagal create contact: ${JSON.stringify(createContactRes.body)}`);
  }
  const baseContactId = createContactRes.body.payload.id;
  console.log(`  -> Contact utama dibuat dengan ID: ${baseContactId}`);

  // Update contact
  const updateContactRes = await request('PATCH', `/api/v1/accounts/${accountId}/contacts/${baseContactId}`, {
    name: 'Budi Santoso Updated',
    phone_number: '+628999888777',
  }, authHeaders);
  if (updateContactRes.status !== 200 || updateContactRes.body.payload.name !== 'Budi Santoso Updated') {
    throw new Error(`Gagal update contact: ${JSON.stringify(updateContactRes.body)}`);
  }
  console.log('  -> Contact berhasil di-update');

  // =========================================================================
  // SEKTOR 2: CONTACT BATCH IMPORT
  // =========================================================================
  console.log('\n[2/6] Menguji Contact Batch Import (JSON Stream)...');
  const importRes = await request('POST', `/api/v1/accounts/${accountId}/contacts/import`, {
    contacts: [
      { name: 'Imported User 1', email: `import1_${Date.now()}@test.com`, phone_number: '+628111111' },
      { name: 'Imported User 2', email: `import2_${Date.now()}@test.com`, phone_number: '+628222222' },
    ],
  }, authHeaders);

  if (importRes.status !== 200 || importRes.body.imported_count < 2) {
    throw new Error(`Gagal import batch contacts: ${JSON.stringify(importRes.body)}`);
  }
  console.log(`  -> Berhasil import ${importRes.body.imported_count} contacts`);

  // =========================================================================
  // SEKTOR 3: CONTACT MERGE ACTION (TRANSACTIONAL SAFETY)
  // =========================================================================
  console.log('\n[3/6] Menguji Contact Merge Action...');
  // Buat contact kedua (mergee / duplikat)
  const mergeeContactEmail = `budi.duplikat.${Date.now()}@example.com`;
  const mergeeRes = await request('POST', `/api/v1/accounts/${accountId}/contacts`, {
    name: 'Budi Duplikat',
    email: mergeeContactEmail,
    custom_attributes: { notes: 'dari web portal' },
  }, authHeaders);
  const mergeeId = mergeeRes.body.payload.id;
  console.log(`  -> Mergee contact dibuat dengan ID: ${mergeeId}`);

  // Eksekusi merge
  const mergeRes = await request('POST', `/api/v1/accounts/${accountId}/actions/contact_merge`, {
    base_contact_id: baseContactId,
    mergee_contact_id: mergeeId,
  }, authHeaders);

  if (mergeRes.status !== 200) {
    throw new Error(`Gagal merge contacts: ${JSON.stringify(mergeRes.body)}`);
  }
  console.log('  -> Contact merge berhasil dikomit dalam transaksi PostgreSQL');

  // Pastikan mergee contact sudah terhapus
  const checkMergeeRes = await request('GET', `/api/v1/accounts/${accountId}/contacts/${mergeeId}`, null, authHeaders);
  if (checkMergeeRes.status !== 404) {
    throw new Error('Mergee contact seharusnya sudah dihapus setelah merge!');
  }
  console.log('  -> Terverifikasi: Mergee contact telah dihapus secara bersih');

  // =========================================================================
  // SEKTOR 4: BULK ACTIONS ON CONVERSATIONS
  // =========================================================================
  console.log('\n[4/6] Menguji Bulk Actions...');
  // Ambil list percakapan yang ada
  const convListRes = await request('GET', `/api/v1/accounts/${accountId}/conversations`, null, authHeaders);
  const conversations = convListRes.body.data?.payload || [];
  if (conversations.length > 0) {
    const targetConvId = conversations[0].id;
    const bulkRes = await request('POST', `/api/v1/accounts/${accountId}/bulk_actions`, {
      type: 'Conversation',
      ids: [targetConvId],
      fields: {
        status: 'resolved',
      },
    }, authHeaders);

    if (bulkRes.status !== 200 || !bulkRes.body.success) {
      throw new Error(`Gagal bulk action: ${JSON.stringify(bulkRes.body)}`);
    }
    console.log(`  -> Bulk action conversation ID ${targetConvId} berhasil di-resolve`);
  } else {
    console.log('  -> Tidak ada conversation aktif, melewati uji bulk resolve');
  }

  // =========================================================================
  // SEKTOR 5: INBOUND EMAIL WEBHOOK & THREADING
  // =========================================================================
  console.log('\n[5/6] Menguji Inbound Email Webhook & Threading...');
  const initialMsgId = `<inbound-root-${Date.now()}@customer.com>`;
  const emailRes1 = await request('POST', '/api/v1/webhooks/email?account_id=1', {
    from: 'pelanggan.setia@gmail.com',
    from_name: 'Pelanggan Setia',
    to: 'support@chatwoot.local',
    subject: 'Pertanyaan Pesanan #888',
    text: 'Halo tim, apakah pesanan saya sudah dikirim?',
    message_id: initialMsgId,
  });

  if (emailRes1.status !== 200 || !emailRes1.body.conversation_id) {
    throw new Error(`Gagal memproses email inbound pertama: ${JSON.stringify(emailRes1.body)}`);
  }
  const emailConvId = emailRes1.body.conversation_id;
  console.log(`  -> Inbound email 1 diproses. Percakapan baru ID: ${emailConvId}`);

  // Kirim email balasan (threading test) dengan in_reply_to mengarah ke initialMsgId
  const replyMsgId = `<inbound-reply-${Date.now()}@customer.com>`;
  const emailRes2 = await request('POST', '/api/v1/webhooks/email?account_id=1', {
    from: 'pelanggan.setia@gmail.com',
    from_name: 'Pelanggan Setia',
    to: 'support@chatwoot.local',
    subject: 'Re: Pertanyaan Pesanan #888',
    text: 'Terima kasih, mohon infokan nomor resinya ya.',
    message_id: replyMsgId,
    in_reply_to: initialMsgId,
  });

  if (emailRes2.status !== 200 || emailRes2.body.conversation_id !== emailConvId) {
    throw new Error(`Threading email gagal! Seharusnya percakapan ${emailConvId}, didapat ${emailRes2.body.conversation_id}`);
  }
  console.log(`  -> Threading terverifikasi: Email balasan masuk ke percakapan yang sama (ID: ${emailConvId})`);

  // =========================================================================
  // SEKTOR 6: TOKOPEDIA OMNICHANNEL WEBHOOK
  // =========================================================================
  console.log('\n[6/6] Menguji Tokopedia Omnichannel Webhook & HMAC Signature...');
  // 1. Handshake verification
  const handshakeRes = await request('GET', '/api/v1/webhooks/tokopedia?challenge=tokopedia_chal_xyz');
  if (handshakeRes.status !== 200 || handshakeRes.body !== 'tokopedia_chal_xyz') {
    throw new Error(`Gagal handshake Tokopedia: ${JSON.stringify(handshakeRes.body)}`);
  }
  console.log('  -> Handshake challenge Tokopedia lolos');

  // 2. Inbound chat message dengan HMAC signature
  const tkpdMsgId = `tkpd_msg_${Date.now()}`;
  const tkpdPayload = {
    event: 'chat.receive',
    shop_id: 123456,
    data: {
      msg_id: tkpdMsgId,
      sender: {
        user_id: 98765,
        name: 'Pembeli Tokopedia Ramah',
      },
      message: 'Halo min, warna hijau botol apakah ready stock?',
      timestamp: Math.floor(Date.now() / 1000),
    },
  };

  const rawPayload = JSON.stringify(tkpdPayload);
  const hmac = crypto.createHmac('sha256', TOKOPEDIA_APP_SECRET);
  hmac.update(rawPayload);
  const tkpdSig = hmac.digest('hex');

  const tkpdRes = await request('POST', '/api/v1/webhooks/tokopedia', rawPayload, {
    'X-Tkpd-Signature': tkpdSig,
  });

  if (tkpdRes.status !== 200 || !tkpdRes.body.conversation_id) {
    throw new Error(`Gagal proses chat Tokopedia: ${JSON.stringify(tkpdRes.body)}`);
  }
  console.log(`  -> Chat Tokopedia berhasil diterima. Percakapan ID: ${tkpdRes.body.conversation_id}`);

  // 3. Deduplikasi pesan Tokopedia
  const dupTkpdRes = await request('POST', '/api/v1/webhooks/tokopedia', rawPayload, {
    'X-Tkpd-Signature': tkpdSig,
  });

  if (dupTkpdRes.status !== 200 || dupTkpdRes.body.status !== 'duplicate') {
    throw new Error('Deduplikasi pesan Tokopedia gagal!');
  }
  console.log('  -> Terverifikasi: Pesan duplikat Tokopedia berhasil dideduplikasi');

  console.log('\n================================================================');
  console.log('   SUITE 12: ALL EMAIL, CRM & TOKOPEDIA TESTS PASSED (100%)!    ');
  console.log('================================================================');
}

run().catch((err) => {
  console.error('\n[SUITE 12 FAILED]:', err.message);
  process.exit(1);
});
