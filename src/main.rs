use clap::{CommandFactory, Parser};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;
use termion::screen::IntoAlternateScreen;

mod app;
mod htmlparser;
mod row;
mod screen;

use app::App;

#[derive(Parser, Debug)]
#[command(
    name = "hless",
    version,
    about = "A pager for HTML files with nested folding",
    after_help = "Ejemplos:\n  hless page.html\n  cat page.html | hless\n  curl https://example.com | hless"
)]
struct Opt {
    /// Input file. Reads from stdin if not provided or '-' is specified.
    input: Option<PathBuf>,
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

    let (input_string, input_filename) = match get_input_and_filename(&opt) {
        Ok(val) => val,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    };

    // Si stdout no es un terminal, no podemos abrir TUI: se vuelca el HTML tal cual.
    if !isatty::stdout_isatty() {
        print!("{input_string}");
        std::process::exit(0);
    }

    let mut app = match App::new(&input_string, &input_filename) {
        Ok(app) => app,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    };

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
                print!("{input_string}");
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
