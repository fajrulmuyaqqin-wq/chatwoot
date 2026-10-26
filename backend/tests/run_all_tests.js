// Master Test Runner for Full Chatwoot Rust Backend Rewrite (Weeks 1 to 4)
const { execSync } = require('child_process');

const testSuites = [
  { name: 'Week 1: ActionCable WebSocket & Heartbeat Protocol', file: 'backend/tests/cable_client_test.js' },
  { name: 'Week 2: Core Messaging, Conversations, Uploads & Events', file: 'backend/tests/minggu2_integration_test.js' },
  { name: 'Week 3: Meta WhatsApp Cloud Webhooks, HMAC & Real-time Sync', file: 'backend/tests/minggu3_whatsapp_test.js' },
  { name: 'Week 4: Enterprise Features (Audit, SLA Breaches & AI RAG)', file: 'backend/tests/minggu4_enterprise_test.js' },
  { name: 'Meta Focus: Instagram Direct Message (Sprint 1)', file: 'backend/tests/instagram_channel_test.js' },
  { name: 'Indonesia Omnichannel: Facebook Messenger, Telegram & Shopee Chat', file: 'backend/tests/indonesia_omnichannel_test.js' },
  { name: 'Agent Productivity Suite: Canned Responses, Labels & Macros', file: 'backend/tests/productivity_suite_test.js' },
  { name: 'Enterprise Operations: Teams, Custom Attributes, Webhooks & Reports V2', file: 'backend/tests/teams_reports_webhooks_test.js' },
  { name: 'Multi-Tenant, RBAC & Super Admin Dashboard', file: 'backend/tests/multi_tenant_superadmin_test.js' },
  { name: 'Automation Rules Engine & In-App Agent Notifications', file: 'backend/tests/automation_and_notifications_test.js' },
  { name: 'Outbound Campaigns & CSAT Survey System', file: 'backend/tests/campaigns_and_csat_test.js' },
  { name: 'Final Parity: Email Channel, Advanced CRM & Tokopedia Omnichannel', file: 'backend/tests/email_crm_tokopedia_test.js' },
];

console.log('================================================================');
console.log('   CHATWOOT RUST BACKEND REWRITE - FULL VERIFICATION AUDIT      ');
console.log('================================================================\n');

let allPassed = true;

for (const suite of testSuites) {
  console.log(`>>> RUNNING: ${suite.name} (${suite.file})`);
  const startTime = Date.now();
  try {
    const output = execSync(`node ${suite.file}`, { stdio: 'pipe' }).toString();
    const duration = ((Date.now() - startTime) / 1000).toFixed(2);
    console.log(`[PASS] ${suite.name} completed in ${duration}s\n`);
  } catch (err) {
    allPassed = false;
    console.error(`[FAIL] ${suite.name} failed!`);
    console.error(err.stdout ? err.stdout.toString() : err.message);
    break;
  }
}

if (allPassed) {
  console.log('================================================================');
  console.log('   AUDIT RESULT: 100% ALL 12 TEST SUITES VERIFIED & PASSED!   ');
  console.log('================================================================\n');
  process.exit(0);
} else {
  console.error('\nAUDIT FAILED: Some test suites encountered errors.\n');
  process.exit(1);
}
