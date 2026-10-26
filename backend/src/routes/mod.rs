pub mod accounts;
pub mod agent_capacity_policies;
pub mod agents;
pub mod ai;
pub mod app_frontend;
pub mod audit_logs;
pub mod auth;
pub mod automation_rules;
pub mod bulk_actions;
pub mod campaigns;
pub mod canned_responses;
pub mod captain;
pub mod calls;
pub mod callbacks;
pub mod companies;
pub mod copilot;

pub mod contact_merges;
pub mod contacts;
pub mod contacts_import;
pub mod conversations;
pub mod csat;
pub mod custom_attribute_definitions;
pub mod custom_filters;
pub mod custom_roles;
pub mod dashboard_apps;
pub mod enterprise_limits;
pub mod inboxes;
pub mod integrations;
pub mod labels;
pub mod macros;
pub mod messages;
pub mod monitors;
pub mod notifications;
pub mod portals;
pub mod profile;
pub mod reports;
pub mod saml_settings;
pub mod search;
pub mod sla;
pub mod super_admin;
pub mod teams;
pub mod uploads;
pub mod webhooks;
pub mod webhooks_api;

use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};
use tower_http::services::ServeDir;

use crate::state::AppState;

pub fn create_router(state: AppState) -> Router<AppState> {
    // Auth routes (Devise Token Auth)
    let auth_routes = Router::new()
        .route("/sign_in", post(auth::sign_in))
        .route("/validate_token", get(auth::validate_token))
        .route("/sign_out", delete(auth::sign_out));

    // Rute API Akun Chatwoot v1 yang diproteksi oleh auth middleware
    let account_routes_v1 = Router::new()
        // Mendukung GET /api/v1/accounts/:account_id/ dari ApiClient.js
        .route(
            "/",
            get(accounts::get_account)
                .patch(accounts::update_account)
                .put(accounts::update_account),
        )
        .route(
            "/inboxes",
            get(inboxes::list_inboxes).post(inboxes::create_inbox),
        )
        .route(
            "/inboxes/",
            get(inboxes::list_inboxes),
        )
        .route(
            "/inboxes/:id",
            get(inboxes::get_inbox)
                .patch(inboxes::update_inbox)
                .put(inboxes::update_inbox)
                .delete(inboxes::delete_inbox),
        )
        .route(
            "/inboxes/:id/message_templates",
            get(inboxes::get_message_templates),
        )
        .route(
            "/inboxes/:id/sync_templates",
            post(inboxes::sync_templates),
        )
        .route(
            "/inboxes/:id/agent_bot",
            get(inboxes::get_agent_bot),
        )
        .route(
            "/inboxes/:id/set_agent_bot",
            post(inboxes::set_agent_bot),
        )
        .route(
            "/inboxes/:id/avatar",
            delete(inboxes::delete_inbox_avatar),
        )
        .route(
            "/inbox_members",
            post(inboxes::update_inbox_members)
                .patch(inboxes::update_inbox_members)
                .delete(inboxes::delete_inbox_members),
        )
        .route(
            "/inbox_members/:inbox_id",
            get(inboxes::get_inbox_members),
        )
        // Meta Facebook Pages Callbacks
        .route("/callbacks/facebook_pages.json", post(callbacks::fetch_facebook_pages))
        .route("/callbacks/facebook_pages", post(callbacks::fetch_facebook_pages))
        .route("/callbacks/register_facebook_page", post(callbacks::register_facebook_page))
        .route("/callbacks/reauthorize_page", post(callbacks::reauthorize_facebook_page))
        // Instagram Authorization
        .route("/instagram/authorization", post(callbacks::instagram_authorization))
        .route(
            "/contacts",
            get(contacts::list_contacts).post(contacts::create_contact),
        )
        .route(
            "/contacts/:id",
            get(contacts::get_contact)
                .patch(contacts::update_contact)
                .put(contacts::update_contact)
                .delete(contacts::delete_contact),
        )
        .route("/contacts/:id/call", post(calls::create_contact_call))
        .route(
            "/contacts/import",
            post(contacts_import::import_contacts_json),
        )
        .route(
            "/contacts/import_csv",
            post(contacts_import::import_contacts_csv),
        )
        .route(
            "/actions/contact_merge",
            post(contact_merges::merge_contacts),
        )
        .route(
            "/bulk_actions",
            post(bulk_actions::perform_bulk_action),
        )
        // Companies CRM
        .route(
            "/companies",
            get(companies::list_companies).post(companies::create_company),
        )
        .route("/companies/search", get(companies::search_companies))
        .route(
            "/companies/:id",
            get(companies::get_company)
                .patch(companies::update_company)
                .put(companies::update_company)
                .delete(companies::delete_company),
        )
        .route(
            "/companies/:id/destroy_custom_attributes",
            post(companies::destroy_custom_attributes),
        )
        .route("/companies/:id/avatar", delete(companies::destroy_avatar))
        .route("/companies/:id/enrichment", post(companies::enrich_company))
        .route(
            "/companies/:id/contacts",
            get(companies::list_company_contacts).post(companies::attach_company_contact),
        )
        .route(
            "/companies/:id/contacts/search",
            get(companies::search_company_contacts),
        )
        .route(
            "/companies/:id/contacts/:contact_id",
            delete(companies::remove_company_contact),
        )
        .route(
            "/companies/:id/conversations",
            get(companies::list_company_conversations),
        )
        .route(
            "/companies/:id/conversations/filter",
            post(companies::filter_company_conversations),
        )
        .route(
            "/companies/:id/notes",
            get(companies::list_company_notes),
        )
        // Calls (Enterprise Voice & WhatsApp Calls)
        .route("/calls", get(calls::list_calls))
        .route("/calls/:id", get(calls::get_call))
        .route("/whatsapp_calls/:id", get(calls::get_whatsapp_call))
        .route("/whatsapp_calls/:id/accept", post(calls::accept_whatsapp_call))
        .route("/whatsapp_calls/:id/reject", post(calls::reject_whatsapp_call))
        .route("/whatsapp_calls/:id/terminate", post(calls::terminate_whatsapp_call))
        .route("/whatsapp_calls/:id/upload_recording", post(calls::upload_whatsapp_recording))
        .route("/whatsapp_calls/initiate", post(calls::initiate_whatsapp_call))
        .route("/calls/twilio/token", post(calls::get_twilio_client_token))
        .route("/conversations", get(conversations::list_conversations))
        .route(
            "/conversations/:display_id",
            get(conversations::get_conversation),
        )
        .route(
            "/conversations/:display_id/toggle_status",
            post(conversations::toggle_status),
        )
        .route(
            "/conversations/:display_id/messages",
            get(messages::list_messages).post(messages::create_message),
        )
        .route(
            "/conversations/:display_id/direct_uploads",
            post(uploads::direct_upload),
        )
        .route("/audit_logs", get(audit_logs::list_audit_logs))
        .route(
            "/sla_policies",
            get(sla::list_sla_policies).post(sla::create_sla_policy),
        )
        .route(
            "/conversations/:display_id/apply_sla",
            post(sla::apply_sla_to_conversation),
        )
        .route("/articles", get(ai::list_articles).post(ai::create_article))
        .route("/ai/reply_suggestions", post(ai::get_reply_suggestion))
        // Omnisearch
        .route("/search", get(search::search_all))
        .route("/search/conversations", get(search::search_conversations))
        .route("/search/contacts", get(search::search_contacts))
        .route("/search/messages", get(search::search_messages))
        .route("/search/articles", get(search::search_articles))
        // Canned responses
        .route(
            "/canned_responses",
            get(canned_responses::list_canned_responses).post(canned_responses::create_canned_response),
        )
        .route(
            "/canned_responses/:id",
            patch(canned_responses::update_canned_response)
                .put(canned_responses::update_canned_response)
                .delete(canned_responses::delete_canned_response),
        )
        // Labels
        .route(
            "/labels",
            get(labels::list_labels).post(labels::create_label),
        )
        .route(
            "/labels/:id",
            patch(labels::update_label)
                .put(labels::update_label)
                .delete(labels::delete_label),
        )
        // Conversation labels
        .route(
            "/conversations/:display_id/labels",
            get(labels::get_conversation_labels).post(labels::set_conversation_labels),
        )
        // Macros
        .route(
            "/macros",
            get(macros::list_macros).post(macros::create_macro),
        )
        .route(
            "/macros/:id",
            patch(macros::update_macro)
                .put(macros::update_macro)
                .delete(macros::delete_macro),
        )
        .route(
            "/macros/:id/execute",
            post(macros::execute_macro),
        )
        // Teams & Team Members
        .route(
            "/teams",
            get(teams::list_teams).post(teams::create_team),
        )
        .route(
            "/teams/:id",
            get(teams::get_team)
                .patch(teams::update_team)
                .put(teams::update_team)
                .delete(teams::delete_team),
        )
        .route(
            "/teams/:id/team_members",
            get(teams::list_team_members)
                .post(teams::add_team_members)
                .delete(teams::remove_team_members),
        )
        // Custom Attribute Definitions
        .route(
            "/custom_attribute_definitions",
            get(custom_attribute_definitions::list_custom_attribute_definitions)
                .post(custom_attribute_definitions::create_custom_attribute_definition),
        )
        .route(
            "/custom_attribute_definitions/:id",
            get(custom_attribute_definitions::get_custom_attribute_definition)
                .patch(custom_attribute_definitions::update_custom_attribute_definition)
                .put(custom_attribute_definitions::update_custom_attribute_definition)
                .delete(custom_attribute_definitions::delete_custom_attribute_definition),
        )
        // Outbound Webhooks API
        .route(
            "/webhooks",
            get(webhooks_api::list_webhooks).post(webhooks_api::create_webhook),
        )
        .route(
            "/webhooks/:id",
            patch(webhooks_api::update_webhook)
                .put(webhooks_api::update_webhook)
                .delete(webhooks_api::delete_webhook),
        )
        // Agents Management & Role Assignments
        .route(
            "/agents",
            get(agents::list_agents).post(agents::create_agent),
        )
        .route(
            "/agents/:id",
            patch(agents::update_agent)
                .put(agents::update_agent)
                .delete(agents::delete_agent),
        )
        .route(
            "/assignable_agents",
            get(agents::list_assignable_agents),
        )
        // Conversation assignments
        .route(
            "/conversations/:display_id/assignments",
            post(conversations::assign_conversation),
        )
        // Automation Rules
        .route(
            "/automation_rules",
            get(automation_rules::list_automation_rules).post(automation_rules::create_automation_rule),
        )
        .route(
            "/automation_rules/:id",
            get(automation_rules::get_automation_rule)
                .patch(automation_rules::update_automation_rule)
                .put(automation_rules::update_automation_rule)
                .delete(automation_rules::delete_automation_rule),
        )
        .route(
            "/automation_rules/:id/clone",
            post(automation_rules::clone_automation_rule),
        )
        // In-App Notifications
        .route(
            "/notifications",
            get(notifications::list_notifications),
        )
        .route(
            "/notifications/read_all",
            post(notifications::read_all_notifications),
        )
        .route(
            "/notifications/unread_count",
            get(notifications::get_unread_count).post(notifications::get_unread_count),
        )
        .route(
            "/notifications/destroy_all",
            delete(notifications::delete_all_notifications),
        )
        .route(
            "/notifications/:id",
            patch(notifications::mark_notification_as_read)
                .delete(notifications::delete_notification),
        )
        .route(
            "/notifications/:id/unread",
            post(notifications::mark_notification_as_unread),
        )
        .route(
            "/notifications/:id/snooze",
            post(notifications::snooze_notification),
        )
        // Campaigns (Outbound Broadcast Blast & Ongoing)
        .route(
            "/campaigns",
            get(campaigns::list_campaigns).post(campaigns::create_campaign),
        )
        .route(
            "/campaigns/:id",
            get(campaigns::get_campaign)
                .patch(campaigns::update_campaign)
                .put(campaigns::update_campaign)
                .delete(campaigns::delete_campaign),
        )
        .route(
            "/campaigns/:id/trigger",
            post(campaigns::trigger_campaign_blast),
        )
        .route(
            "/campaigns/:id/analytics/metrics",
            get(campaigns::get_campaign_metrics),
        )
        .route(
            "/campaigns/:id/analytics/contacts",
            get(campaigns::get_campaign_contacts),
        )
        // CSAT Survey Responses & Metrics
        .route(
            "/csat_survey_responses",
            get(csat::list_csat_responses).post(csat::create_csat_response),
        )
        .route(
            "/csat_survey_responses/metrics",
            get(csat::get_csat_metrics),
        )
        .route(
            "/csat_survey_responses/download",
            get(csat::download_csat_csv),
        )
        // Custom Filters (Folders)
        .route(
            "/custom_filters",
            get(custom_filters::list_custom_filters).post(custom_filters::create_custom_filter),
        )
        .route(
            "/custom_filters/:id",
            delete(custom_filters::delete_custom_filter),
        )
        // Dashboard Apps
        .route(
            "/dashboard_apps",
            get(dashboard_apps::list_dashboard_apps),
        )
        // Portals (Help Center)
        .route(
            "/portals",
            get(portals::list_portals).post(portals::create_portal),
        )
        .route(
            "/portals/",
            get(portals::list_portals),
        )
        .route(
            "/portals/:slug",
            get(portals::get_portal)
                .patch(portals::update_portal)
                .put(portals::update_portal)
                .delete(portals::delete_portal),
        )
        .route(
            "/portals/:slug/archive",
            patch(portals::archive_portal),
        )
        .route(
            "/portals/:slug/logo",
            delete(portals::delete_portal_logo),
        )
        // Portal Categories
        .route(
            "/portals/:portal_slug/categories",
            get(portals::list_portal_categories).post(portals::create_portal_category),
        )
        .route(
            "/portals/:portal_slug/categories/:id",
            get(portals::get_portal_category)
                .patch(portals::update_portal_category)
                .put(portals::update_portal_category)
                .delete(portals::delete_portal_category),
        )
        .route(
            "/portals/:portal_slug/categories/reorder",
            post(portals::reorder_portal_categories),
        )
        // Portal Articles
        .route(
            "/portals/:portal_slug/articles",
            get(portals::list_portal_articles).post(portals::create_portal_article),
        )
        .route(
            "/portals/:portal_slug/articles/:id",
            get(portals::get_portal_article)
                .patch(portals::update_portal_article)
                .put(portals::update_portal_article)
                .delete(portals::delete_portal_article),
        )
        .route(
            "/portals/:portal_slug/articles/reorder",
            post(portals::reorder_portal_articles),
        )
        // Captain AI Assistants
        .route(
            "/captain/assistants",
            get(captain::list_assistants).post(captain::create_assistant),
        )
        .route(
            "/captain/assistants/",
            get(captain::list_assistants),
        )
        .route(
            "/captain/assistants/:id",
            get(captain::get_assistant)
                .patch(captain::update_assistant)
                .put(captain::update_assistant)
                .delete(captain::delete_assistant),
        )
        .route(
            "/captain/assistants/:id/metrics",
            get(captain::get_metrics),
        )
        .route(
            "/captain/assistants/:id/faq_stats",
            get(captain::get_faq_stats),
        )
        .route(
            "/captain/assistants/:id/summary",
            get(captain::get_summary),
        )
        .route(
            "/captain/assistants/:id/drilldown",
            get(captain::get_drilldown),
        )
        .route(
            "/captain/assistants/:id/playground",
            post(captain::playground),
        )
        .route(
            "/captain/preferences",
            get(captain::get_preferences),
        )
        // Captain Documents (RAG)
        .route(
            "/captain/documents",
            get(captain::list_documents).post(captain::create_document),
        )
        .route(
            "/captain/documents/",
            get(captain::list_documents),
        )
        .route(
            "/captain/documents/:id",
            get(captain::get_document)
                .patch(captain::update_document)
                .put(captain::update_document)
                .delete(captain::delete_document),
        )
        .route(
            "/captain/documents/:id/sync",
            post(captain::sync_document),
        )
        .route(
            "/captain/documents/:id/drilldown",
            get(captain::get_document_drilldown),
        )
        // Captain Assistant Responses (Knowledge Q&A)
        .route(
            "/captain/assistant_responses",
            get(captain::list_assistant_responses).post(captain::create_assistant_response),
        )
        .route(
            "/captain/assistant_responses/",
            get(captain::list_assistant_responses),
        )
        .route(
            "/captain/assistant_responses/:id",
            patch(captain::update_assistant_response)
                .put(captain::update_assistant_response)
                .delete(captain::delete_assistant_response),
        )
        .route(
            "/captain/assistant_responses/:id/drilldown",
            get(captain::get_response_drilldown),
        )
        .route(
            "/captain/faq_suggestions",
            get(captain::list_faq_suggestions),
        )
        // Captain Custom Tools & Manifest
        .route(
            "/captain/assistants/tools",
            get(captain::get_assistant_tools),
        )
        .route(
            "/captain/custom_tools",
            get(captain::list_custom_tools).post(captain::create_custom_tool),
        )
        .route(
            "/captain/custom_tools/test",
            post(captain::test_custom_tool),
        )
        .route(
            "/captain/custom_tools/:id",
            get(captain::get_custom_tool)
                .put(captain::update_custom_tool)
                .patch(captain::update_custom_tool)
                .delete(captain::delete_custom_tool),
        )
        .route(
            "/captain/tools_manifest/installed",
            get(captain::get_installed_tools),
        )
        // Captain Scenarios
        .route(
            "/captain/scenarios",
            get(captain::list_scenarios).post(captain::create_scenario),
        )
        .route(
            "/captain/scenarios/:id",
            delete(captain::delete_scenario),
        )
        // Scoped Captain Assistant Inboxes & Scenarios
        .route(
            "/captain/assistants/:assistant_id/inboxes",
            get(captain::list_assistant_inboxes).post(captain::create_assistant_inbox),
        )
        .route(
            "/captain/assistants/:assistant_id/inboxes/:inbox_id",
            delete(captain::delete_assistant_inbox),
        )
        .route(
            "/captain/assistants/:assistant_id/scenarios",
            get(captain::list_assistant_scenarios).post(captain::create_assistant_scenario),
        )
        .route(
            "/captain/assistants/:assistant_id/scenarios/:id",
            get(captain::get_assistant_scenario)
                .put(captain::update_assistant_scenario)
                .patch(captain::update_assistant_scenario)
                .delete(captain::delete_assistant_scenario),
        )
        // Scoped Captain Assistant Stats
        .route(
            "/captain/assistants/:assistant_id/stats/overview",
            get(captain::get_assistant_stats_overview),
        )
        .route(
            "/captain/assistants/:assistant_id/stats/overview_summary",
            get(captain::get_assistant_stats_overview_summary),
        )
        .route(
            "/captain/assistants/:assistant_id/stats/resolution_flow",
            get(captain::get_assistant_stats_resolution_flow),
        )
        .route(
            "/captain/assistants/:assistant_id/stats/resolution_trend",
            get(captain::get_assistant_stats_resolution_trend),
        )
        .route(
            "/captain/assistants/:assistant_id/stats/drilldown",
            get(captain::get_assistant_stats_drilldown),
        )
        // Copilot (Agent Assist)
        .route(
            "/captain/copilot_threads",
            get(copilot::list_threads).post(copilot::create_thread),
        )
        .route(
            "/captain/copilot_threads/:thread_id/copilot_messages",
            get(copilot::list_messages).post(copilot::create_message),
        )
        // Enterprise Custom Roles (RBAC)
        .route(
            "/custom_roles",
            get(custom_roles::list_custom_roles).post(custom_roles::create_custom_role),
        )
        .route(
            "/custom_roles/",
            get(custom_roles::list_custom_roles),
        )
        .route(
            "/custom_roles/:id",
            get(custom_roles::get_custom_role)
                .patch(custom_roles::update_custom_role)
                .put(custom_roles::update_custom_role)
                .delete(custom_roles::delete_custom_role),
        )
        // Enterprise Agent Capacity Policies
        .route(
            "/agent_capacity_policies",
            get(agent_capacity_policies::list_policies).post(agent_capacity_policies::create_policy),
        )
        .route(
            "/agent_capacity_policies/",
            get(agent_capacity_policies::list_policies),
        )
        .route(
            "/agent_capacity_policies/:id",
            get(agent_capacity_policies::get_policy)
                .patch(agent_capacity_policies::update_policy)
                .put(agent_capacity_policies::update_policy)
                .delete(agent_capacity_policies::delete_policy),
        )
        .route(
            "/agent_capacity_policies/:policy_id/inbox_limits",
            post(agent_capacity_policies::create_inbox_limit),
        )
        .route(
            "/agent_capacity_policies/:policy_id/inbox_limits/:id",
            patch(agent_capacity_policies::update_inbox_limit)
                .put(agent_capacity_policies::update_inbox_limit)
                .delete(agent_capacity_policies::delete_inbox_limit),
        )
        .route(
            "/agent_capacity_policies/:policy_id/inbox_capacity_limits",
            post(agent_capacity_policies::create_inbox_limit),
        )
        .route(
            "/agent_capacity_policies/:policy_id/inbox_capacity_limits/:id",
            patch(agent_capacity_policies::update_inbox_limit)
                .put(agent_capacity_policies::update_inbox_limit)
                .delete(agent_capacity_policies::delete_inbox_limit),
        )
        .route(
            "/agent_capacity_policies/:policy_id/users",
            get(agent_capacity_policies::list_policy_users).post(agent_capacity_policies::add_policy_user),
        )
        .route(
            "/agent_capacity_policies/:policy_id/users/:user_id",
            delete(agent_capacity_policies::remove_policy_user),
        )
        // Enterprise SAML Settings (SSO)
        .route(
            "/saml_settings",
            get(saml_settings::get_saml_settings)
                .post(saml_settings::create_saml_settings)
                .patch(saml_settings::update_saml_settings)
                .put(saml_settings::update_saml_settings)
                .delete(saml_settings::delete_saml_settings),
        )
        .route(
            "/saml_settings/refresh",
            post(saml_settings::refresh_saml_metadata),
        )
        // Enterprise Conversation Monitors (AI QA)
        .route(
            "/monitors",
            get(monitors::list_monitors).post(monitors::create_monitor),
        )
        .route(
            "/monitors/",
            get(monitors::list_monitors),
        )
        .route(
            "/monitors/preview",
            post(monitors::preview_monitor),
        )
        .route(
            "/monitors/preview/:token",
            get(monitors::preview_status),
        )
        .route(
            "/monitors/:id",
            get(monitors::get_monitor)
                .patch(monitors::update_monitor)
                .put(monitors::update_monitor)
                .delete(monitors::delete_monitor),
        )
        .route(
            "/monitors/:id/timeseries",
            get(monitors::get_monitor_timeseries),
        )
        .route(
            "/monitors/:id/conversations",
            get(monitors::get_monitor_conversations),
        )
        .route(
            "/monitors/:id/retry_evaluations",
            post(monitors::retry_evaluations),
        )
        .route(
            "/monitors/:id/resume",
            post(monitors::resume_monitor),
        )
        // Integrations Hub & Apps
        .route("/integrations/apps", get(integrations::list_apps))
        .route("/integrations/apps/:id", get(integrations::get_app))
        .route("/integrations/hooks", get(integrations::list_hooks).post(integrations::create_hook))
        .route("/integrations/hooks/:id", delete(integrations::delete_hook))
        .route("/integrations/:app_id", delete(integrations::delete_app_hooks))
        // Slack Integration
        .route(
            "/integrations/slack",
            post(integrations::connect_slack)
                .put(integrations::update_slack)
                .patch(integrations::update_slack)
                .delete(integrations::disconnect_slack),
        )
        .route("/integrations/slack/list_all_channels", get(integrations::list_slack_channels))
        // Linear Integration
        .route("/integrations/linear/teams", get(integrations::list_linear_teams))
        .route("/integrations/linear/team_entities", get(integrations::get_linear_team_entities))
        .route("/integrations/linear/create_issue", post(integrations::create_linear_issue))
        .route("/integrations/linear/link_issue", post(integrations::link_linear_issue))
        .route("/integrations/linear/linked_issues", get(integrations::list_linked_linear_issues))
        .route("/integrations/linear/unlink_issue", post(integrations::unlink_linear_issue))
        .route("/integrations/linear/search_issue", get(integrations::search_linear_issues))
        // Dyte Video Calls Integration
        .route("/integrations/dyte/create_a_meeting", post(integrations::create_dyte_meeting))
        .route("/integrations/dyte/add_participant_to_meeting", post(integrations::add_dyte_participant))
        // Migration Importers
        .route("/migrations/intercom", post(integrations::import_intercom))
        .route("/migrations/zendesk", post(integrations::import_zendesk))
        .route("/migrations/freshdesk", post(integrations::import_freshdesk))
        .route("/migrations/status", get(integrations::migration_status));


    let api_v1 = Router::new()
        .route("/profile", get(profile::get_profile).put(profile::update_profile).patch(profile::update_profile))
        .route("/profile/set_active_account", put(profile::set_active_account).post(profile::set_active_account))
        .route("/profile/availability", post(profile::update_availability).put(profile::update_availability))
        .route("/profile/auto_offline", post(profile::update_auto_offline).put(profile::update_auto_offline))
        .route("/profile/avatar", delete(profile::dummy_ok))
        .route("/profile/resend_confirmation", post(profile::dummy_ok))
        .route("/profile/reset_access_token", post(profile::dummy_ok))
        .route("/accounts", post(accounts::create_account))
        .route(
            "/accounts/:account_id/",
            get(accounts::get_account)
                .patch(accounts::update_account)
                .put(accounts::update_account),
        )
        .nest("/accounts/:account_id", account_routes_v1)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::audit::audit_logger,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::auth::require_auth,
        ));

    // Rute API Akun Chatwoot v2 (Reports & Analytics Engine)
    let account_routes_v2 = Router::new()
        // Reports Timeseries & Summary
        .route("/reports/summary", get(reports::get_reports_summary))
        .route("/reports", get(reports::get_reports_timeseries))
        .route("/reports/conversations", get(reports::get_reports_conversations))
        .route("/reports/drilldown", get(reports::get_reports_drilldown))
        .route("/reports/bot_metrics", get(reports::get_bot_metrics))
        .route("/reports/bot_summary", get(reports::get_bot_summary))
        // Reports CSV Exports
        .route("/reports/agents", get(reports::get_reports_agents_csv))
        .route("/reports/inboxes", get(reports::get_reports_inboxes_csv))
        .route("/reports/labels", get(reports::get_reports_labels_csv))
        .route("/reports/teams", get(reports::get_reports_teams_csv))
        .route("/reports/conversations_summary", get(reports::get_reports_conversations_summary_csv))
        .route("/reports/conversation_traffic", get(reports::get_reports_conversation_traffic_csv))
        // Summary Reports (Agent, Team, Inbox, Label, Channel)
        .route("/summary_reports/agent", get(reports::get_agent_summary))
        .route("/summary_reports/team", get(reports::get_team_summary))
        .route("/summary_reports/inbox", get(reports::get_inbox_summary))
        .route("/summary_reports/label", get(reports::get_label_summary))
        .route("/summary_reports/channel", get(reports::get_channel_summary))
        // Live Reports
        .route("/live_reports/conversation_metrics", get(reports::get_live_conversation_metrics))
        .route("/live_reports/grouped_conversation_metrics", get(reports::get_live_grouped_metrics))
        // Year In Review
        .route("/year_in_review", get(reports::get_year_in_review));

    let api_v2 = Router::new()
        .nest("/accounts/:account_id", account_routes_v2)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::audit::audit_logger,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::auth::require_auth,
        ));

    // Rute API Super Admin
    let super_admin_api = Router::new()
        .route(
            "/accounts",
            get(super_admin::list_all_accounts).post(super_admin::super_admin_create_account),
        )
        .route("/accounts/:id", get(super_admin::super_admin_get_account))
        .route("/accounts/:id/seed", post(super_admin::super_admin_seed_account))
        .route(
            "/users",
            get(super_admin::list_all_users).post(super_admin::super_admin_create_user),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::audit::audit_logger,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::auth::require_auth,
        ));

    // Rute API Enterprise (Protected)
    let enterprise_api = Router::new()
        .route("/accounts/:account_id/limits", get(enterprise_limits::get_account_limits))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::auth::require_auth,
        ));

    // Webhook routes (Public endpoints for external channels: WhatsApp, Instagram, Facebook, Telegram, Shopee)
    let webhook_routes = Router::new()
        .route(
            "/whatsapp/:phone_number",
            get(webhooks::whatsapp::verify_webhook).post(webhooks::whatsapp::process_webhook),
        )
        .route(
            "/instagram",
            get(webhooks::instagram::verify_webhook).post(webhooks::instagram::process_webhook),
        )
        .route(
            "/facebook",
            get(webhooks::facebook::verify_webhook).post(webhooks::facebook::process_webhook),
        )
        .route(
            "/telegram/:bot_token",
            post(webhooks::telegram::process_webhook),
        )
        .route(
            "/shopee",
            post(webhooks::shopee::process_webhook),
        )
        .route(
            "/email",
            post(webhooks::email::handle_inbound_email),
        )
        .route(
            "/tokopedia",
            get(webhooks::tokopedia::handle_tokopedia_handshake).post(webhooks::tokopedia::handle_tokopedia_webhook),
        )
        .route(
            "/slack",
            post(integrations::slack_webhook),
        )
        .route(
            "/sms/twilio",
            post(webhooks::sms::twilio_sms_webhook),
        )
        .route(
            "/sms/bandwidth",
            post(webhooks::sms::bandwidth_sms_webhook),
        )
        .route(
            "/line",
            post(webhooks::line::line_webhook),
        )
        .route(
            "/twitter",
            get(webhooks::twitter::twitter_crc).post(webhooks::twitter::twitter_webhook),
        )
        .route(
            "/voice/twilio/inbound",
            post(calls::twilio_voice_inbound),
        )
        .route(
            "/voice/twilio/status",
            post(calls::twilio_voice_status),
        )
        .route(
            "/voice/twilio/recordings",
            post(calls::twilio_voice_recordings),
        );

    // Layanan static file untuk upload dan aset public (mendukung Docker /app dan lokal)
    let uploads_path = std::env::var("STORAGE_DIR").unwrap_or_else(|_| {
        if std::path::Path::new("/app/storage").exists() {
            "/app/storage".to_string()
        } else {
            "./storage".to_string()
        }
    });

    let public_path = std::env::var("PUBLIC_DIR").unwrap_or_else(|_| {
        if std::path::Path::new("/app/public").exists() {
            "/app/public".to_string()
        } else if std::path::Path::new("./public").exists() {
            "./public".to_string()
        } else {
            "../public".to_string()
        }
    });

    let uploads_service = ServeDir::new(uploads_path);
    let static_service = ServeDir::new(public_path);

    Router::new()
        // Frontend SPA routes
        .route("/", get(app_frontend::root_redirect))
        .route("/app", get(app_frontend::render_vue_app))
        .route("/app/", get(app_frontend::render_vue_app))
        .route("/app/*path", get(app_frontend::render_vue_app))
        // Vite Dev Server Proxy (HMR assets & modules)
        .route("/vite-dev/*path", get(app_frontend::vite_proxy_handler))
        .route("/@vite/*path", get(app_frontend::vite_proxy_handler))
        .route("/@fs/*path", get(app_frontend::vite_proxy_handler))
        // Rails Facebook Messenger engine fallback endpoint (/bot)
        .route(
            "/bot",
            get(webhooks::facebook::verify_webhook).post(webhooks::facebook::process_webhook),
        )
        // Instagram OAuth Callback
        .route(
            "/instagram/callback",
            get(callbacks::instagram_callback),
        )
        // Devise Auth endpoints
        .nest("/auth", auth_routes)
        // API v1 (Protected)
        .nest("/api/v1", api_v1)
        // Enterprise API v1 (Protected)
        .nest("/enterprise/api/v1", enterprise_api)
        // API v2 (Protected)
        .nest("/api/v2", api_v2)
        // Super Admin Dashboard API
        .nest("/super_admin/api/v1", super_admin_api)
        // Webhooks (Public)
        .nest("/webhooks", webhook_routes.clone())
        .nest("/api/v1/webhooks", webhook_routes)
        // Storage upload static files
        .nest_service("/storage", uploads_service)
        // Public assets fallback (favicon, brand-assets, dll)
        .fallback_service(static_service)
}

