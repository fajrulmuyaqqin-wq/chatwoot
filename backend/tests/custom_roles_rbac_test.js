// Comprehensive Test Suite for Enterprise Custom Roles & Granular RBAC Permissions
const API_URL = process.env.API_URL || 'http://127.0.0.1:3000';

async function runTests() {
  console.log('===============================================================');
  console.log('   ENTERPRISE CUSTOM ROLES & GRANULAR RBAC: VERIFICATION TEST  ');
  console.log('===============================================================\n');

  // STEP 1: AUTHENTICATION
  console.log('=== STEP 1: AUTHENTICATION (Administrator Login) ===');
  const loginRes = await fetch(`${API_URL}/auth/sign_in`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: 'john@acme.inc', password: 'Password123!' }),
  });

  if (!loginRes.ok) {
    throw new Error(`Login failed: ${loginRes.status} ${await loginRes.text()}`);
  }

  const token = loginRes.headers.get('access-token');
  const client = loginRes.headers.get('client');
  const uid = loginRes.headers.get('uid');
  console.log('✅ Administrator login successful.');

  const headers = {
    'access-token': token,
    'client': client,
    'uid': uid,
    'Content-Type': 'application/json',
  };

  // STEP 2: LIST EXISTING CUSTOM ROLES
  console.log('\n=== STEP 2: LIST CUSTOM ROLES ===');
  const listRes = await fetch(`${API_URL}/api/v1/accounts/1/custom_roles`, { headers });
  if (listRes.status === 200) {
    const roles = await listRes.json();
    console.log(`✅ GET /custom_roles -> 200 OK (${roles.length} roles found)`);
  } else {
    throw new Error(`GET /custom_roles failed: ${listRes.status}`);
  }

  // STEP 3: VALIDATION ON INVALID PERMISSIONS
  console.log('\n=== STEP 3: PERMISSION VALIDATION ===');
  const invalidRoleRes = await fetch(`${API_URL}/api/v1/accounts/1/custom_roles`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      custom_role: {
        name: 'Invalid Test Role',
        permissions: ['invalid_super_permission_xyz'],
      },
    }),
  });
  if (invalidRoleRes.status === 422) {
    console.log(`✅ POST /custom_roles with invalid permissions -> 422 Unprocessable Entity (rejected as expected)`);
  } else {
    throw new Error(`Expected 422 for invalid permission, got: ${invalidRoleRes.status}`);
  }

  // STEP 4: CREATE CUSTOM ROLE
  console.log('\n=== STEP 4: CREATE CUSTOM ROLE ===');
  const createRoleRes = await fetch(`${API_URL}/api/v1/accounts/1/custom_roles`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      custom_role: {
        name: 'Junior Support Tier 1',
        description: 'Handles unassigned incoming chats and contacts',
        permissions: ['conversation_unassigned_manage', 'contact_manage'],
      },
    }),
  });

  if (createRoleRes.status !== 200) {
    throw new Error(`POST /custom_roles failed: ${createRoleRes.status} ${await createRoleRes.text()}`);
  }

  const createdRole = await createRoleRes.json();
  const roleId = createdRole.id;
  console.log(`✅ POST /custom_roles -> 200 OK (Created Role ID: ${roleId}, name: "${createdRole.name}")`);

  // STEP 5: GET SINGLE CUSTOM ROLE
  console.log('\n=== STEP 5: GET SINGLE CUSTOM ROLE ===');
  const getRoleRes = await fetch(`${API_URL}/api/v1/accounts/1/custom_roles/${roleId}`, { headers });
  if (getRoleRes.status === 200) {
    const roleData = await getRoleRes.json();
    console.log(`✅ GET /custom_roles/:id -> 200 OK (Permissions: ${JSON.stringify(roleData.permissions)})`);
  } else {
    throw new Error(`GET /custom_roles/${roleId} failed: ${getRoleRes.status}`);
  }

  // STEP 6: UPDATE CUSTOM ROLE
  console.log('\n=== STEP 6: UPDATE CUSTOM ROLE ===');
  const updateRoleRes = await fetch(`${API_URL}/api/v1/accounts/1/custom_roles/${roleId}`, {
    method: 'PATCH',
    headers,
    body: JSON.stringify({
      custom_role: {
        description: 'Updated description: Handles unassigned chats and reports',
        permissions: ['conversation_unassigned_manage', 'report_manage'],
      },
    }),
  });
  if (updateRoleRes.status === 200) {
    const updated = await updateRoleRes.json();
    console.log(`✅ PATCH /custom_roles/:id -> 200 OK (Updated permissions: ${JSON.stringify(updated.permissions)})`);
  } else {
    throw new Error(`PATCH /custom_roles/${roleId} failed: ${updateRoleRes.status}`);
  }

  // STEP 7: ASSIGN CUSTOM ROLE TO AGENT
  console.log('\n=== STEP 7: AGENT CUSTOM ROLE ASSIGNMENT ===');
  const agentsRes = await fetch(`${API_URL}/api/v1/accounts/1/agents`, { headers });
  if (!agentsRes.ok) throw new Error(`GET /agents failed: ${agentsRes.status}`);
  const agents = await agentsRes.json();
  console.log(`Found ${agents.length} agents in account.`);

  // Find or use an agent user
  const targetAgent = agents.find((a) => a.id !== 18) || agents[0];
  console.log(`Assigning custom role ${roleId} to agent ID: ${targetAgent.id} (${targetAgent.name})...`);

  const assignRes = await fetch(`${API_URL}/api/v1/accounts/1/agents/${targetAgent.id}`, {
    method: 'PATCH',
    headers,
    body: JSON.stringify({ custom_role_id: roleId }),
  });

  if (assignRes.status === 200) {
    const assignedAgent = await assignRes.json();
    console.log(`✅ PATCH /agents/:id -> 200 OK (custom_role_id: ${assignedAgent.custom_role_id}, role: "${assignedAgent.custom_role?.name || ''}")`);
  } else {
    throw new Error(`PATCH /agents/${targetAgent.id} failed: ${assignRes.status} ${await assignRes.text()}`);
  }

  // STEP 8: DELETE CUSTOM ROLE & VERIFY NULLIFY
  console.log('\n=== STEP 8: DELETE CUSTOM ROLE (AUTO-NULLIFY REFERENCES) ===');
  const deleteRes = await fetch(`${API_URL}/api/v1/accounts/1/custom_roles/${roleId}`, {
    method: 'DELETE',
    headers,
  });

  if (deleteRes.status === 200) {
    console.log(`✅ DELETE /custom_roles/:id -> 200 OK`);
  } else {
    throw new Error(`DELETE /custom_roles/${roleId} failed: ${deleteRes.status}`);
  }

  // Check agent again to verify custom_role_id was safely cleared/nullified
  const checkAgentRes = await fetch(`${API_URL}/api/v1/accounts/1/agents`, { headers });
  const checkAgents = await checkAgentRes.json();
  const refreshedAgent = checkAgents.find((a) => a.id === targetAgent.id);
  if (!refreshedAgent.custom_role_id) {
    console.log(`✅ Verified agent custom_role_id is cleanly nullified (custom_role_id: ${refreshedAgent.custom_role_id})`);
  } else {
    throw new Error(`Expected custom_role_id to be null after deletion, got: ${refreshedAgent.custom_role_id}`);
  }

  console.log('\n===============================================================');
  console.log('   🎉 CUSTOM ROLES & GRANULAR RBAC VERIFIED SUCCESSFULLY!      ');
  console.log('===============================================================\n');
}

runTests().catch((err) => {
  console.error('\n❌ Test execution failed:', err);
  process.exit(1);
});
