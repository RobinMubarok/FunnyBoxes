use std::path::PathBuf;
use std::process::ExitCode;

use funny::error::SourceFile;
use funny::output;

const USAGE: &str = "funny: compiler FunnyBoxes (.fny)

Pemakaian:
  funny <file.fny> [-o <dir>] [--debug]     build (sama dengan 'funny build')
  funny build <file.fny> [-o <dir>] [--debug]
  funny check <file.fny> [--debug]          periksa sintaks dan semantik
  funny ast <file.fny>                      tampilkan AST (debug)
  funny --version

Opsi:
  --core <file.wasm>   lokasi core.wasm (default: env FUNNY_CORE_WASM)

File entry memakai semua file .fny di folder yang sama.
Default folder output: dist/
Keluaran: index.html, style.css, app.js (core.wasm belum tersedia).";

struct Cli {
    cmd: String,
    file: PathBuf,
    out: PathBuf,
    debug: bool,
    core: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut out = PathBuf::from("dist");
    let mut debug = false;
    let mut core: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                let dir = args.get(i).ok_or_else(|| "-o butuh nama folder".to_string())?;
                out = PathBuf::from(dir.clone());
            }
            "--debug" => debug = true,
            "--core" => {
                i += 1;
                let f = args.get(i).ok_or_else(|| "--core butuh path ke core.wasm".to_string())?;
                core = Some(PathBuf::from(f.clone()));
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    let (cmd, file) = match positional.as_slice() {
        [c, f] if matches!(c.as_str(), "build" | "check" | "ast") => (c.clone(), f.clone()),
        [f] if f.ends_with(".fny") => ("build".to_string(), f.clone()),
        _ => return Err(String::new()),
    };
    Ok(Cli {
        cmd,
        file: PathBuf::from(file),
        out,
        debug,
        core,
    })
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
        "ast" => {
            let src = match std::fs::read_to_string(&cli.file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("funny: tidak bisa membaca '{}': {}", cli.file.display(), e);
                    return ExitCode::from(1);
                }
            };
            let file = SourceFile::new(cli.file.display().to_string(), src);
            match funny::parse_file(&file) {
                Ok(program) => {
                    println!("{:#?}", program);
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprintln!("{}\n\nGagal.", msg);
                    ExitCode::from(1)
                }
            }
        }
        "check" => match output::build_project(&cli.file, cli.debug, cli.core.as_deref()) {
            Ok(_) => {
                println!("OK: tidak ada error.");
                ExitCode::SUCCESS
            }
            Err(msgs) => report_errors(&msgs),
        },
        _ => match output::build_project(&cli.file, cli.debug, cli.core.as_deref()) {
            Ok(out) => match output::write(&out, &cli.out) {
                Ok(()) => {
                    let mut extra = String::new();
                    if out.core_wasm.is_some() {
                        extra.push_str(", core.wasm");
                    }
                    if !out.wasm_funcs.is_empty() {
                        extra.push_str(&format!(", {} fungsi WASM", out.wasm_funcs.len()));
                    }
                    println!("Berhasil: {}/ (index.html, style.css, app.js{})", cli.out.display(), extra);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("funny: gagal menulis ke '{}': {}", cli.out.display(), e);
                    ExitCode::from(1)
                }
            },
            Err(msgs) => report_errors(&msgs),
        },
    }
}

fn report_errors(msgs: &[String]) -> ExitCode {
    for m in msgs {
        eprintln!("{}\n", m);
    }
    eprintln!("Gagal: {} error ditemukan.", msgs.len());
    ExitCode::from(1)
}
