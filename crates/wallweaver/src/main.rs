//! The `wallweaver` command-line tool.

use std::fs;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use ww_package::{PublicKey, SecretKey, VerifyOptions};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "wallweaver - make, check and share Wallweaver wallpapers

Usage: wallweaver <command> [options]

Making wallpapers:
  validate <folder|wallpaper.json>     check a manifest and report every problem
  pack <folder> [-o out.wwp]           build a package from a folder
  keygen [-o name]                     create a signing key pair (name.wwkey, name.wwpub)
  sign <in.wwp> --key <k.wwkey> [-o out.wwp]
                                       sign a package (replaces any earlier signature)
  verify <pkg.wwp> [--trust k.wwpub]... [--allow-unsigned]
                                       check signature, file list and every file hash
  schema                               print the wallpaper.json JSON Schema

Measuring:
  bench [paths...] [--seconds N] [--paused-seconds N] [--size WxH] [--fps N]
        [--out DIR] [--daemon PATH | --no-daemon] [--software]
                                       measure frames, CPU, memory and GPU time
                                       (built-in test wallpapers if no paths are given)

Controlling the running daemon:
  status                               what is showing where
  set <file|folder> [--monitor N]      show a wallpaper (all monitors by default)
  clear [--monitor N]                  remove the wallpaper
  pause | resume | stop                pause all, resume, or stop and free GPU memory
  quit                                 stop the daemon

  --help, --version
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        None | Some("--help" | "-h" | "help") => {
            print!("{HELP}");
            Ok(())
        }
        Some("--version" | "-V") => {
            println!("wallweaver {VERSION}");
            Ok(())
        }
        Some(cmd) => run(cmd, &args[1..]),
    };
    if let Err(e) = result {
        eprintln!("wallweaver: {e}");
        std::process::exit(1);
    }
}

/// Parsed arguments: positionals plus `--name value` / `--flag` options.
struct Args {
    positional: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse(raw: &[String], flags: &[&str], valued: &[&str]) -> Result<Args, String> {
        let mut a = Args {
            positional: Vec::new(),
            options: Vec::new(),
        };
        let mut it = raw.iter();
        while let Some(arg) = it.next() {
            if flags.contains(&arg.as_str()) {
                a.options.push((arg.clone(), None));
            } else if valued.contains(&arg.as_str()) {
                let v = it.next().ok_or_else(|| format!("{arg} needs a value"))?;
                a.options.push((arg.clone(), Some(v.clone())));
            } else if arg.starts_with('-') && arg.len() > 1 {
                return Err(format!("unknown option '{arg}' (see --help)"));
            } else {
                a.positional.push(arg.clone());
            }
        }
        Ok(a)
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.options
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.as_deref())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.options
            .iter()
            .filter(|(k, _)| k == name)
            .filter_map(|(_, v)| v.as_deref())
            .collect()
    }

    fn has(&self, name: &str) -> bool {
        self.options.iter().any(|(k, _)| k == name)
    }

    fn one(&self, what: &str) -> Result<&str, String> {
        match self.positional.as_slice() {
            [one] => Ok(one),
            [] => Err(format!("missing {what}")),
            _ => Err("too many arguments".into()),
        }
    }
}

fn run(cmd: &str, raw: &[String]) -> Result<(), String> {
    match cmd {
        "validate" => validate(&Args::parse(raw, &[], &[])?),
        "pack" => pack(&Args::parse(raw, &[], &["-o", "--out"])?),
        "keygen" => keygen(&Args::parse(raw, &[], &["-o", "--out"])?),
        "sign" => sign(&Args::parse(raw, &[], &["--key", "-o", "--out"])?),
        "verify" => verify(&Args::parse(raw, &["--allow-unsigned"], &["--trust"])?),
        "schema" => {
            print!("{}", ww_manifest::json_schema_string());
            Ok(())
        }
        "bench" => bench(&Args::parse(
            raw,
            &["--no-daemon", "--software"],
            &[
                "--seconds",
                "--paused-seconds",
                "--size",
                "--fps",
                "--out",
                "--daemon",
            ],
        )?),
        "status" | "set" | "clear" | "pause" | "resume" | "stop" | "quit" => {
            control(cmd, &Args::parse(raw, &[], &["--monitor", "--socket"])?)
        }
        other => Err(format!("unknown command '{other}' (see --help)")),
    }
}

fn read_manifest_at(path: &Path) -> Result<ww_manifest::Manifest, String> {
    let file = if path.is_dir() {
        path.join(ww_manifest::MANIFEST_FILE)
    } else {
        path.to_path_buf()
    };
    let text = fs::read_to_string(&file).map_err(|e| format!("reading {}: {e}", file.display()))?;
    ww_manifest::Manifest::from_json(&text).map_err(|e| e.to_string())
}

fn validate(a: &Args) -> Result<(), String> {
    let path = PathBuf::from(a.one("a folder or wallpaper.json")?);
    let m = read_manifest_at(&path)?;
    println!("ok: {} {} ({}, {})", m.id, m.version, m.kind, m.license);
    if path.is_dir() {
        for (field, rel) in m.referenced_files() {
            let mut p = path.clone();
            p.extend(rel.split('/'));
            if !p.is_file() {
                return Err(format!("{field} file '{rel}' does not exist"));
            }
        }
    }
    let caps = ww_permissions_summary(&m);
    if caps.is_empty() {
        println!("permissions: none");
    } else {
        println!("permissions: {}", caps.join(", "));
    }
    Ok(())
}

fn ww_permissions_summary(m: &ww_manifest::Manifest) -> Vec<String> {
    let p = &m.permissions;
    let mut out = Vec::new();
    if p.audio {
        out.push("audio".to_string());
    }
    for s in &p.system_stats {
        out.push(format!("system.stats:{}", s.as_str()));
    }
    if p.weather {
        out.push("weather".into());
    }
    if p.calendar {
        out.push("calendar".into());
    }
    for o in p.network_origins() {
        out.push(format!("network:{o}"));
    }
    if p.input_mouse {
        out.push("input.mouse".into());
    }
    if p.script_eval {
        out.push("script.eval".into());
    }
    out
}

fn create_new(path: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("creating {}: {e}", path.display()))
}

fn pack(a: &Args) -> Result<(), String> {
    let src = PathBuf::from(a.one("a folder")?);
    let m = read_manifest_at(&src)?;
    let out = a
        .get("-o")
        .or(a.get("--out"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{}-{}.wwp", m.id, m.version)));
    let file = create_new(&out)?;
    let report = match ww_package::pack_dir(&src, BufWriter::new(file)) {
        Ok(r) => r,
        Err(e) => {
            let _ = fs::remove_file(&out);
            return Err(e.to_string());
        }
    };
    println!(
        "packed {} files ({} bytes) into {} (unsigned; sign it with `wallweaver sign`)",
        report.checksums.entries().len(),
        report.checksums.total_size(),
        out.display()
    );
    Ok(())
}

fn keygen(a: &Args) -> Result<(), String> {
    if !a.positional.is_empty() {
        return Err("keygen takes no positional arguments; use -o <name>".into());
    }
    let name = a.get("-o").or(a.get("--out")).unwrap_or("wallweaver");
    let secret_path = PathBuf::from(format!("{name}.wwkey"));
    let public_path = PathBuf::from(format!("{name}.wwpub"));
    let key = SecretKey::generate().map_err(|e| e.to_string())?;
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(&secret_path)
        .map_err(|e| format!("creating {}: {e}", secret_path.display()))?;
    f.write_all(key.to_file_string().as_bytes())
        .map_err(|e| e.to_string())?;
    let mut p = create_new(&public_path)?;
    p.write_all(key.public_key().to_file_string().as_bytes())
        .map_err(|e| e.to_string())?;
    println!(
        "key {}\n  secret: {} (keep it private, never put it in a package or repo)\n  public: {}",
        key.public_key().key_id(),
        secret_path.display(),
        public_path.display()
    );
    Ok(())
}

fn sign(a: &Args) -> Result<(), String> {
    let input = PathBuf::from(a.one("a package")?);
    let key_path = a.get("--key").ok_or("--key <file.wwkey> is required")?;
    let key_text = fs::read_to_string(key_path).map_err(|e| format!("reading {key_path}: {e}"))?;
    let key = SecretKey::from_file_string(&key_text).map_err(|e| e.to_string())?;
    let out = match a.get("-o").or(a.get("--out")) {
        Some(o) => PathBuf::from(o),
        None => {
            let stem = input
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            input.with_file_name(format!("{stem}.signed.wwp"))
        }
    };
    let reader = BufReader::new(
        fs::File::open(&input).map_err(|e| format!("opening {}: {e}", input.display()))?,
    );
    let file = create_new(&out)?;
    match ww_package::sign_package(reader, BufWriter::new(file), &key) {
        Ok(pkg) => {
            println!(
                "signed {} {} with key {} -> {}",
                pkg.manifest.id,
                pkg.manifest.version,
                key.public_key().key_id(),
                out.display()
            );
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&out);
            Err(e.to_string())
        }
    }
}

fn verify(a: &Args) -> Result<(), String> {
    let input = PathBuf::from(a.one("a package")?);
    let mut opts = if a.has("--allow-unsigned") {
        VerifyOptions::developer()
    } else {
        VerifyOptions::signed()
    };
    for t in a.all("--trust") {
        let text = fs::read_to_string(t).map_err(|e| format!("reading {t}: {e}"))?;
        opts.trusted_keys
            .push(PublicKey::from_file_string(&text).map_err(|e| e.to_string())?);
    }
    let reader = BufReader::new(
        fs::File::open(&input).map_err(|e| format!("opening {}: {e}", input.display()))?,
    );
    let pkg = ww_package::verify(reader, &opts).map_err(|e| e.to_string())?;
    println!(
        "ok: {} {} ({}), {} files, {} bytes, {}",
        pkg.manifest.id,
        pkg.manifest.version,
        pkg.manifest.kind,
        pkg.checksums.entries().len(),
        pkg.checksums.total_size(),
        match &pkg.signer {
            Some(s) => format!("signed by {}", s.key_id),
            None => "UNSIGNED (developer mode only)".into(),
        }
    );
    Ok(())
}

fn parse_size(s: &str) -> Result<(u32, u32), String> {
    s.split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .filter(|(w, h): &(u32, u32)| *w > 0 && *h > 0)
        .ok_or_else(|| format!("bad size '{s}', use WIDTHxHEIGHT"))
}

fn bench(a: &Args) -> Result<(), String> {
    let num = |name: &str, default: f64| -> Result<f64, String> {
        a.get(name)
            .map(|v| {
                v.parse::<f64>()
                    .map_err(|_| format!("{name} needs a number"))
            })
            .unwrap_or(Ok(default))
    };
    let (width, height) = parse_size(a.get("--size").unwrap_or("1920x1080"))?;
    let tmp = std::env::temp_dir().join(format!("wallweaver-bench-{}", std::process::id()));
    let cases = if a.positional.is_empty() {
        ww_bench::builtin_cases(&tmp)?
    } else {
        a.positional
            .iter()
            .map(|p| ww_bench::Case {
                name: Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.clone()),
                path: PathBuf::from(p),
            })
            .collect()
    };
    let daemon = if a.has("--no-daemon") {
        None
    } else if let Some(d) = a.get("--daemon") {
        Some(PathBuf::from(d))
    } else {
        std::env::current_exe()
            .ok()
            .map(|me| {
                me.with_file_name(if cfg!(windows) {
                    "ww-daemon.exe"
                } else {
                    "ww-daemon"
                })
            })
            .filter(|p| p.exists())
    };
    let config = ww_bench::BenchConfig {
        seconds: num("--seconds", 10.0)?.max(1.0),
        paused_seconds: num("--paused-seconds", 3.0)?.max(1.0),
        width,
        height,
        fps_cap: num("--fps", 60.0)?.clamp(1.0, 240.0) as u32,
        cases,
        gpu: ww_render::GpuOptions {
            allow_software: a.has("--software"),
            timestamps: true,
            ..ww_render::GpuOptions::default()
        },
        daemon,
    };
    let report = ww_bench::run(&config, |p| eprintln!("  {p}"));
    let _ = fs::remove_dir_all(&tmp);
    let report = report?;
    let md = ww_bench::to_markdown(&report);
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    let out = PathBuf::from(a.get("--out").unwrap_or("bench-results"));
    fs::create_dir_all(&out).map_err(|e| format!("creating {}: {e}", out.display()))?;
    fs::write(out.join("bench.json"), json + "\n").map_err(|e| e.to_string())?;
    fs::write(out.join("bench.md"), &md).map_err(|e| e.to_string())?;
    println!("{md}");
    println!(
        "wrote {} and {}",
        out.join("bench.json").display(),
        out.join("bench.md").display()
    );
    Ok(())
}

fn control(cmd: &str, a: &Args) -> Result<(), String> {
    use ww_ipc::{MonitorTarget, Request, Response, WallpaperSource};
    let endpoint = a
        .get("--socket")
        .map(|p| ww_ipc::Endpoint::Path(PathBuf::from(p)))
        .unwrap_or_else(ww_ipc::Endpoint::default_for_user);
    let monitor = match a.get("--monitor") {
        Some(m) => MonitorTarget::Id(m.parse().map_err(|_| "--monitor needs a number")?),
        None => MonitorTarget::All,
    };
    let request = match cmd {
        "status" => Request::Status,
        "set" => {
            let p = fs::canonicalize(a.one("a file or folder")?).map_err(|e| e.to_string())?;
            let path = p.to_string_lossy().into_owned();
            let source = if p.is_dir() {
                WallpaperSource::Folder { path }
            } else {
                WallpaperSource::File { path }
            };
            Request::SetWallpaper { monitor, source }
        }
        "clear" => Request::ClearWallpaper { monitor },
        "pause" => Request::PauseAll,
        "resume" => Request::ResumeAll,
        "stop" => Request::StopAll,
        _ => Request::Shutdown,
    };
    let mut client = ww_ipc::Client::connect(&endpoint, &format!("wallweaver-cli {VERSION}"))
        .map_err(|e| format!("{e} (is ww-daemon running?)"))?;
    match client.request(request).map_err(|e| e.to_string())? {
        Response::Status(s) => {
            println!(
                "ww-daemon {} (protocol {}), up {:.0} s, memory {}, adapter {}",
                s.version,
                s.protocol,
                s.uptime_ms as f64 / 1000.0,
                s.rss_bytes
                    .map(|b| format!("{:.1} MiB", b as f64 / 1048576.0))
                    .unwrap_or_else(|| "unknown".into()),
                s.adapter.unwrap_or_else(|| "none open".into())
            );
            for m in s.monitors {
                let what = m
                    .wallpaper
                    .map(|w| format!("{} ({})", w.name, w.kind))
                    .unwrap_or_else(|| "no wallpaper".into());
                let state = m
                    .state
                    .map(|st| serde_json::to_string(&st).unwrap_or_default())
                    .unwrap_or_default();
                println!(
                    "  monitor {} {} {}x{}: {what} {state}",
                    m.monitor.id, m.monitor.name, m.monitor.width, m.monitor.height
                );
            }
        }
        _ => println!("ok"),
    }
    Ok(())
}
