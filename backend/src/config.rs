use std::env;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    pub frontend_url: String,
    pub secret_key_base: String,
    pub max_db_connections: u32,
    pub whatsapp_app_secret: Option<String>,
    pub whatsapp_verify_token: Option<String>,
    pub whatsapp_cloud_base_url: String,
    pub instagram_app_secret: Option<String>,
    pub instagram_verify_token: Option<String>,
    pub instagram_graph_base_url: String,
    pub fb_app_secret: Option<String>,
    pub fb_verify_token: Option<String>,
    pub fb_graph_base_url: String,
    pub telegram_api_base_url: String,
    pub shopee_api_base_url: String,
    pub llm_gateway_url: String,
    pub llm_gateway_api_key: String,
    pub llm_gateway_platform_id: String,
    pub default_llm_provider: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let database_url = if let Ok(url) = env::var("DATABASE_URL") {
            url
        } else {
            let user = env::var("POSTGRES_USERNAME").unwrap_or_else(|_| "postgres".to_string());
            let password = env::var("POSTGRES_PASSWORD").unwrap_or_default();
            let host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
            let port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
            let db = env::var("POSTGRES_DATABASE").unwrap_or_else(|_| "chatwoot_dev".to_string());

            if password.is_empty() {
                format!("postgres://{}@{}:{}/{}", user, host, port, db)
            } else {
                format!("postgres://{}:{}@{}:{}/{}", user, password, host, port, db)
            }
        };

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(3000);

        let frontend_url = env::var("FRONTEND_URL")
            .unwrap_or_else(|_| format!("http://localhost:{}", port));

        let secret_key_base = env::var("SECRET_KEY_BASE")
            .unwrap_or_else(|_| "chatwoot_rust_secret_key_base_default_development_key".to_string());

        let max_db_connections = env::var("RAILS_MAX_THREADS")
            .or_else(|_| env::var("DB_POOL_SIZE"))
            .ok()
            .and_then(|c| c.parse::<u32>().ok())
            .unwrap_or(10);

        let whatsapp_app_secret = env::var("WHATSAPP_APP_SECRET").ok();
        let whatsapp_verify_token = env::var("WHATSAPP_VERIFY_TOKEN").ok();
        let whatsapp_cloud_base_url = env::var("WHATSAPP_CLOUD_BASE_URL")
            .unwrap_or_else(|_| "https://graph.facebook.com".to_string());

        let instagram_app_secret = env::var("INSTAGRAM_APP_SECRET")
            .or_else(|_| env::var("IG_APP_SECRET"))
            .or_else(|_| env::var("FB_APP_SECRET"))
            .ok();
        let instagram_verify_token = env::var("INSTAGRAM_VERIFY_TOKEN")
            .or_else(|_| env::var("IG_VERIFY_TOKEN"))
            .ok();
        let instagram_graph_base_url = env::var("INSTAGRAM_GRAPH_BASE_URL")
            .unwrap_or_else(|_| "https://graph.instagram.com".to_string());

        let fb_app_secret = env::var("FB_APP_SECRET").ok();
        let fb_verify_token = env::var("FB_VERIFY_TOKEN")
            .or_else(|_| env::var("FACEBOOK_VERIFY_TOKEN"))
            .ok();
        let fb_graph_base_url = env::var("FB_GRAPH_BASE_URL")
            .unwrap_or_else(|_| "https://graph.facebook.com".to_string());

        let telegram_api_base_url = env::var("TELEGRAM_API_BASE_URL")
            .unwrap_or_else(|_| "https://api.telegram.org".to_string());

        let shopee_api_base_url = env::var("SHOPEE_API_BASE_URL")
            .unwrap_or_else(|_| "https://partner.shopeemobile.com".to_string());

        let llm_gateway_url = env::var("LLM_GATEWAY_URL")
            .or_else(|_| env::var("BOSON_GATEWAY_URL"))
            .or_else(|_| env::var("OPENAI_API_BASE"))
            .unwrap_or_else(|_| "http://127.0.0.1:8088/v1".to_string());
        let llm_gateway_api_key = env::var("LLM_GATEWAY_API_KEY")
            .or_else(|_| env::var("BOSON_API_KEY"))
            .or_else(|_| env::var("OPENAI_API_KEY"))
            .unwrap_or_else(|_| "sk_boson_chatwoot_master_key_998877".to_string());
        let llm_gateway_platform_id = env::var("LLM_GATEWAY_PLATFORM_ID")
            .or_else(|_| env::var("BOSON_PLATFORM_ID"))
            .unwrap_or_else(|_| "chatwoot-captain".to_string());
        let default_llm_provider = env::var("DEFAULT_LLM_PROVIDER").unwrap_or_else(|_| "gateway".to_string());

        Ok(Self {
            database_url,
            host,
            port,
            frontend_url,
            secret_key_base,
            max_db_connections,
            whatsapp_app_secret,
            whatsapp_verify_token,
            whatsapp_cloud_base_url,
            instagram_app_secret,
            instagram_verify_token,
            instagram_graph_base_url,
            fb_app_secret,
            fb_verify_token,
            fb_graph_base_url,
            telegram_api_base_url,
            shopee_api_base_url,
            llm_gateway_url,
            llm_gateway_api_key,
            llm_gateway_platform_id,
            default_llm_provider,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = Config::from_env().expect("Config should parse with defaults");
        assert_eq!(config.port, 3000);
        assert!(!config.database_url.is_empty());
        assert!(!config.secret_key_base.is_empty());
    }
}
