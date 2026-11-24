use std::path::PathBuf;
use std::process::ExitCode;

use funny::error::SourceFile;
use funny::output;

const USAGE: &str = "funny: compiler FunnyBoxes

Pemakaian:
  funny init <nama>                       buat proyek baru (src/main.fny, images/)
  funny <file.fny | folder> [-o <target>] [--debug] [--core <file.wasm>]
  funny build <file.fny | folder> ...     sama seperti di atas
  funny check <file.fny | folder>         periksa sintaks dan semantik
  funny ast <file.fny>                    tampilkan AST (debug)
  funny --version

Keluaran: SATU file HTML (CSS, JS, dan WASM tertanam di dalamnya).
Default: <nama>.html di folder aktif, misalnya `funny y.fny` menghasilkan y.html.
-o bisa berupa nama file; kalau tanpa .html, ekstensi .html ditambahkan.";

/// Nama output default: `<nama>.html`. Untuk proyek (folder atau file di src), namanya
/// mengikuti folder proyek. Untuk file tunggal, namanya mengikuti nama file.
fn default_output(input: &PathBuf) -> PathBuf {
    let name = if input.is_dir() {
        input.file_name().and_then(|s| s.to_str()).map(String::from)
    } else {
        let parent = input.parent();
        let in_src = parent.and_then(|p| p.file_name()).and_then(|n| n.to_str()) == Some("src");
        if in_src {
            parent
                .and_then(|p| p.parent())
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(String::from)
        } else {
            input.file_stem().and_then(|s| s.to_str()).map(String::from)
        }
    };
    PathBuf::from(format!("{}.html", name.unwrap_or_else(|| "app".to_string())))
}

struct Cli {
    cmd: String,
    input: PathBuf,
    out: Option<PathBuf>,
    debug: bool,
    core: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut pos: Vec<String> = Vec::new();
    let mut out = None;
    let mut debug = false;
    let mut core = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                let v = args.get(i).ok_or("-o butuh path")?;
                out = Some(PathBuf::from(v));
            }
            "--core" => {
                i += 1;
                let v = args.get(i).ok_or("--core butuh path ke core.wasm")?;
                core = Some(PathBuf::from(v));
            }
            "--debug" => debug = true,
            other => pos.push(other.to_string()),
        }
        i += 1;
    }

    match pos.first().map(|s| s.as_str()) {
        Some("init") => {
            let name = pos.get(1).ok_or("init butuh nama proyek")?;
            Ok(Cli { cmd: "init".into(), input: PathBuf::from(name), out: None, debug, core })
        }
        Some(c @ ("build" | "check" | "ast")) => {
            let input = pos.get(1).ok_or(format!("'{}' butuh file atau folder", c))?;
            Ok(Cli { cmd: c.to_string(), input: PathBuf::from(input), out, debug, core })
        }
        Some(first) => Ok(Cli { cmd: "build".into(), input: PathBuf::from(first), out, debug, core }),
        None => Err(String::new()),
    }
}

fn report(msgs: &[String]) -> ExitCode {
    for m in msgs {
        eprintln!("{}\n", m);
    }
    eprintln!("Gagal: {} error.", msgs.len());
    ExitCode::from(1)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if matches!(args.first().map(|s| s.as_str()), Some("--version") | Some("-V")) {
        println!("funny {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let cli = match parse_args(&args) {
        Ok(c) => c,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("funny: {}\n", msg);
            }
            eprintln!("{}", USAGE);
            return ExitCode::from(1);
        }
    };

    match cli.cmd.as_str() {
        "init" => match output::init_project(&cli.input) {
            Ok(()) => {
                println!("Proyek '{}' dibuat. Jalankan: funny {}", cli.input.display(), cli.input.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("funny: {}", e);
                ExitCode::from(1)
            }
        },
        "ast" => {
            let src = match std::fs::read_to_string(&cli.input) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("funny: tidak bisa membaca '{}': {}", cli.input.display(), e);
                    return ExitCode::from(1);
                }
            };
            let file = SourceFile::new(cli.input.display().to_string(), src);
            match funny::parse_file(&file) {
                Ok(program) => {
                    println!("{:#?}", program);
                    ExitCode::SUCCESS
                }
                Err(msg) => report(&[msg]),
            }
        }
        "check" => match output::build_project(&cli.input, cli.debug, cli.core.as_deref()) {
            Ok(_) => {
                println!("OK: tidak ada error.");
                ExitCode::SUCCESS
            }
            Err(msgs) => report(&msgs),
        },
        _ => match output::build_project(&cli.input, cli.debug, cli.core.as_deref()) {
            Ok(out) => {
                let target = match &cli.out {
                    Some(p) if p.extension().map_or(false, |e| e == "html") => p.clone(),
                    Some(p) => p.with_extension("html"),
                    None => default_output(&cli.input),
                };
                match output::write(&out, &target) {
                    Ok(file) => {
                        let mut extra = String::new();
                        if out.core_wasm {
                            extra.push_str(", core.wasm tertanam");
                        }
                        if !out.wasm_funcs.is_empty() {
                            extra.push_str(&format!(", {} fungsi WASM tertanam", out.wasm_funcs.len()));
                        }
                        println!("Berhasil: {}{}", file.display(), extra);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("funny: gagal menulis '{}': {}", target.display(), e);
                        ExitCode::from(1)
                    }
                }
            }
            Err(msgs) => report(&msgs),
        },
    }
}
