-- Sample Seed Data untuk Messaging

-- 1. Sample Inbox (API Channel)
INSERT INTO inboxes (id, account_id, channel_id, channel_type, name, created_at, updated_at)
VALUES (1, 1, 1, 'Channel::Api', 'Support Inbox', NOW(), NOW())
ON CONFLICT (id) DO NOTHING;

-- 2. Sample Contact
INSERT INTO contacts (id, account_id, name, email, phone_number, created_at, updated_at)
VALUES (1, 1, 'Sarah Connor', 'sarah@example.com', '+1234567890', NOW(), NOW())
ON CONFLICT (id) DO NOTHING;

-- 3. Sample Contact Inbox
INSERT INTO contact_inboxes (id, contact_id, inbox_id, source_id, created_at, updated_at)
VALUES (1, 1, 1, 'source_sarah_1', NOW(), NOW())
ON CONFLICT (id) DO NOTHING;

-- 4. Sample Conversation (display_id: 1, status: 0 / open)
INSERT INTO conversations (
    id, account_id, inbox_id, contact_id, contact_inbox_id, display_id,
    status, assignee_id, created_at, updated_at, last_activity_at
)
VALUES (1, 1, 1, 1, 1, 1, 0, NULL, NOW() - INTERVAL '5 minutes', NOW(), NOW())
ON CONFLICT (id) DO NOTHING;

-- 5. Sample Inbound Message from Contact
INSERT INTO messages (
    id, account_id, inbox_id, conversation_id, message_type, content,
    content_type, sender_type, sender_id, created_at, updated_at
)
VALUES (
    1, 1, 1, 1, 0, 'Hi, I need help with my recent order!',
    0, 'Contact', 1, NOW() - INTERVAL '4 minutes', NOW() - INTERVAL '4 minutes'
)
ON CONFLICT (id) DO NOTHING;
