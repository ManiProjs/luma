//! Long-lived local gateway for Teammates background execution.
//!
//! The gateway owns the task supervisor independently of any UI process.
//! Desktop, Dashboard and Ink may connect and disconnect without taking
//! scheduled or queued teammate work down with them.

use anyhow::{Context, Result, bail};
#[cfg(unix)]
use std::fs::OpenOptions;
use std::{
    process::{Command, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::sleep,
};
use tokio_util::sync::CancellationToken;

use crate::{config, runtime::RuntimeStore, worker::RuntimeSupervisor};

const HANDSHAKE: &[u8] = b"LUMA_GATEWAY\n";

pub async fn ensure_running() -> Result<()> {
    if is_running().await {
        return Ok(());
    }

    let executable = std::env::current_exe().context("locating the Luma core executable")?;
    Command::new(executable)
        .arg("gateway")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("starting the persistent Luma gateway")?;

    for _ in 0..80 {
        if is_running().await {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }

    bail!("Luma gateway did not become ready; inspect the Luma log for startup errors")
}

/// Gateway management commands used by the Ink CLI and desktop shell.
pub async fn run_command(args: &[String]) -> Result<()> {
    match args {
        [] => run().await,
        [command] if command == "status" => {
            println!(
                "{}",
                if is_running().await {
                    "running"
                } else {
                    "stopped"
                }
            );
            Ok(())
        }
        [command] if command == "start" => {
            ensure_running().await?;
            println!("Luma gateway started.");
            Ok(())
        }
        [command] if command == "stop" => stop().await,
        [command] if command == "restart" => {
            stop().await?;
            ensure_running().await?;
            println!("Luma gateway restarted.");
            Ok(())
        }
        _ => bail!("usage: luma-core gateway [status|start|stop|restart]"),
    }
}

/// Stop the detached gateway through its owner-only local control socket.
pub async fn stop() -> Result<()> {
    if !is_running().await {
        println!("Luma gateway is already stopped.");
        return Ok(());
    }
    send_stop_request().await?;
    for _ in 0..80 {
        if !is_running().await {
            println!("Luma gateway stopped.");
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    bail!("Luma gateway did not stop within 8 seconds")
}

pub async fn run() -> Result<()> {
    #[cfg(unix)]
    return run_unix().await;

    #[cfg(not(unix))]
    return run_tcp().await;
}

async fn start_supervisor() -> Result<(CancellationToken, tokio::task::JoinHandle<()>)> {
    let store = RuntimeStore::open_default()?;
    let recovered = store.recover_after_restart()?;
    if !recovered.is_empty() {
        tracing::warn!(
            count = recovered.len(),
            "gateway reconciled unfinished tasks at startup"
        );
    }
    let shutdown = CancellationToken::new();
    let supervisor = RuntimeSupervisor::new(store);
    let task = tokio::spawn(supervisor.run(shutdown.clone()));
    Ok((shutdown, task))
}

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    let configured = config::config_dir().join("gateway.sock");
    if configured.to_string_lossy().len() < 100 {
        return configured;
    }

    // macOS and BSD Unix-domain sockets have a short sun_path limit. Keep a
    // fallback in /tmp for unusually long home/config paths; the socket itself
    // is still owner-only and the filename is unique to the current uid.
    let uid = unsafe { libc::geteuid() };
    std::path::PathBuf::from(format!("/tmp/luma-gateway-{uid}.sock"))
}

#[cfg(unix)]
pub async fn is_running() -> bool {
    use tokio::{io::AsyncReadExt, net::UnixStream};

    let Ok(mut stream) = UnixStream::connect(socket_path()).await else {
        return false;
    };
    let mut response = vec![0; HANDSHAKE.len()];
    tokio::time::timeout(Duration::from_secs(1), stream.read_exact(&mut response))
        .await
        .is_ok_and(|result| result.is_ok())
        && response == HANDSHAKE
}

#[cfg(unix)]
async fn send_stop_request() -> Result<()> {
    use tokio::net::UnixStream;
    let mut stream = UnixStream::connect(socket_path())
        .await
        .context("connecting to the Luma gateway control socket")?;
    stream.write_all(b"LUMA_GATEWAY_STOP\n").await?;
    stream.flush().await?;
    let mut handshake = vec![0; HANDSHAKE.len()];
    tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut handshake))
        .await
        .context("timed out waiting for gateway control handshake")??;
    if handshake != HANDSHAKE {
        bail!("invalid response from Luma gateway control socket");
    }
    let mut response = [0; 9];
    tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut response))
        .await
        .context("timed out waiting for gateway shutdown acknowledgement")??;
    if &response != b"STOPPING\n" {
        bail!("gateway did not acknowledge shutdown");
    }
    Ok(())
}

#[cfg(unix)]
async fn run_unix() -> Result<()> {
    use std::{
        fs,
        os::{fd::AsRawFd, unix::fs::PermissionsExt},
    };
    use tokio::{
        net::UnixListener,
        signal::unix::{SignalKind, signal},
    };

    let directory = config::config_dir();
    fs::create_dir_all(&directory)?;
    let lock_path = directory.join("gateway.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600))?;
    // The advisory lock is released by the OS if the gateway crashes. The
    // lock file itself can safely remain between launches.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("Luma gateway is already running");
    }

    let socket = socket_path();
    if socket.exists() {
        fs::remove_file(&socket).context("removing stale gateway socket")?;
    }
    let (shutdown, supervisor) = start_supervisor().await?;
    let listener = UnixListener::bind(&socket).context("binding the local gateway socket")?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    tracing::info!(socket = %socket.display(), "Luma gateway ready");
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = terminate.recv() => break,
            _ = interrupt.recv() => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((mut stream, _)) => {
                        let shutdown = shutdown.clone();
                        tokio::spawn(async move {
                            if stream.write_all(HANDSHAKE).await.is_err() { return; }
                            let mut command = [0; 64];
                            let read = tokio::time::timeout(Duration::from_millis(250), stream.read(&mut command)).await;
                            if matches!(read, Ok(Ok(length)) if &command[..length] == b"LUMA_GATEWAY_STOP\n") {
                                let _ = stream.write_all(b"STOPPING\n").await;
                                shutdown.cancel();
                            }
                        });
                    }
                    Err(error) => tracing::warn!(%error, "gateway health-check connection failed"),
                }
            }
        }
    }

    shutdown.cancel();
    if let Err(error) = supervisor.await {
        tracing::warn!(%error, "gateway supervisor task did not shut down cleanly");
    }
    drop(listener);
    let _ = fs::remove_file(socket);
    // Keep the lock handle alive until after the socket is removed.
    drop(lock);
    Ok(())
}

#[cfg(not(unix))]
const GATEWAY_PORT: u16 = 39_271;

#[cfg(not(unix))]
fn tcp_control_token() -> Result<String> {
    use std::io::Write;
    let directory = config::config_dir();
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("gateway.token");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            let token = uuid::Uuid::new_v4().to_string();
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            Ok(token)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let token = std::fs::read_to_string(path)?.trim().to_owned();
            if token.len() != 36
                || !token
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
            {
                bail!(
                    "invalid Luma gateway control token; remove gateway.token and restart the gateway"
                )
            }
            Ok(token)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(unix))]
pub async fn is_running() -> bool {
    use tokio::{io::AsyncReadExt, net::TcpStream};
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", GATEWAY_PORT)).await else {
        return false;
    };
    let mut response = vec![0; HANDSHAKE.len()];
    tokio::time::timeout(Duration::from_secs(1), stream.read_exact(&mut response))
        .await
        .is_ok_and(|result| result.is_ok())
        && response == HANDSHAKE
}

#[cfg(not(unix))]
async fn send_stop_request() -> Result<()> {
    use tokio::net::TcpStream;
    let token = tcp_control_token()?;
    let mut stream = TcpStream::connect(("127.0.0.1", GATEWAY_PORT))
        .await
        .context("connecting to the Luma gateway control port")?;
    stream
        .write_all(format!("LUMA_GATEWAY_STOP {token}\n").as_bytes())
        .await?;
    stream.flush().await?;
    let mut handshake = vec![0; HANDSHAKE.len()];
    tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut handshake))
        .await
        .context("timed out waiting for gateway control handshake")??;
    if handshake != HANDSHAKE {
        bail!("invalid response from Luma gateway control port");
    }
    let mut response = [0; 9];
    tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut response))
        .await
        .context("timed out waiting for gateway shutdown acknowledgement")??;
    if &response != b"STOPPING\n" {
        bail!("gateway did not acknowledge shutdown");
    }
    Ok(())
}

#[cfg(not(unix))]
async fn run_tcp() -> Result<()> {
    use tokio::{net::TcpListener, signal};
    let token = tcp_control_token()?;
    let listener = TcpListener::bind(("127.0.0.1", GATEWAY_PORT))
        .await
        .context("binding the local gateway port")?;
    // Claim the singleton endpoint before starting the supervisor. Otherwise
    // two simultaneous launches can both begin consuming queued tasks before
    // one of them loses the bind race.
    let (shutdown, supervisor) = start_supervisor().await?;
    tracing::info!(port = GATEWAY_PORT, "Luma gateway ready");
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = signal::ctrl_c() => break,
            accepted = listener.accept() => {
                if let Ok((mut stream, _)) = accepted {
                    let shutdown = shutdown.clone();
                    let token = token.clone();
                    tokio::spawn(async move {
                        if stream.write_all(HANDSHAKE).await.is_err() { return; }
                        let expected = format!("LUMA_GATEWAY_STOP {token}\n");
                        let mut command = vec![0; expected.len()];
                        let read = tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut command)).await;
                        if matches!(read, Ok(Ok(_)) if command == expected.as_bytes()) {
                            let _ = stream.write_all(b"STOPPING\n").await;
                            shutdown.cancel();
                        }
                    });
                }
            }
        }
    }
    shutdown.cancel();
    let _ = supervisor.await;
    Ok(())
}
