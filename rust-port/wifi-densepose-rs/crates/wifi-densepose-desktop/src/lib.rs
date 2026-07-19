pub mod auth;
pub mod commands;
pub mod domain;
pub mod provision;
pub mod state;

use commands::{discovery, flash, license, ota, pi_node, server, settings, telemetry as telemetry_cmd, wasm};
use commands::provision as provision_cmd;
use commands::auth as auth_cmd;
use commands::users;
use commands::roles;
use commands::enterprise;
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .setup(|app| {
            let app_handle = app.handle().clone();
            // Initialize auth system in background
            tauri::async_runtime::spawn(async move {
                if let Err(e) = init_auth_system(app_handle).await {
                    println!("CRITICAL AUTH INIT ERROR: {}", e);
                    tracing::error!("Failed to initialize auth system: {}", e);
                }
            });
            // Auto-start the sensing server so the app is operational without
            // a manual Start. Configuration comes from persisted settings when
            // present (ports, source, bind address, tick, model/RVF/node-
            // position paths); otherwise fall back to the server binary's own
            // defaults (HTTP 8080, WS 8765, UDP 5005, Nexmon 5500). Ports must
            // be explicit (not None): the frontend SensingProvider reads
            // ws_port from server status to open the live stream — with no
            // port recorded, the UI never connects and every telemetry card
            // stays blank. If something already listens on the configured HTTP
            // port (e.g. an orphaned or manually started server), the spawn is
            // skipped and the external server is adopted rather than killed.
            // The Sensing page can stop/restart with custom settings.
            let server_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let config = match settings::load_settings_inner(&server_handle) {
                    Ok(Some(saved)) => server::ServerConfig::from_settings(&saved),
                    Ok(None) => server::ServerConfig::with_default_ports(),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load settings for auto-start, using defaults: {e}"
                        );
                        server::ServerConfig::with_default_ports()
                    }
                };
                let state = server_handle.state::<state::AppState>();
                let http_port = config.http_port.unwrap_or(4000);
                if server::tcp_port_in_use(config.bind_address.as_deref(), http_port) {
                    tracing::info!(
                        "Port {http_port} already in use — adopting external sensing \
                         server instead of spawning a duplicate"
                    );
                    server::adopt_external_server(state.inner(), &config);
                    return;
                }
                match server::start_server_impl(&server_handle, &config, state.inner()).await {
                    Ok(r) => tracing::info!("Sensing server auto-started (pid {})", r.pid),
                    Err(e) => tracing::warn!("Sensing server auto-start failed: {e}"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Auth
            auth_cmd::login,
            auth_cmd::logout,
            auth_cmd::refresh_token,
            auth_cmd::get_current_user,
            auth_cmd::change_password,
            // User management
            users::list_users,
            users::get_user,
            users::create_user,
            users::update_user,
            users::delete_user,
            users::assign_user_role,
            // Roles & permissions
            roles::list_roles,
            roles::get_role,
            roles::create_role,
            roles::delete_role,
            roles::set_role_permissions,
            roles::list_modules,
            roles::list_tenant_modules,
            // License & Tenants
            license::license_status,
            license::activate_license,
            license::list_tenants,
            license::create_tenant,
            license::assign_tenant_modules,
            license::delete_tenant,
            // Discovery
            discovery::discover_nodes,
            discovery::list_serial_ports,
            discovery::configure_esp32_wifi,
            discovery::host_network_info,
            discovery::scan_wifi_networks,
            discovery::wifi_saved_password,
            // Flash
            flash::flash_firmware,
            flash::flash_progress,
            flash::verify_firmware,
            flash::check_espflash,
            flash::supported_chips,
            // OTA
            ota::ota_update,
            ota::batch_ota_update,
            ota::check_ota_endpoint,
            // WASM
            wasm::wasm_list,
            wasm::wasm_upload,
            wasm::wasm_control,
            wasm::wasm_info,
            wasm::wasm_stats,
            wasm::check_wasm_support,
            // Pi node management
            pi_node::pi_node_probe,
            pi_node::pi_node_build_agent,
            pi_node::pi_node_deploy_binary,
            pi_node::pi_node_check_prereqs,
            pi_node::pi_node_csi_health,
            pi_node::pi_node_push_config,
            pi_node::pi_node_install_service,
            pi_node::pi_node_service,
            pi_node::pi_node_install_nexmon,
            // Server
            server::start_server,
            server::stop_server,
            server::server_status,
            server::restart_server,
            server::server_logs,
            // Provision (serial-based, legacy)
            provision_cmd::provision_node,
            provision_cmd::read_nvs,
            provision_cmd::erase_nvs,
            provision_cmd::validate_config,
            provision_cmd::generate_mesh_configs,
            // Firmware bundle flashing + native NVS provisioning (onboarding W1)
            flash::fetch_firmware_release,
            flash::flash_firmware_bundle,
            provision_cmd::provision_esp32_nvs,
            provision_cmd::esp32_serial_check,
            // Settings
            settings::get_settings,
            settings::save_settings,
            // Telemetry / Langfuse
            telemetry_cmd::set_langfuse_config,
            telemetry_cmd::get_langfuse_config,
            // Enterprise
            enterprise::get_enterprise_settings,
            enterprise::save_enterprise_settings,
            enterprise::whapi_get_qr,
            enterprise::whapi_send_test,
            enterprise::whapi_send_alert,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                // Kill the managed sensing server so it doesn't orphan and
                // hold ports 8080/8765/5005/5500 for the next launch. An
                // adopted external server has no child handle, so it is
                // naturally left running.
                let state = app_handle.state::<state::AppState>();
                let lock = state.server.lock();
                if let Ok(mut srv) = lock {
                    if let Some(mut child) = srv.child.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    srv.running = false;
                    srv.pid = None;
                }
            }
        });
}

/// Initialize the auth subsystem: SQLite pool, JWT secret, module seeding.
async fn init_auth_system(app_handle: tauri::AppHandle) -> Result<(), String> {
    use crate::auth::{db, jwt, rate_limiter::RateLimiter, seed};

    // Get app data directory via Manager trait
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {}", e))?;

    // Ensure directory exists
    std::fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Cannot create app data dir: {}", e))?;

    // Initialize SQLite pool
    let pool = db::init_pool(&app_data_dir)
        .await
        .map_err(|e| format!("Database init failed: {}", e))?;

    // Get or create JWT secret
    let jwt_secret = jwt::get_or_create_jwt_secret(&pool)
        .await
        .map_err(|e| format!("JWT secret init failed: {}", e))?;

    // Seed access modules
    seed::seed_modules(&pool)
        .await
        .map_err(|e| format!("Module seeding failed: {}", e))?;

    // Create the auth manager and register it as Tauri state
    let auth_manager = Arc::new(auth_cmd::AuthManager {
        pool,
        jwt_secret,
        login_limiter: RateLimiter::default_login(),
    });

    app_handle.manage(auth_manager);

    tracing::info!("Auth system initialized successfully");
    Ok(())
}
