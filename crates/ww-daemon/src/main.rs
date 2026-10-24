//! `ww-daemon`: the small process that keeps wallpapers on the desktop.
//!
//! It owns the desktop host, the power governor, the native renderer and
//! the tray icon, and listens for the manager UI on a per-user local
//! socket. The manager UI is a separate process that only runs while its
//! window is open.

mod daemon;
mod server;
mod store;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use daemon::{Daemon, Options};
use ww_render::GpuOptions;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Log a line to stderr with the time since start.
pub(crate) fn log(msg: &str) {
    let t = START.get_or_init(Instant::now).elapsed();
    eprintln!("[{:>9.3}] {msg}", t.as_secs_f64());
}

const HELP: &str = "ww-daemon - keeps Wallweaver wallpapers on the desktop

Usage: ww-daemon [options]

Options:
  --set <path>          show this image, GIF, shader or package folder on every monitor
  --headless[=WxH]      render offscreen (default 1920x1080) instead of on the desktop
  --data-dir <path>     settings and installed packages (default: per-user data folder)
  --socket <path>       listen on this socket path instead of the per-user default
  --exit-after <secs>   quit after this many seconds (for benchmarks)
  --software            allow a software graphics adapter
  --version             print the version
  --help                print this help
";

fn parse() -> Result<Options, String> {
    let mut opts = Options {
        headless: None,
        data_dir: store::default_data_dir(),
        endpoint: None,
        set: None,
        exit_after: None,
        gpu: GpuOptions::default(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("ww-daemon {VERSION}");
                std::process::exit(0);
            }
            "--set" => opts.set = Some(PathBuf::from(value("--set")?)),
            "--data-dir" => opts.data_dir = PathBuf::from(value("--data-dir")?),
            "--socket" => {
                opts.endpoint = Some(ww_ipc::Endpoint::Path(PathBuf::from(value("--socket")?)))
            }
            "--exit-after" => {
                let secs: f64 = value("--exit-after")?
                    .parse()
                    .map_err(|_| "--exit-after needs a number of seconds".to_string())?;
                opts.exit_after = Some(Duration::from_secs_f64(secs.max(0.0)));
            }
            "--software" => opts.gpu.allow_software = true,
            "--headless" => opts.headless = Some((1920, 1080)),
            other if other.starts_with("--headless=") => {
                let spec = &other["--headless=".len()..];
                let (w, h) = spec
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .ok_or_else(|| format!("bad size '{spec}', use WIDTHxHEIGHT"))?;
                opts.headless = Some((w, h));
            }
            other => return Err(format!("unknown option '{other}' (see --help)")),
        }
    }
    Ok(opts)
}

fn main() {
    START.get_or_init(Instant::now);
    let opts = match parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ww-daemon: {e}");
            std::process::exit(2);
        }
    };
    let host: Box<dyn ww_desktop::DesktopHost> = match opts.headless {
        Some((w, h)) => Box::new(ww_desktop::NullHost::new(w, h)),
        None => match ww_desktop::platform_host() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("ww-daemon: {e}");
                std::process::exit(1);
            }
        },
    };
    let endpoint = opts
        .endpoint
        .clone()
        .unwrap_or_else(ww_ipc::Endpoint::default_for_user);
    let server = match ww_ipc::Server::bind(&endpoint) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "ww-daemon: could not listen on {endpoint}: {e} (is another daemon running?)"
            );
            std::process::exit(1);
        }
    };
    let mut daemon = match Daemon::new(opts, host) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("ww-daemon: {e}");
            std::process::exit(1);
        }
    };
    let (tx, waker) = daemon.ipc_handles();
    server::spawn(server, tx, waker, VERSION);
    log(&format!("ww-daemon {VERSION} listening on {endpoint}"));
    daemon.run();
    log("stopped");
}
