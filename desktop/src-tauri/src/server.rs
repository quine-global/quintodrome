use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};
use thiserror::Error;

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 4533;

const READY_TIMEOUT: Duration = Duration::from_secs(60);
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const HEALTH_PATH: &str = "/ping";

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("could not resolve the application data folder")]
    DataFolder,
    #[error("could not resolve the music folder")]
    MusicFolder,
    #[error("failed to spawn navidrome: {0}")]
    Spawn(#[from] tauri_plugin_shell::Error),
    #[error("navidrome did not become ready within {READY_TIMEOUT:?}")]
    Timeout,
}

/// Holds the handle to the Navidrome child process so it can be shut down
/// gracefully when the app exits. It is `None` when Navidrome was already
/// running before the app started.
#[derive(Default)]
pub struct ServerState {
    child: Mutex<Option<CommandChild>>,
}

pub struct Navidrome {
    pub host: String,
    pub port: u16,
    pub url: String,
}

impl Navidrome {
    pub fn new(host: String, port: u16) -> Self {
        let url = format!("http://{host}:{port}");
        Self { host, port, url }
    }
}

/// Ensures a Navidrome server is reachable at `navidrome.url`. If one is
/// already listening, it is reused as-is. Otherwise a bundled Navidrome binary
/// is spawned as a separate child process and we wait for it to become ready.
pub fn ensure_running(handle: &AppHandle, navidrome: &Navidrome) -> Result<(), ServerError> {
    if is_ready(&navidrome.host, navidrome.port) {
        eprintln!(
            "quintodrome: navidrome is already running at {}",
            navidrome.url
        );
        return Ok(());
    }

    eprintln!("quintodrome: navidrome is not running, starting it...");
    spawn(handle, navidrome)?;
    wait_until_ready(&navidrome.host, navidrome.port)?;
    eprintln!("quintodrome: navidrome is ready at {}", navidrome.url);
    Ok(())
}

/// Bundle identifier used before the rename to com.quineglobal.drome. The app
/// data folder is named after the identifier, so it moved with the rename.
const LEGACY_IDENTIFIER: &str = "com.quintodrome.desktop";

/// Moves Navidrome's data (database, cache, backups) from the pre-rename data
/// folder to `data_folder`, once, so existing users keep their library. Does
/// nothing if the new folder already exists or there is no old folder.
fn migrate_legacy_data_folder(data_folder: &std::path::Path) {
    let Some(legacy) = data_folder.parent().map(|p| p.join(LEGACY_IDENTIFIER)) else {
        return;
    };
    if data_folder.exists() || !legacy.is_dir() {
        return;
    }
    match std::fs::rename(&legacy, data_folder) {
        Ok(()) => eprintln!(
            "quintodrome: moved data folder {} -> {}",
            legacy.display(),
            data_folder.display()
        ),
        Err(err) => eprintln!(
            "quintodrome: could not move data folder {}: {err}",
            legacy.display()
        ),
    }
}

fn spawn(handle: &AppHandle, navidrome: &Navidrome) -> Result<(), ServerError> {
    let data_folder = handle
        .path()
        .app_data_dir()
        .map_err(|_| ServerError::DataFolder)?;
    migrate_legacy_data_folder(&data_folder);
    std::fs::create_dir_all(&data_folder).ok();

    let music_folder = match std::env::var_os("QUINTODROME_MUSIC_FOLDER") {
        Some(path) if !path.is_empty() => path.into(),
        _ => handle
            .path()
            .audio_dir()
            .map_err(|_| ServerError::MusicFolder)?,
    };
    std::fs::create_dir_all(&music_folder).ok();

    let data_folder = data_folder.to_string_lossy().to_string();
    let music_folder = music_folder.to_string_lossy().to_string();
    let port = navidrome.port.to_string();

    let args = vec![
        "--nobanner".to_string(),
        "--address".to_string(),
        navidrome.host.clone(),
        "--port".to_string(),
        port,
        "--datafolder".to_string(),
        data_folder,
        "--musicfolder".to_string(),
        music_folder,
    ];

    // Auto-provision the first admin user on a fresh install. Navidrome only
    // honors this during initial setup (when no users/data exist), so an
    // existing installation is left untouched.
    let admin_password =
        std::env::var("QUINTODROME_ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());

    let (mut rx, child) = handle
        .shell()
        .sidecar("navidrome")?
        .env(
            "ND_DEVAUTOCREATEADMINPASSWORD",
            admin_password.as_str(),
        )
        .env("ND_ENABLEINSIGHTSCOLLECTOR", "false")
        .env("ND_DEVAUTOLOGINUSERNAME", "admin")
        .args(args)
        .spawn()?;

    let pid = child.pid();
    eprintln!("quintodrome: started navidrome (pid {pid})");

    *handle.state::<ServerState>().child.lock().unwrap() = Some(child);

    // Forward Navidrome's output to our own stdout/stderr.
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    print!("{}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Stderr(line) => {
                    eprint!("{}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Terminated(payload) => {
                    eprintln!(
                        "quintodrome: navidrome exited with code {:?}",
                        payload.code
                    );
                }
                _ => {}
            }
        }
    });

    Ok(())
}

fn wait_until_ready(host: &str, port: u16) -> Result<(), ServerError> {
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if is_ready(host, port) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(ServerError::Timeout)
}

/// Performs a lightweight HTTP health check against Navidrome's `/ping`
/// endpoint. A successful `200` response means the server is up and serving.
fn is_ready(host: &str, port: u16) -> bool {
    let addr = match (host, port).to_socket_addrs().ok().and_then(|mut it| it.next()) {
        Some(addr) => addr,
        None => return false,
    };

    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
        Ok(stream) => stream,
        Err(_) => return false,
    };

    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

    let request = format!(
        "GET {HEALTH_PATH} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut response = Vec::new();
    let mut buffer = [0u8; 512];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buffer[..n]),
            Err(_) => break,
        }
    }

    let response = String::from_utf8_lossy(&response);
    response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200")
}

/// Sends SIGTERM to the spawned Navidrome process (so it can shut down
/// gracefully) and falls back to a hard kill if it does not exit in time.
pub fn shutdown(state: &ServerState) {
    let child = match state.child.lock().unwrap().take() {
        Some(child) => child,
        None => return,
    };

    let pid = child.pid();
    eprintln!("quintodrome: stopping navidrome (pid {pid})");

    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }

    let deadline = Instant::now() + GRACEFUL_SHUTDOWN_TIMEOUT;
    while Instant::now() < deadline {
        if !process_alive(pid) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }

    if let Err(err) = child.kill() {
        eprintln!("quintodrome: failed to kill navidrome: {err}");
    }
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    true
}
