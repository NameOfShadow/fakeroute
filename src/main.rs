//! FakeRoute — A lightweight, file‑based mock API server.
//!
//! Watches a directory of `.json` files and serves them as REST endpoints.
//! Supports nested routes, hot reloading, and CORS out of the box.

use std::{
    ffi::OsStr,
    fs,
    net::SocketAddr,
    path::{Component, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, Response, StatusCode},
    routing::get,
    Router,
};
use chrono::Local;
use notify::{event::ModifyKind, Event, EventKind, RecursiveMode, Watcher};
use owo_colors::OwoColorize;
use tokio::{net::TcpListener, sync::mpsc};

// -----------------------------------------------------------------------------
// Application State
// -----------------------------------------------------------------------------

/// Global state shared across all request handlers.
struct AppState {
    base_dir: PathBuf,
}

// -----------------------------------------------------------------------------
// Entry Point
// -----------------------------------------------------------------------------

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = pico_args::Arguments::from_env();

    // Show help if requested
    if args.contains(["-h", "--help"]) {
        print_help();
        return;
    }

    // Parse command line arguments
    let current_port: u16 = args.value_from_str(["-p", "--port"]).unwrap_or(3000);
    let dir: String = args
        .value_from_str(["-d", "--dir"])
        .unwrap_or_else(|_| "mocks".to_string());

    // Find an available port (increment if the default is busy)
    let listener = bind_available_port(current_port).await;

    let base_dir = PathBuf::from(&dir);
    if !base_dir.exists() {
        let _ = fs::create_dir_all(&base_dir);
    }

    // Watch the mock directory for file changes
    spawn_file_watcher(base_dir.clone());

    let shared_state = Arc::new(AppState {
        base_dir: base_dir.clone(),
    });

    // Build the router with a catch‑all route and a custom 404 fallback
    let app = Router::new()
        .route("/{*path}", get(mock_handler))
        .with_state(shared_state)
        .fallback(not_found_handler);

    let final_addr = listener.local_addr().unwrap();

    // Display startup information
    render_banner(final_addr, &base_dir);
    println!("  {}", " ENDPOINTS ".on_white().black().bold());
    list_endpoints_recursive(&base_dir, &base_dir, "", current_port);
    println!();

    // Start the server with graceful shutdown on Ctrl+C
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server failed unexpectedly");
}

// -----------------------------------------------------------------------------
// Help & Banner
// -----------------------------------------------------------------------------

/// Print detailed usage instructions.
fn print_help() {
    println!(
        "\n  {} {}",
        " FakeRoute ".on_bright_magenta().black().bold(),
        "v".dimmed().to_string() + env!("CARGO_PKG_VERSION")
    );
    println!(
        "  {}\n",
        "Instant JSON API mock server for fast development."
            .italic()
            .dimmed()
    );

    println!("  {}", " USAGE ".on_white().black().bold());
    println!("    fakeroute [OPTIONS]\n");

    println!("  {}", " OPTIONS ".on_white().black().bold());
    println!(
        "    {:16} Set the server port {}",
        "-p, --port".green(),
        "[default: 3000]".dimmed()
    );
    println!(
        "    {:16} Set the mocks directory {}",
        "-d, --dir".green(),
        "[default: mocks]".dimmed()
    );
    println!(
        "    {:16} Print this help message",
        "-h, --help".green()
    );

    println!("\n  {}", " EXAMPLES ".on_white().black().bold());
    println!(
        "    $ {:26} {}",
        "fakeroute".cyan(),
        "# Start with defaults".dimmed()
    );
    println!(
        "    $ {:26} {}",
        "fakeroute -p 8080 -d ./api".cyan(),
        "# Custom port and folder".dimmed()
    );

    println!("\n  {} {}", "┃".bright_black(), "Happy coding!".magenta());
    println!();
}

/// Display a startup banner with server address and watched directory.
fn render_banner(addr: SocketAddr, mocks_dir: &PathBuf) {
    let abs_path = fs::canonicalize(mocks_dir).unwrap_or_else(|_| mocks_dir.clone());

    println!("\n  {}", " FakeRoute ".on_bright_magenta().black().bold());
    println!("  {} {}", "v".dimmed(), env!("CARGO_PKG_VERSION").dimmed());

    println!(
        "  {} {} {}",
        "┏".bright_black(),
        "Listening on:".bright_black(),
        format!("http://{}", addr).cyan().underline()
    );
    println!(
        "  {} {} {}",
        "┗".bright_black(),
        "Mocks folder:".bright_black(),
        abs_path.display().to_string().yellow()
    );
    println!();
}

// -----------------------------------------------------------------------------
// Network Helpers
// -----------------------------------------------------------------------------

/// Try to bind to a port, incrementing until an available one is found.
async fn bind_available_port(mut port: u16) -> TcpListener {
    loop {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        match TcpListener::bind(addr).await {
            Ok(listener) => return listener,
            Err(_) => {
                println!(
                    "  {} Port {} is busy, trying {}...",
                    "⚠".yellow(),
                    port,
                    port + 1
                );
                port += 1;
            }
        }
    }
}

/// Graceful shutdown signal (waits for Ctrl+C).
async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    println!(
        "\n\n  {}",
        "👋 FakeRoute stopped. See you later!"
            .bright_magenta()
            .bold()
    );
}

// -----------------------------------------------------------------------------
// Request Handlers
// -----------------------------------------------------------------------------

/// Core handler – maps a URL path to a `.json` file and returns its contents.
async fn mock_handler(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Response<Body> {
    // Ignore favicon requests quietly
    if path.ends_with("favicon.ico") {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    }

    let display_path = if path.is_empty() { "/" } else { &path };

    // Prevent path traversal attacks
    let mock_path = match build_safe_path(&state.base_dir, &path) {
        Ok(p) => p,
        Err(_) => return error_response(StatusCode::FORBIDDEN, "Invalid path"),
    };

    match tokio::fs::read_to_string(&mock_path).await {
        Ok(json) => {
            let file_name = mock_path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            log_request("200", display_path, &file_name, true);

            Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(json))
                .unwrap()
        }
        Err(_) => {
            log_request("404", display_path, "Not Found", false);
            error_response(StatusCode::NOT_FOUND, "Mock not found")
        }
    }
}

/// Fallback handler for unmatched routes (returns 404).
async fn not_found_handler(req: Request) -> (StatusCode, &'static str) {
    if !req.uri().path().ends_with("favicon.ico") {
        println!(
            "  {} {} {}",
            "○".bright_red(),
            "404".bright_red().bold(),
            req.uri()
        );
    }
    (StatusCode::NOT_FOUND, "Route not found")
}

// -----------------------------------------------------------------------------
// Endpoint Listing
// -----------------------------------------------------------------------------

/// Recursively print all available endpoints in a tree‑like structure.
fn list_endpoints_recursive(base: &PathBuf, current: &PathBuf, prefix: &str, port: u16) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| !e.path().is_dir()); // directories first

    let total = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let path = entry.path();
        let is_last = i == total - 1;
        let connector = if is_last { "└─" } else { "├─" };

        if path.is_dir() {
            println!(
                "  {}{}{}",
                prefix.bright_black(),
                connector.bright_black(),
                path.file_name().unwrap().to_string_lossy().bold().blue()
            );
            let new_prefix = format!("{}{}", prefix, if is_last { "  " } else { "│ " });
            list_endpoints_recursive(base, &path, &new_prefix, port);
        } else if path.extension() == Some(OsStr::new("json")) {
            if let Ok(rel) = path.strip_prefix(base) {
                let route_path = rel.with_extension("");
                let route_str = format!("/{}", route_path.display());

                let url = format!("http://localhost:{}{}", port, route_str);
                let clickable_route = make_osc8_link(&url, &route_str);

                println!(
                    "  {}{}{}  {}",
                    prefix.bright_black(),
                    connector.bright_black(),
                    "GET".bright_cyan().dimmed(),
                    clickable_route.white(),
                );
            }
        }
    }
}

/// Create a clickable OSC 8 hyperlink for supported terminal emulators.
fn make_osc8_link(url: &str, text: &str) -> String {
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, text)
}

// -----------------------------------------------------------------------------
// File Watching
// -----------------------------------------------------------------------------

/// Start a filesystem watcher that monitors the mock directory recursively.
fn spawn_file_watcher(path: PathBuf) {
    let (tx, mut rx) = mpsc::unbounded_channel();

    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .expect("Failed to initialize file watcher");

    watcher
        .watch(&path, RecursiveMode::Recursive)
        .expect("Failed to watch directory");

    // Keep the watcher alive for the lifetime of the process
    Box::leak(Box::new(watcher));

    tokio::spawn(async move {
        while let Some(Ok(event)) = rx.recv().await {
            process_file_event(event);
        }
    });
}

/// Handle a single filesystem event and log changes to `.json` files.
fn process_file_event(event: Event) {
    // Debounce duplicate modify events (common on some OSes)
    static LAST_EVENT: Mutex<Option<(PathBuf, Instant)>> = Mutex::new(None);

    if let Some(path) = event.paths.first() {
        if path.extension() != Some(OsStr::new("json")) {
            return;
        }

        let time = Local::now().format("%H:%M:%S").to_string();
        let file = path.file_name().unwrap_or_default().to_string_lossy();

        match event.kind {
            EventKind::Create(_) => {
                println!(
                    "{} {} {:>8} {}",
                    time.dimmed(),
                    "✚".cyan(),
                    "CREATED".cyan().bold(),
                    file.white().dimmed()
                );
            }
            EventKind::Modify(ModifyKind::Data(_)) => {
                let mut last = LAST_EVENT.lock().unwrap();
                if let Some((last_path, last_time)) = &*last {
                    if last_path == path && last_time.elapsed() < Duration::from_millis(100) {
                        return; // Duplicate event, skip
                    }
                }
                *last = Some((path.clone(), Instant::now()));

                println!(
                    "{} {} {:>8} {}",
                    time.dimmed(),
                    "⚡".yellow(),
                    "RELOADED".yellow().bold(),
                    file.white().dimmed()
                );
            }
            EventKind::Remove(_) => {
                println!(
                    "{} {} {:>8} {}",
                    time.dimmed(),
                    "✖".red(),
                    "REMOVED".red().bold(),
                    file.white().dimmed()
                );
            }
            _ => {}
        }
    }
}

// -----------------------------------------------------------------------------
// Logging & Utilities
// -----------------------------------------------------------------------------

/// Log an HTTP request in a compact, colored format.
fn log_request(status: &str, route: &str, info: &str, success: bool) {
    let time = Local::now().format("%H:%M:%S").to_string();

    let status_styled = if success {
        format!(" {:<3} ", status).on_green().black().bold().to_string()
    } else {
        format!(" {:<3} ", status).on_red().black().bold().to_string()
    };

    println!(
        "{} {} {} {:<25} {}",
        time.dimmed(),
        status_styled,
        "→".bright_black(),
        route.white().bold(),
        info.italic().dimmed()
    );
}

/// Construct a safe filesystem path, rejecting attempts to escape the base directory.
fn build_safe_path(base_dir: &PathBuf, request_path: &str) -> Result<PathBuf, ()> {
    let candidate = PathBuf::from(request_path);

    if candidate
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(());
    }

    let mut path = base_dir.clone();
    path.push(candidate);
    path.set_extension(OsStr::new("json"));
    Ok(path)
}

/// Return a JSON error response with the given status code and message.
fn error_response(code: StatusCode, msg: &str) -> Response<Body> {
    Response::builder()
        .status(code)
        .header(header::CONTENT_TYPE, "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(format!("{{\"error\": \"{}\"}}", msg)))
        .unwrap()
}