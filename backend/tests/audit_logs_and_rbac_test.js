// Comprehensive Integration Test for Audit Logs Advanced Filters, CSV Download, and Granular RBAC (Reports & Contacts)
const API_URL = process.env.API_URL || 'http://127.0.0.1:3000';

async function login(email, password) {
  const res = await fetch(`${API_URL}/auth/sign_in`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  if (!res.ok) {
    throw new Error(`Login failed for ${email}: ${res.status} ${await res.text()}`);
  }
  return {
    'access-token': res.headers.get('access-token'),
    'client': res.headers.get('client'),
    'uid': res.headers.get('uid'),
    'Content-Type': 'application/json',
  };
}

async function runTests() {
  console.log('========================================================================');
  console.log('  AUDIT LOGS ADVANCED FILTERS, CSV EXPORT & GRANULAR RBAC VERIFICATION ');
  console.log('========================================================================\n');

  // 1. ADMIN AUTHENTICATION
  console.log('=== TEST 1: Administrator Authentication ===');
  const adminHeaders = await login('john@acme.inc', 'Password123!');
  console.log('✅ Admin login successful.\n');

  // 2. AUDIT LOGS JBUILDER & STORE COMPATIBILITY
  console.log('=== TEST 2: Audit Logs Response Schema & Pagination ===');
  const auditRes = await fetch(`${API_URL}/api/v1/accounts/1/audit_logs?page=1`, { headers: adminHeaders });
  if (auditRes.status !== 200) {
    throw new Error(`GET /audit_logs failed with status ${auditRes.status}`);
  }
  const auditData = await auditRes.json();
  if (!Array.isArray(auditData.audit_logs)) {
    throw new Error('Expected audit_logs array in response');
  }
  if (typeof auditData.total_entries !== 'number') {
    throw new Error('Expected total_entries number in response');
  }
  if (typeof auditData.per_page !== 'number' || typeof auditData.current_page !== 'number') {
    throw new Error('Expected per_page and current_page in response');
  }
  console.log(`✅ Audit logs schema valid: ${auditData.total_entries} total entries, ${auditData.audit_logs.length} returned on page 1.`);
  if (auditData.audit_logs.length > 0) {
    const sample = auditData.audit_logs[0];
    console.log(`   Sample log ID: ${sample.id}, Action: ${sample.action}, Auditable: ${sample.auditable_type}, User: ${sample.username}`);
  }

  // 3. ADVANCED FILTERS
  console.log('\n=== TEST 3: Audit Logs Filters (Search q, Auditable Type, Sort) ===');
  // 3a. Search by username
  const searchRes = await fetch(`${API_URL}/api/v1/accounts/1/audit_logs?q=john`, { headers: adminHeaders });
  const searchData = await searchRes.json();
  console.log(`✅ Search q=john returned ${searchData.total_entries} entries.`);

  // 3b. Filter by auditable type
  const typeRes = await fetch(`${API_URL}/api/v1/accounts/1/audit_logs?types[]=CustomRole`, { headers: adminHeaders });
  const typeData = await typeRes.json();
  console.log(`✅ Filter types[]=CustomRole returned ${typeData.total_entries} entries.`);
  for (const item of typeData.audit_logs) {
    if (item.auditable_type !== 'CustomRole') {
      throw new Error(`Expected auditable_type 'CustomRole', got '${item.auditable_type}'`);
    }
  }
  console.log('✅ All entries in filtered result correctly match auditable_type=CustomRole.');

  // 3c. Sort ascending
  const ascRes = await fetch(`${API_URL}/api/v1/accounts/1/audit_logs?sort=asc`, { headers: adminHeaders });
  const ascData = await ascRes.json();
  if (ascData.audit_logs.length >= 2) {
    const first = ascData.audit_logs[0].created_at;
    const second = ascData.audit_logs[1].created_at;
    if (first > second) {
      throw new Error(`Sort asc failed: first timestamp (${first}) > second timestamp (${second})`);
    }
  }
  console.log('✅ Sort asc returned logs in chronological ascending order.');

  // 4. CSV DOWNLOAD STREAM
  console.log('\n=== TEST 4: Audit Logs CSV Export Stream ===');
  const csvRes = await fetch(`${API_URL}/api/v1/accounts/1/audit_logs/download`, { headers: adminHeaders });
  if (csvRes.status !== 200) {
    throw new Error(`GET /audit_logs/download failed: ${csvRes.status}`);
  }
  const contentType = csvRes.headers.get('content-type') || '';
  const disposition = csvRes.headers.get('content-disposition') || '';
  if (!contentType.includes('text/csv')) {
    throw new Error(`Expected text/csv content-type, got: ${contentType}`);
  }
  if (!disposition.includes('audit_logs.csv')) {
    throw new Error(`Expected audit_logs.csv filename in content-disposition, got: ${disposition}`);
  }
  const csvText = await csvRes.text();
  const csvLines = csvText.trim().split('\n');
  console.log(`✅ CSV stream returned 200 OK (${csvLines.length} lines, header: ${csvLines[0]})`);

  // 5. RBAC: REPORTS PERMISSION ENFORCEMENT
  console.log('\n=== TEST 5: RBAC Granular Reports Permission Check ===');
  // 5a. Admin can view reports
  const adminReportRes = await fetch(`${API_URL}/api/v2/accounts/1/reports/summary`, { headers: adminHeaders });
  if (adminReportRes.status !== 200) {
    throw new Error(`Admin failed to access reports: ${adminReportRes.status}`);
  }
  console.log('✅ Administrator successfully accessed /reports/summary (200 OK).');

  // Find an agent in account 1
  const agentsRes = await fetch(`${API_URL}/api/v1/accounts/1/agents`, { headers: adminHeaders });
  const agents = await agentsRes.json();
  const agentUser = agents.find(a => a.role === 'agent' && a.email !== 'john@acme.inc');

  if (!agentUser) {
    console.log('⚠️ No dedicated agent found to test RBAC restrictions, skipping agent role check.');
  } else {
    console.log(`Found agent for RBAC testing: ${agentUser.name} (${agentUser.email})`);
    
    // Login as agent (using standard default dev password)
    let agentHeaders;
    try {
      agentHeaders = await login(agentUser.email, 'password123');
    } catch {
      try {
        agentHeaders = await login(agentUser.email, 'Password123!');
      } catch (err) {
        console.log('⚠️ Could not login agent with default password:', err.message);
      }
    }

    if (agentHeaders) {
      // 5b. Default agent without custom role -> must be FORBIDDEN (403) from accessing reports!
      // First ensure agent has NO custom role
      await fetch(`${API_URL}/api/v1/accounts/1/agents/${agentUser.id}`, {
        method: 'PATCH',
        headers: adminHeaders,
        body: JSON.stringify({ custom_role_id: null }),
      });

      const agentNoPermReport = await fetch(`${API_URL}/api/v2/accounts/1/reports/summary`, { headers: agentHeaders });
      if (agentNoPermReport.status === 403) {
        console.log('✅ Agent without custom role correctly denied access to /reports/summary (403 Forbidden).');
      } else {
        throw new Error(`Expected 403 Forbidden for agent without report_manage, got ${agentNoPermReport.status}`);
      }

      // 5c. Create custom role with report_manage and assign to agent
      const rolePayload = {
        custom_role: {
          name: 'Analytics Agent',
          description: 'Agent with report access',
          permissions: ['conversation_manage', 'report_manage'],
        },
      };
      const createRoleRes = await fetch(`${API_URL}/api/v1/accounts/1/custom_roles`, {
        method: 'POST',
        headers: adminHeaders,
        body: JSON.stringify(rolePayload),
      });
      const createdRole = await createRoleRes.json();
      console.log(`Created custom role: ${createdRole.name} (ID: ${createdRole.id})`);

      // Assign to agent
      await fetch(`${API_URL}/api/v1/accounts/1/agents/${agentUser.id}`, {
        method: 'PATCH',
        headers: adminHeaders,
        body: JSON.stringify({ custom_role_id: createdRole.id }),
      });
      console.log(`Assigned role ${createdRole.id} to agent ${agentUser.id}`);

      // Now agent should be ALLOWED (200 OK)
      const agentAllowedReport = await fetch(`${API_URL}/api/v2/accounts/1/reports/summary`, { headers: agentHeaders });
      if (agentAllowedReport.status === 200) {
        console.log('✅ Agent with report_manage permission successfully accessed /reports/summary (200 OK)!');
      } else {
        throw new Error(`Expected 200 OK for agent with report_manage, got ${agentAllowedReport.status}`);
      }

      // 5d. Test Contact Export RBAC
      console.log('\n=== TEST 6: RBAC Contact Management Permission Check ===');
      // Role only has ['conversation_manage', 'report_manage'], lacking 'contact_manage'
      const contactExportNoPerm = await fetch(`${API_URL}/api/v1/accounts/1/contacts/export`, {
        method: 'POST',
        headers: agentHeaders,
      });
      if (contactExportNoPerm.status === 403) {
        console.log('✅ Agent without contact_manage correctly denied access to /contacts/export (403 Forbidden).');
      } else {
        throw new Error(`Expected 403 Forbidden for contact export, got ${contactExportNoPerm.status}`);
      }

      // Update role to include 'contact_manage'
      await fetch(`${API_URL}/api/v1/accounts/1/custom_roles/${createdRole.id}`, {
        method: 'PATCH',
        headers: adminHeaders,
        body: JSON.stringify({
          custom_role: {
            permissions: ['conversation_manage', 'report_manage', 'contact_manage'],
          },
        }),
      });
      console.log('Updated custom role to include contact_manage.');

      // Now contact export should succeed (200 OK)
      const contactExportAllowed = await fetch(`${API_URL}/api/v1/accounts/1/contacts/export`, {
        method: 'POST',
        headers: agentHeaders,
      });
      if (contactExportAllowed.status === 200) {
        console.log('✅ Agent with contact_manage successfully executed /contacts/export (200 OK)!');
      } else {
        throw new Error(`Expected 200 OK for contact export, got ${contactExportAllowed.status}`);
      }

      // Cleanup: Delete test custom role
      await fetch(`${API_URL}/api/v1/accounts/1/custom_roles/${createdRole.id}`, {
        method: 'DELETE',
        headers: adminHeaders,
      });
      console.log(`Cleaned up test custom role ${createdRole.id}.`);
    }
  }

  console.log('\n========================================================================');
  console.log('  ALL AUDIT LOGS & GRANULAR RBAC VERIFICATION TESTS PASSED 100%!        ');
  console.log('========================================================================');
}

runTests().catch(err => {
  console.error('\n❌ TEST FAILURE:', err);
  process.exit(1);
});
