use clap::{CommandFactory, Parser};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;
use termion::screen::IntoAlternateScreen;

mod app;
mod htmlparser;
mod row;
mod screen;
mod sniff;

use app::App;

#[derive(Parser, Debug)]
#[command(
    name = "hless",
    version,
    about = "A pager for HTML files with nested folding",
    after_help = "Ejemplos:\n  hless page.html\n  cat page.html | hless\n  curl https://example.com | hless\n  curl -i https://example.com | hless   (Ctrl+O: cabeceras HTTP)"
)]
struct Opt {
    /// Input file. Reads from stdin if not provided or '-' is specified.
    input: Option<PathBuf>,

    /// Muestra la entrada aunque no parezca HTML.
    #[arg(short, long)]
    force: bool,
}

fn main() {
    let opt = Opt::parse();

    // Sin fichero y con la entrada en un terminal: no hay nada que leer, así que
    // se muestra la ayuda (si stdin viene por pipe, se lee de él).
    if opt.input.is_none() && isatty::stdin_isatty() {
        let _ = Opt::command().print_help();
        println!();
        std::process::exit(0);
    }

    // Aviso si el fichero tiene una extensión que no es HTML (no bloquea).
    if let Some(path) = &opt.input {
        if path.as_os_str() != "-" {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ok = matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "html" | "htm" | "xhtml" | "svg"
                );
                if !ok {
                    eprintln!(
                        "hless: aviso: '{}' no tiene extensión HTML; se mostrará igualmente",
                        path.display()
                    );
                }
            }
        }
    }

    let (raw, filename) = match get_input_and_filename(&opt) {
        Ok(val) => val,
        Err(err) => {
            if err.kind() == io::ErrorKind::InvalidData {
                eprintln!(
                    "hless: la entrada no es texto UTF-8 (¿contenido comprimido o binario?)"
                );
            } else {
                eprintln!("hless: {err}");
            }
            std::process::exit(1);
        }
    };

    // Si stdout no es un terminal, no podemos abrir TUI: se vuelca tal cual.
    if !isatty::stdout_isatty() {
        print!("{raw}");
        std::process::exit(0);
    }

    // Clasificar la entrada.
    let kind = sniff::sniff(&raw);
    let mut headers = None;
    let mut notice = None;
    let mut open_headers = false;
    let mut empty_http_body = false;

    let source: String = match kind {
        sniff::InputKind::Html => raw.clone(),
        sniff::InputKind::Http(resp) => {
            let sniff::HttpResponse { meta, body } = resp;
            empty_http_body = body.trim().is_empty();
            if empty_http_body {
                open_headers = true;
                notice = Some(
                    "Respuesta HTTP sin cuerpo (p. ej. HEAD). Pulsa Ctrl+O para ver las cabeceras."
                        .to_string(),
                );
            }
            headers = Some(meta);
            body
        }
        sniff::InputKind::Empty => {
            eprintln!("hless: la entrada está vacía");
            std::process::exit(2);
        }
        sniff::InputKind::Other => {
            if !opt.force {
                eprintln!("hless: la entrada no parece HTML (no empieza por '<').");
                eprintln!("        usa --force para mostrarla igualmente.");
                std::process::exit(2);
            }
            raw.clone()
        }
    };

    let is_http = headers.is_some();
    let node = match htmlparser::parse_html(&source) {
        Ok(node) => node,
        Err(err) => {
            eprintln!("hless: {err}");
            std::process::exit(1);
        }
    };

    // Comprobación estructural: debe haber al menos una etiqueta real.
    if !opt.force && !empty_http_body && !htmlparser::has_real_element(&node) {
        if is_http {
            eprintln!("hless: el cuerpo de la respuesta HTTP no parece HTML.");
        } else {
            eprintln!("hless: la entrada no parece HTML (no contiene etiquetas).");
        }
        eprintln!("        usa --force para mostrarla igualmente.");
        std::process::exit(2);
    }

    let mut app = App::from_html(node, &filename);
    if let Some(meta) = headers {
        app.set_headers(meta, open_headers);
    }
    if let Some(msg) = notice {
        app.set_notice(msg);
    }

    // Fuente del teclado: stdin si es un terminal; si la entrada vino por pipe
    // (p. ej. `curl ... | hless`), se lee del terminal controlando (/dev/tty),
    // porque stdin ya está consumido/agotado.
    let mut keys: Box<dyn Read> = if isatty::stdin_isatty() {
        Box::new(io::stdin())
    } else {
        match File::open("/dev/tty") {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(_) => {
                // Sin terminal para el teclado no hay TUI posible: se vuelca.
                print!("{raw}");
                std::process::exit(0);
            }
        }
    };

    let stdout = std::io::stdout();
    let raw_stdout = match termion::raw::IntoRawMode::into_raw_mode(stdout) {
        Ok(term) => term,
        Err(e) => {
            eprintln!("Error accessing terminal: {e}");
            std::process::exit(1);
        }
    };

    let mut stdout: termion::screen::AlternateScreen<termion::raw::RawTerminal<std::io::Stdout>> =
        raw_stdout.into_alternate_screen().unwrap();

    app.run(&mut *keys, &mut stdout);
}

fn get_input_and_filename(opt: &Opt) -> io::Result<(String, String)> {
    let mut input_string = String::new();
    let filename = match &opt.input {
        Some(path) if path.as_os_str() == "-" => {
            io::stdin().read_to_string(&mut input_string)?;
            "<stdin>".to_string()
        }
        Some(path) => {
            File::open(path)?.read_to_string(&mut input_string)?;
            path.to_string_lossy().to_string()
        }
        None => {
            io::stdin().read_to_string(&mut input_string)?;
            "<stdin>".to_string()
        }
    };
    Ok((input_string, filename))
}
