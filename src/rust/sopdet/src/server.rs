//! Synchronous HTTP server for the sopdet ground station.
//!
//! Exposes a simple JSON API over Wi-Fi that the Android app connects to.
//! No authentication is required.
//!
//! # Endpoints
//!
//! | Method | Path                  | Description                                      |
//! |--------|-----------------------|--------------------------------------------------|
//! | GET    | `/`                   | Alias for `/status`                              |
//! | GET    | `/status`             | Station health, GPS fix, survey-in state, uptime |
//! | GET    | `/telemetry/latest`   | Most recently received downlink packet           |
//! | GET    | `/telemetry/history`  | All buffered packets (`?limit=N`, default 100)   |
//! | POST   | `/command/emergency`  | Queue an `EmergencyLocate` command               |
//! | POST   | `/command/deploy`     | Queue a `DeployEjectionCharge` command           |
//!
//! All responses are JSON with `Content-Type: application/json` and
//! `Access-Control-Allow-Origin: *` (for cross-origin Android WebView requests).

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tiny_http::{Header, Request, Response, Server};

use crate::logger::{AccessLogEntry, Logger};
use crate::state::AppState;

// ── JSON response types ───────────────────────────────────────────────────────

/// Serialisable view of a received downlink telemetry packet.
#[derive(Serialize)]
struct TelemetryJson {
    /// Unix timestamp (milliseconds since epoch) of when this packet was received.
    received_at: u64,
    sequence_num: u16,
    /// Milliseconds since rocket boot.
    timestamp_ms: u32,
    altitude_m: f32,
    velocity_mps: f32,
    accel_z_gs: f32,
    gps_lat: f64,
    gps_lon: f64,
    gps_alt_m: f32,
    /// Human-readable RTK fix type, e.g. `"RTK-Fixed"`, `"GPS"`, `"NoFix"`.
    rtk_fix: String,
    pyro_deployed: bool,
    pyro_continuity: bool,
    /// 0 = Standby, 1 = MotorBurn, 2 = Coast, 3 = Freefall, 4 = Landed.
    flight_state: u8,
    /// Received signal strength (dBm).
    rssi: i16,
    /// Signal-to-noise ratio (dB).
    snr: f32,
}

/// Serialisable station-status snapshot.
#[derive(Serialize)]
struct StatusJson {
    /// Seconds since sopdet started.
    uptime_seconds: u64,
    /// True once the GPS survey-in has fully converged.
    svin_complete: bool,
    /// True while the GPS survey-in is actively in progress.
    svin_active: bool,
    /// Latest GPS fix from the base-station LC29H (null if not yet received).
    gps_fix: Option<GpsFixJson>,
    /// Number of downlink telemetry packets buffered in memory.
    telemetry_count: usize,
    /// RSSI of the most recently received downlink packet (null if none yet).
    last_downlink_rssi: Option<i16>,
}

/// Serialisable GPS fix snapshot.
#[derive(Serialize)]
struct GpsFixJson {
    latitude: f64,
    longitude: f64,
    altitude_m: f32,
    /// Human-readable fix quality, e.g. `"NoFix"`, `"GpsFix"`, `"RtkFixed"`.
    fix_quality: String,
    satellites_used: u8,
    hdop: f32,
}

/// Returned by POST command endpoints on success.
#[derive(Serialize)]
struct CommandResponse {
    status: &'static str,
    command: &'static str,
}

/// Returned by POST /resurvey on success.
#[derive(Serialize)]
struct ResurveyResponse {
    status: &'static str,
}

/// Returned for any error condition.
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ── Server entry point ────────────────────────────────────────────────────────

/// Run the synchronous HTTP server loop (blocks forever — call from a dedicated
/// thread).
///
/// Uses `tiny_http`'s `incoming_requests()` iterator which blocks until a
/// request arrives, handles it, and then waits for the next one.  The server
/// thread is entirely dedicated to request handling, so blocking is fine.
pub fn run_server(addr: &str, state: Arc<Mutex<AppState>>, logger: Logger) {
    let server = match Server::http(addr) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Cannot bind HTTP server on '{}': {}", addr, e);
            return;
        }
    };

    log::info!("HTTP server listening on http://{}", addr);
    log::info!(
        "Endpoints: GET /status  GET /telemetry/latest  GET /telemetry/history  POST /command/emergency  POST /command/deploy"
    );

    for request in server.incoming_requests() {
        handle_request(request, &state, &logger);
    }

    log::warn!("HTTP server stopped (incoming_requests iterator ended)");
}

// ── Request dispatch ──────────────────────────────────────────────────────────

fn handle_request(request: Request, state: &Arc<Mutex<AppState>>, logger: &Logger) {
    // Capture metadata before consuming the request.
    let method = request.method().to_string();
    let full_url = request.url().to_string();
    let client_addr = request
        .remote_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Split path from query string.
    let (path, query) = match full_url.find('?') {
        Some(pos) => (
            full_url[..pos].to_string(),
            Some(full_url[pos + 1..].to_string()),
        ),
        None => (full_url.clone(), None),
    };

    let (status_code, body) = route(method.as_str(), path.as_str(), query.as_deref(), state);

    // Build response with JSON content type and permissive CORS header.
    let content_type: Header = "Content-Type: application/json"
        .parse()
        .expect("static header is valid");
    let cors: Header = "Access-Control-Allow-Origin: *"
        .parse()
        .expect("static header is valid");

    let response = Response::from_string(body)
        .with_status_code(status_code)
        .with_header(content_type)
        .with_header(cors);

    if let Err(e) = request.respond(response) {
        log::warn!("Failed to send HTTP response to {}: {}", client_addr, e);
    }

    // Log the access.
    logger.log_access(AccessLogEntry {
        timestamp_ms: crate::logger::unix_ms(),
        method,
        path,
        client_addr,
        response_code: status_code,
    });
}

// ── Route table ───────────────────────────────────────────────────────────────

/// Map `(method_str, path)` to `(HTTP status code, JSON response body)`.
fn route(
    method: &str,
    path: &str,
    query: Option<&str>,
    state: &Arc<Mutex<AppState>>,
) -> (u16, String) {
    match (method, path) {
        // ── Station status ────────────────────────────────────────────────────
        ("GET", "/") | ("GET", "/status") => {
            let s = state.lock().unwrap();

            let gps_fix = s.latest_gps.as_ref().map(|g| GpsFixJson {
                latitude: g.latitude,
                longitude: g.longitude,
                altitude_m: g.altitude_m,
                fix_quality: format!("{:?}", g.fix_quality),
                satellites_used: g.satellites_used,
                hdop: g.hdop,
            });

            let payload = StatusJson {
                uptime_seconds: s.uptime_start.elapsed().as_secs(),
                svin_complete: s.svin_complete,
                svin_active: s.svin_active,
                gps_fix,
                telemetry_count: s.telemetry.len(),
                last_downlink_rssi: s.last_downlink_rssi,
            };

            (
                200,
                serde_json::to_string(&payload).unwrap_or_else(|e| error_json(&e.to_string())),
            )
        }

        // ── Latest telemetry packet ───────────────────────────────────────────
        ("GET", "/telemetry/latest") => {
            let s = state.lock().unwrap();
            match s.telemetry.last() {
                Some(entry) => (
                    200,
                    serde_json::to_string(&entry_to_json(entry))
                        .unwrap_or_else(|e| error_json(&e.to_string())),
                ),
                None => (204, error_json("no telemetry received yet")),
            }
        }

        // ── Telemetry history ─────────────────────────────────────────────────
        // Optional query param: ?limit=N  (default 100, capped at 1000)
        ("GET", "/telemetry/history") => {
            let limit = parse_limit(query, 100);
            let s = state.lock().unwrap();
            // Return the most recent `limit` entries, newest first.
            let entries: Vec<TelemetryJson> = s
                .telemetry
                .iter()
                .rev()
                .take(limit)
                .map(entry_to_json)
                .collect();
            (
                200,
                serde_json::to_string(&entries).unwrap_or_else(|e| error_json(&e.to_string())),
            )
        }

        // ── Emergency Locate ON command ───────────────────────────────────────
        ("POST", "/command/emergency") | ("POST", "/command/emergency/on") => {
            if let Ok(mut s) = state.lock() {
                s.pending_commands
                    .push_back(protocol::GroundCommand::EmergencyLocate);
                log::info!("HTTP: queued EmergencyLocate command");
            }
            (
                200,
                serde_json::to_string(&CommandResponse {
                    status: "queued",
                    command: "EmergencyLocate",
                })
                .unwrap_or_else(|e| error_json(&e.to_string())),
            )
        }

        // ── Emergency Locate OFF command ──────────────────────────────────────
        ("POST", "/command/emergency/off") => {
            if let Ok(mut s) = state.lock() {
                s.pending_commands
                    .push_back(protocol::GroundCommand::EmergencyLocateOff);
                log::info!("HTTP: queued EmergencyLocateOff command");
            }
            (
                200,
                serde_json::to_string(&CommandResponse {
                    status: "queued",
                    command: "EmergencyLocateOff",
                })
                .unwrap_or_else(|e| error_json(&e.to_string())),
            )
        }

        // ── Deploy Ejection Charge command ────────────────────────────────────
        ("POST", "/command/deploy") => {
            if let Ok(mut s) = state.lock() {
                s.pending_commands
                    .push_back(protocol::GroundCommand::DeployEjectionCharge);
                log::warn!("HTTP: queued DeployEjectionCharge command");
            }
            (
                200,
                serde_json::to_string(&CommandResponse {
                    status: "queued",
                    command: "DeployEjectionCharge",
                })
                .unwrap_or_else(|e| error_json(&e.to_string())),
            )
        }

        // ── Re-survey base station position ───────────────────────────────────
        // Use this after moving the base station.  Resets the survey-in state
        // and instructs the GPS thread to re-run the full configuration sequence.
        ("POST", "/resurvey") => {
            if let Ok(mut s) = state.lock() {
                s.resurvey_requested = true;
                s.svin_complete = false;
                s.svin_active = false;
                log::info!("HTTP: resurvey requested — GPS survey-in will restart");
            }
            (
                200,
                serde_json::to_string(&ResurveyResponse {
                    status: "resurveying",
                })
                .unwrap_or_else(|e| error_json(&e.to_string())),
            )
        }

        // ── OPTIONS preflight (for browsers / WebView CORS) ──────────────────
        ("OPTIONS", _) => (204, String::new()),

        // ── 404 catch-all ─────────────────────────────────────────────────────
        _ => (404, error_json(&format!("not found: {} {}", method, path))),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert a [`crate::state::TelemetryEntry`] to its JSON-serialisable form.
fn entry_to_json(e: &crate::state::TelemetryEntry) -> TelemetryJson {
    TelemetryJson {
        received_at: e.received_at,
        sequence_num: e.sequence_num,
        timestamp_ms: e.timestamp_ms,
        altitude_m: e.altitude_m,
        velocity_mps: e.velocity_mps,
        accel_z_gs: e.accel_z_gs,
        gps_lat: e.gps_lat,
        gps_lon: e.gps_lon,
        gps_alt_m: e.gps_alt_m,
        rtk_fix: e.rtk_fix.clone(),
        pyro_deployed: e.pyro_deployed,
        pyro_continuity: e.pyro_continuity,
        flight_state: e.flight_state,
        rssi: e.rssi,
        snr: e.snr,
    }
}

/// Parse an optional `limit=N` query parameter.
///
/// Falls back to `default` if the parameter is absent or unparseable.
/// Caps the result at [`crate::state::MAX_TELEMETRY_HISTORY`].
fn parse_limit(query: Option<&str>, default: usize) -> usize {
    let limit = query
        .and_then(|q| q.split('&').find(|p| p.starts_with("limit=")))
        .and_then(|p| p.strip_prefix("limit="))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default);
    limit.min(crate::state::MAX_TELEMETRY_HISTORY)
}

/// Serialise a simple `{"error": "..."}` JSON object.
fn error_json(msg: &str) -> String {
    serde_json::to_string(&ErrorResponse {
        error: msg.to_string(),
    })
    .unwrap_or_else(|_| r#"{"error":"serialisation failed"}"#.to_string())
}
