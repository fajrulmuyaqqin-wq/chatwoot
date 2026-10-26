use axum::{
    body::Body,
    extract::State,
    http::{HeaderName, HeaderValue, StatusCode, Uri},
    response::{Html, IntoResponse, Redirect, Response},
};

use crate::state::AppState;

pub async fn root_redirect() -> impl IntoResponse {
    Redirect::to("/app/login")
}

pub async fn vite_proxy_handler(uri: Uri) -> impl IntoResponse {
    let client = reqwest::Client::new();
    let target = format!("http://localhost:3036{}", uri);

    match client.get(&target).send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut builder = Response::builder().status(status);

            for (key, val) in resp.headers().iter() {
                if let (Ok(h_key), Ok(h_val)) = (
                    HeaderName::from_bytes(key.as_str().as_bytes()),
                    HeaderValue::from_bytes(val.as_bytes()),
                ) {
                    builder = builder.header(h_key, h_val);
                }
            }

            let bytes = resp.bytes().await.unwrap_or_default();
            builder.body(Body::from(bytes)).unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap()
            })
        }
        Err(_) => Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Body::from("Vite Dev Server Unreachable"))
            .unwrap(),
    }
}

pub async fn render_vue_app(
    State(state): State<AppState>,
    uri: Uri,
) -> impl IntoResponse {
    let host_url = &state.config.frontend_url;
    let path = uri.path();

    // Pilih entrypoint script: v3app untuk auth/login/signup/reset, dashboard untuk agent app
    let entrypoint = if path.contains("/auth") || path.contains("/login") || path.contains("/signup") {
        "entrypoints/v3app.js"
    } else {
        "entrypoints/dashboard.js"
    };

    // Menghasilkan HTML bootstrap yang menginjeksi seluruh konfigurasi global ke window
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Chatwoot</title>
    <meta name="viewport" content="width=device-width, initial-scale=1, shrink-to-fit=no, user-scalable=0" />
    <link rel="icon" type="image/png" sizes="512x512" href="/brand-assets/logo_thumbnail.svg" />
    <link rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png" />
    <link rel="icon" type="image/png" sizes="16x16" href="/favicon-16x16.png" />
    <link rel="manifest" href="/manifest.json" />
    <meta name="csrf-param" content="authenticity_token" />
    <meta name="csrf-token" content="chatwoot_rust_token" />

    <script>
      window.chatwootConfig = {{
        hostURL: "{host_url}",
        helpCenterURL: "",
        fbAppId: "",
        instagramAppId: "",
        tiktokAppId: "",
        googleOAuthClientId: null,
        googleOAuthCallbackUrl: null,
        allowedLoginMethods: ["email", "saml"],
        fbApiVersion: "v14.0",
        whatsappAppId: "",
        whatsappConfigurationId: "",
        whatsappApiVersion: "v16.0",
        signupEnabled: "true",
        isEnterprise: "true",
        isMfaEnabled: "true",
        enterprisePlanName: "enterprise",
        enabledLanguages: [
          {{ "name": "English", "iso_639_1_code": "en" }},
          {{ "name": "Indonesian", "iso_639_1_code": "id" }}
        ],
        helpUrls: {{}},
        inboxEventsEnabled: "true",
        searchTimeRangeLimitDays: 30,
        autoResolveMinMinutes: 10,
        selectedLocale: "en"
      }};
      window.globalConfig = {{
        APP_VERSION: "4.1.0",
        GIT_SHA: "3564ec1e9",
        DISPLAY_MANIFEST: false,
        INSTALLATION_NAME: "Chatwoot",
        BRAND_NAME: "Chatwoot",
        BRAND_URL: "https://www.chatwoot.com",
        WIDGET_BRAND_URL: "https://www.chatwoot.com",
        TERMS_URL: "https://www.chatwoot.com/terms-of-service",
        PRIVACY_URL: "https://www.chatwoot.com/privacy-policy",
        LOGO: "/brand-assets/logo.svg",
        LOGO_DARK: "/brand-assets/logo_dark.svg",
        LOGO_THUMBNAIL: "/brand-assets/logo_thumbnail.svg",
        IS_ENTERPRISE: "true",
        IS_COMPANY_ENRICHMENT_ENABLED: "false",
        CREATE_NEW_ACCOUNT_FROM_DASHBOARD: true,
        CHATWOOT_INBOX_TOKEN: "",
        CHATWOOT_INBOX_HMAC_KEY: "",
        API_CHANNEL_NAME: "API Channel",
        API_CHANNEL_THUMBNAIL: "",
        DIRECT_UPLOADS_ENABLED: "true",
        MAXIMUM_FILE_UPLOAD_SIZE: 40,
        HCAPTCHA_SITE_KEY: "",
        LOGOUT_REDIRECT_LINK: "/",
        DISABLE_USER_PROFILE_UPDATE: false,
        DISABLE_META_INBOX_CREATION: false,
        DISABLE_META_MESSAGE_SENDING: false,
        DEPLOYMENT_ENV: "production",
        INSTALLATION_PRICING_PLAN: "enterprise",
        CAPTAIN_TOOLS_MANIFEST_ENABLED: "true",
        CAPTAIN_TOOLS_CATALOG_URL: "",
        ACTIVE_PLATFORM_BANNERS: [],
        ACTIVE_FEATURE_ANNOUNCEMENTS: []
      }};
      window.browserConfig = {{
        browser_name: "chrome"
      }};
      window.errorLoggingConfig = "";
    </script>

    <!-- Vite Scripts (proxied via port 3000 -> 3036, no direct localhost:3036 reference) -->
    <script type="module" src="/vite-dev/{entrypoint}"></script>

    <!-- Auto-mount helper: triggers window.onload() jika evaluasi ES module selesai setelah DOM load event -->
    <script>
      (function() {{
        var attempts = 0;
        var MAX_ATTEMPTS = 300;
        var interval = setInterval(function() {{
          attempts++;
          var appEl = document.getElementById('app');
          if (appEl && appEl.children.length === 0 && typeof window.onload === 'function') {{
            clearInterval(interval);
            try {{ window.onload(); }} catch(e) {{ console.error('[chatwoot] mount error:', e); }}
            return;
          }}
          if (appEl && appEl.children.length > 0) {{
            clearInterval(interval);
            return;
          }}
          if (attempts >= MAX_ATTEMPTS) {{
            clearInterval(interval);
            console.warn('[chatwoot] Auto-mount: timed out waiting for app to mount.');
          }}
        }}, 50);
      }})();
    </script>
  </head>
  <body class="text-slate-600">
    <div id="app"></div>
    <noscript id="noscript">This app works best with JavaScript enabled.</noscript>
  </body>
</html>
"#,
        host_url = host_url,
        entrypoint = entrypoint
    );

    Html(html)
}
