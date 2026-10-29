#![recursion_limit = "256"]

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

mod config;
mod middleware;
mod models;
mod routes;
mod state;
mod services;
mod websocket;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inisialisasi tracing / logger
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chatwoot_backend=info,tower_http=info".into()),
        )
        .init();

    // Muat file .env dari root repositori atau direktori kerja jika ada
    let _ = dotenvy::from_path("backend/.env");
    let _ = dotenvy::from_path("../.env");
    let _ = dotenvy::from_path(".env");
    let _ = dotenvy::dotenv();

    info!("Memulai Chatwoot Rust Backend...");

    let config = Config::from_env().map_err(|e| {
        error!("Gagal memuat konfigurasi: {}", e);
        e
    })?;

    info!("Konfigurasi database URL: {}", mask_db_url(&config.database_url));

    // Setup PostgreSQL Connection Pool
    let pool = match PgPoolOptions::new()
        .max_connections(config.max_db_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
    {
        Ok(pool) => {
            info!("Berhasil terhubung ke database PostgreSQL!");
            let _ = sqlx::query("ALTER TABLE audits ADD COLUMN IF NOT EXISTS city varchar, ADD COLUMN IF NOT EXISTS country varchar, ADD COLUMN IF NOT EXISTS country_code varchar")
                .execute(&pool)
                .await;
            pool
        }
        Err(err) => {
            warn!(
                "Peringatan: Belum dapat terhubung ke PostgreSQL ({}) - server tetap berjalan untuk routing/development.",
                err
            );
            PgPoolOptions::new()
                .max_connections(config.max_db_connections)
                .connect_lazy(&config.database_url)?
        }
    };

    let state = AppState::new(pool, config.clone());

    // Jalankan background worker presence reaper
    websocket::presence::spawn_presence_reaper(state.clone());

    // Jalankan background worker SLA policy evaluator (Enterprise)
    services::sla_evaluator::start_sla_evaluator(state.clone());

    // Jalankan background worker Scheduled Campaigns Executor
    let st_camp = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            services::campaign_executor::check_and_execute_scheduled_campaigns(&st_camp).await;
        }
    });

    // Setup CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([
            axum::http::HeaderName::from_static("access-token"),
            axum::http::HeaderName::from_static("token-type"),
            axum::http::HeaderName::from_static("client"),
            axum::http::HeaderName::from_static("expiry"),
            axum::http::HeaderName::from_static("uid"),
        ]);

    // Router utama yang menggabungkan seluruh routes
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api", get(api_root))
        .route("/cable", get(websocket::cable_handler))
        .merge(routes::create_router(state.clone()))
        .layer(axum::middleware::from_fn(security_headers))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("Server Chatwoot Rust berjalan di http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn security_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("SAMEORIGIN"),
    );
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    resp
}


async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let db_status = match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    let status = if db_status == "connected" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "status": if db_status == "connected" { "ok" } else { "degraded" },
            "database": db_status,
            "version": env!("CARGO_PKG_VERSION")
        })),
    )
}

async fn api_root() -> impl IntoResponse {
    Json(json!({
        "name": "Chatwoot Rust Backend API",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "online"
    }))
}

fn mask_db_url(url: &str) -> String {
    if let Some(at_idx) = url.find('@') {
        if let Some(colon_idx) = url[..at_idx].rfind(':') {
            return format!("{}****{}", &url[..colon_idx + 1], &url[at_idx..]);
        }
    }
    url.to_string()
}
