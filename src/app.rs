use std::io::{Read, Write};
use crate::htmlparser;
use crate::row::FlatHtml;
use crate::screen::{self, Colors};

pub struct App {
    pub flat_html: FlatHtml,
    pub filename: String,
    pub focus: usize,
    /// Fila (índice absoluto) que ocupa la primera línea de la ventana.
    /// Permite desplazar la vista sin arrastrar el foco al borde.
    scroll: usize,
    /// Texto que se está escribiendo tras `/` (None si no hay búsqueda activa).
    search_input: Option<String>,
    /// Consulta confirmada que sigue resaltada.
    query: String,
    /// Filas que coinciden con `query` (ordenadas).
    matches: Vec<usize>,
    /// Índice dentro de `matches` de la coincidencia actual.
    active_match: Option<usize>,
    /// Foco al empezar a buscar (para restaurarlo si se cancela).
    search_origin: usize,
    colors: Colors,
}

impl App {
    pub fn new(input: &str, filename: &str) -> Result<Self, String> {
        let html = htmlparser::parse_html(input)?;
        let mut flat_html = FlatHtml::from_html(&html);

        // Por defecto todo arranca plegado: se van descubriendo los niveles al
        // expandir (→). El foco queda en <html>.
        let focus = flat_html.collapse_all_default();

        Ok(App {
            flat_html,
            filename: filename.to_string(),
            focus,
            scroll: 0,
            search_input: None,
            query: String::new(),
            matches: Vec::new(),
            active_match: None,
            search_origin: focus,
            colors: Colors::new(),
        })
    }

    pub fn run(&mut self, keys: &mut dyn Read, stdout: &mut dyn Write) {
        loop {
            self.render(stdout);
            match read_key(keys) {
                Input::Eof => break,
                Input::Ignore => {}
                Input::Key(key) => {
                    if self.handle_key(key) {
                        break;
                    }
                }
            }
        }
    }

    /// Aplica una tecla. Devuelve `true` si hay que salir.
    fn handle_key(&mut self, key: Key) -> bool {
        // Modo entrada de búsqueda: los caracteres van al texto buscado.
        if self.search_input.is_some() {
            match key {
                Key::Escape => self.cancel_search(),
                Key::Enter => self.confirm_search(),
                Key::Backspace => {
                    if let Some(s) = self.search_input.as_mut() {
                        s.pop();
                    }
                    self.refresh_search();
                }
                Key::Char(c) => {
                    if let Some(s) = self.search_input.as_mut() {
                        s.push(c);
                    }
                    self.refresh_search();
                }
                _ => {}
            }
            return false;
        }

        match key {
            Key::Char('q') | Key::Char('Q') => return true,
            Key::Up => self.navigate(Direction::Up),
            Key::Down => self.navigate(Direction::Down),
            Key::Left => self.collapse(),
            Key::Right => self.expand(),
            Key::PageUp => self.page_up(),
            Key::PageDown => self.page_down(),
            Key::Home => self.go_home(),
            Key::End => self.go_end(),
            Key::Char('j') => self.navigate(Direction::Down),
            Key::Char('k') => self.navigate(Direction::Up),
            Key::Char('h') => self.collapse(),
            Key::Char('l') => self.expand(),
            Key::Char(' ') | Key::Char('f') => self.page_down(),
            Key::Char('b') | Key::Char('B') => self.page_up(),
            Key::Char('g') => self.go_home(),
            Key::Char('G') => self.go_end(),
            Key::Char('/') => self.start_search(),
            Key::Char('n') => self.search_next(),
            Key::Char('N') => self.search_prev(),
            _ => {}
        }
        false
    }

    fn render(&mut self, stdout: &mut dyn Write) {
        let size = termion::terminal_size().unwrap_or((80, 24));
        let width = (size.0 as usize).max(1);
        let height = (size.1 as usize).max(3);
        let content_height = height.saturating_sub(2);

        // Calcular la ventana visible: mantiene `scroll` salvo lo mínimo
        // necesario para que el foco quede dentro.
        let visible = self.flat_html.visible_rows();
        let start = if visible.is_empty() {
            0
        } else {
            let start = window_start(
                visible.len(),
                visible_pos(&visible, self.focus),
                visible_pos(&visible, self.scroll),
                content_height,
            );
            self.scroll = visible[start];
            start
        };

        // Clear screen + cursor arriba a la izquierda
        write!(stdout, "{}", termion::clear::All).unwrap();
        write!(stdout, "{}", termion::cursor::Goto(1, 1)).unwrap();
        write!(stdout, "{}", termion::cursor::Hide).unwrap();

        // Render HTML ajustado al ancho/alto reales, sin wrapping
        let active_row = self.active_match.and_then(|i| self.matches.get(i).copied());
        let hits = screen::Hits {
            matches: &self.matches,
            active: active_row,
        };
        let output = screen::render_html(
            &self.flat_html,
            &self.colors,
            width,
            content_height,
            self.focus,
            start,
            hits,
        );

        let total = self.flat_html.rows.len();
        let cur = (self.focus + 1).min(total.max(1));
        let pct = (cur * 100).checked_div(total).unwrap_or(100);

        // Barra superior: marca + fichero a la izquierda, tipo de documento a la derecha
        let header_left = format!(
            "{} hless {}│ {}{}",
            screen::FG_ACCENT, screen::FG_SEP, screen::FG_STRONG, self.filename
        );
        let header_right = format!(
            "{}HTML {}· {}{} rows ",
            screen::FG_ACCENT, screen::FG_SEP, screen::FG_DIM, total
        );
        let header = screen::bar(&header_left, &header_right, width, screen::BAR_BG_HEADER);
        stdout.write_all(header.as_bytes()).unwrap();
        stdout.write_all(b"\r\n").unwrap();

        // Print content (cada fila ya viene truncada a `width` y sin wrapping)
        write!(stdout, "{}", output).unwrap();

        // Barra de estado: posición + atajos a la izquierda, porcentaje a la derecha
        let sep = format!("{} · ", screen::FG_SEP);
        let hints = format!(
            "{k}q{d} quit{s}{k}↑↓{d} nav{s}{k}←→{d} fold{s}{k}/{d} search",
            k = screen::FG_KEY,
            d = screen::FG_DIM,
            s = sep
        );
        let footer_left = format!(
            "{d} Row {a}{cur}{d}/{total} {sep}│ {hints} ",
            d = screen::FG_DIM,
            a = screen::FG_STRONG,
            cur = cur,
            total = total,
            sep = screen::FG_SEP,
            hints = hints
        );
        let footer = if let Some(input) = &self.search_input {
            // Modo entrada: prompt con el texto y el cursor
            let left = format!(
                " {a}/{d} {h}{input}{reset}▏ ",
                a = screen::FG_ACCENT,
                d = screen::FG_DIM,
                h = screen::FG_STRONG,
                input = input,
                reset = screen::RESET,
            );
            let right = match self.matches.len() {
                0 => format!("{}sin coincidencias{} ", screen::FG_DIM, screen::FG_DIM),
                n => {
                    let pos = self.active_match.map(|i| i + 1).unwrap_or(0);
                    format!("{}{}/{} {} ", screen::FG_ACCENT, pos, n, screen::FG_DIM)
                }
            };
            screen::bar(&left, &right, width, screen::BAR_BG_FOOTER)
        } else if !self.query.is_empty() {
            // Búsqueda confirmada: muestra la consulta y n/N
            let (pos, total_m) = (
                self.active_match.map(|i| i + 1).unwrap_or(0),
                self.matches.len(),
            );
            let left = format!(
                " {d}Row {a}{cur}{d}/{total} {sep}│ {k}/{d} {h}{query}{reset}{sep}│ {d}{pos}/{total_m} {k}n{d} next {k}N{d} prev ",
                d = screen::FG_DIM,
                a = screen::FG_STRONG,
                cur = cur,
                total = total,
                sep = screen::FG_SEP,
                k = screen::FG_KEY,
                h = screen::FG_STRONG,
                query = self.query,
                reset = screen::RESET,
                pos = pos,
                total_m = total_m,
            );
            screen::bar(&left, "", width, screen::BAR_BG_FOOTER)
        } else {
            let footer_right = format!("{}{}%{} ", screen::FG_ACCENT, pct, screen::FG_DIM);
            screen::bar(&footer_left, &footer_right, width, screen::BAR_BG_FOOTER)
        };
        write!(stdout, "{}", footer).unwrap();
        
        let _ = stdout.flush();
    }

    fn navigate(&mut self, direction: Direction) {
        match direction {
            Direction::Up => {
                if let Some(prev) = self.flat_html.prev_visible_row(self.focus) {
                    self.focus = prev;
                }
            }
            Direction::Down => {
                if let Some(next) = self.flat_html.next_visible_row(self.focus) {
                    self.focus = next;
                }
            }
        }
    }

    fn collapse(&mut self) {
        // Como jless: si ya está colapsado (u hoja), saltar al padre visible.
        if self.flat_html.rows[self.focus].has_children()
            && !self.flat_html.is_collapsed(self.focus)
        {
            self.flat_html.collapse(self.focus);
        } else {
            self.focus = self.flat_html.first_visible_ancestor(self.focus);
        }
    }

    fn expand(&mut self) {
        self.flat_html.expand(self.focus);
    }

    /// Filas que se desplaza una página (alto útil, dejando cabecera y pie).
    fn page_step(&self) -> usize {
        let height = termion::terminal_size().unwrap_or((80, 24)).1 as usize;
        height.saturating_sub(2).max(1)
    }

    fn page_up(&mut self) {
        self.page(self.page_step(), false);
    }

    fn page_down(&mut self) {
        self.page(self.page_step(), true);
    }

    /// Desplaza una página en el eje visible con altura útil `h`. Con una
    /// página completa por delante, la vista y el foco se mueven lo mismo, así
    /// que el cursor conserva su fila en pantalla; cerca de un extremo salta al
    /// principio o al final.
    fn page(&mut self, h: usize, down: bool) {
        let visible = self.flat_html.visible_rows();
        if visible.is_empty() {
            return;
        }
        let (s, f) = page_move(
            visible.len(),
            visible_pos(&visible, self.scroll),
            visible_pos(&visible, self.focus),
            h,
            down,
        );
        self.scroll = visible[s];
        self.focus = visible[f];
    }

    fn go_home(&mut self) {
        self.focus = 0;
    }

    fn go_end(&mut self) {
        self.focus = self.flat_html.last_visible_item();
    }

    // --- Búsqueda ---

    fn start_search(&mut self) {
        self.search_origin = self.focus;
        self.search_input = Some(String::new());
        self.query.clear();
        self.matches.clear();
        self.active_match = None;
    }

    /// Recalcula coincidencias mientras se escribe y salta a la primera a
    /// partir del origen (con vuelta al principio si no hay ninguna después).
    fn refresh_search(&mut self) {
        let needle = match self.search_input.as_ref() {
            Some(s) => s.to_lowercase(),
            None => self.query.to_lowercase(),
        };
        if needle.is_empty() {
            self.matches.clear();
            self.active_match = None;
            return;
        }
        self.matches = self.flat_html.search_rows(&needle);
        if self.matches.is_empty() {
            self.active_match = None;
            return;
        }
        let origin = self.search_origin;
        let idx = self
            .matches
            .iter()
            .position(|&r| r >= origin)
            .unwrap_or(0);
        self.active_match = Some(idx);
        self.jump_to_active();
    }

    fn confirm_search(&mut self) {
        match self.search_input.take() {
            Some(s) if s.is_empty() => self.cancel_search(),
            Some(s) => {
                self.query = s;
                self.refresh_search();
            }
            None => {}
        }
    }

    fn cancel_search(&mut self) {
        self.search_input = None;
        self.matches.clear();
        self.active_match = None;
        self.query.clear();
        self.focus = self.search_origin;
    }

    fn search_next(&mut self) {
        self.step_match(1);
    }

    fn search_prev(&mut self) {
        self.step_match(-1);
    }

    fn step_match(&mut self, delta: isize) {
        let len = self.matches.len();
        if len == 0 {
            return;
        }
        let cur = self.active_match.unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len as isize) as usize;
        self.active_match = Some(next);
        // Al saltar con n/N se busca en todo el documento: el "origen" pasa a
        // ser el foco actual para que un nuevo / siga desde aquí.
        self.search_origin = self.focus;
        self.jump_to_active();
    }

    /// Despliega los ancestros de la coincidencia actual y lleva el foco allí.
    fn jump_to_active(&mut self) {
        if let Some(idx) = self.active_match {
            if let Some(&row) = self.matches.get(idx) {
                self.flat_html.reveal(row);
                self.focus = row;
            }
        }
    }
}

/// Resultado de una lectura de teclado.
enum Input {
    /// Se reconoció una tecla.
    Key(Key),
    /// Llegaron bytes que no corresponden a ninguna tecla (se ignoran).
    Ignore,
    /// Fin de entrada (o error de E/S): hay que salir. Evita el bucle infinito
    /// cuando la entrada es un pipe ya consumido (p. ej. `curl ... | hless`).
    Eof,
}

/// Lee un bloque de bytes y lo traduce a una tecla.
fn read_key(keys: &mut dyn Read) -> Input {
    let mut buffer = [0u8; 16];
    match keys.read(&mut buffer) {
        Ok(0) | Err(_) => Input::Eof,
        Ok(n) => match decode_key(&buffer[..n]) {
            Some(key) => Input::Key(key),
            None => Input::Ignore,
        },
    }
}

/// Posición de `row` dentro de la lista de filas visibles. Si la fila ya no es
/// visible (p. ej. quedó dentro de un nodo plegado) devuelve la del primer
/// visible que la sigue, o la última.
fn visible_pos(visible: &[usize], row: usize) -> usize {
    match visible.iter().position(|&r| r == row) {
        Some(p) => p,
        None => visible
            .partition_point(|&r| r < row)
            .min(visible.len().saturating_sub(1)),
    }
}

/// Posición de la primera fila de la ventana. Respeta `scroll_pos` y solo lo
/// corrige lo justo para que `focus_pos` quede dentro de `[start, start+h)`.
fn window_start(len: usize, focus_pos: usize, scroll_pos: usize, h: usize) -> usize {
    if h == 0 || len <= h {
        return 0;
    }
    let mut start = scroll_pos.min(len - 1);
    if focus_pos < start {
        start = focus_pos;
    } else if focus_pos >= start + h {
        start = focus_pos + 1 - h;
    }
    start.min(len - h)
}

/// Calcula `(scroll, focus)` nuevos tras avanzar/retroceder una página, en
/// unidades de posición visible. Con una página completa disponible, vista y
/// foco se mueven lo mismo, de modo que el cursor conserva su fila en pantalla;
/// cerca de un extremo se salta al principio/final.
fn page_move(len: usize, start: usize, focus: usize, h: usize, down: bool) -> (usize, usize) {
    if len == 0 || h == 0 {
        return (start, focus);
    }
    if down {
        let max_start = len.saturating_sub(h);
        if start >= max_start {
            (max_start, len - 1)
        } else if start + h <= max_start {
            (start + h, (focus + h).min(len - 1))
        } else {
            (max_start, len - 1)
        }
    } else if start == 0 {
        (0, 0)
    } else if start >= h {
        (start - h, focus.saturating_sub(h))
    } else {
        (0, 0)
    }
}

/// Traduce los bytes leídos a una tecla. Los caracteres imprimibles se
/// devuelven como `Char` (la acción depende del modo: navegación o búsqueda);
/// el resto son teclas de control/flechas. Tolera que lleguen varias teclas
/// juntas (se queda con la primera).
fn decode_key(bytes: &[u8]) -> Option<Key> {
    if bytes.is_empty() {
        return None;
    }

    // Secuencias de escape (flechas, PageUp/Down, Home/End).
    if bytes[0] == 0x1b {
        if bytes.len() >= 3 {
            return match (bytes[1], bytes[2]) {
                (b'[', b'A') | (b'O', b'A') => Some(Key::Up),
                (b'[', b'B') | (b'O', b'B') => Some(Key::Down),
                (b'[', b'C') | (b'O', b'C') => Some(Key::Right),
                (b'[', b'D') | (b'O', b'D') => Some(Key::Left),
                (b'[', b'H') | (b'O', b'H') => Some(Key::Home),
                (b'[', b'F') | (b'O', b'F') => Some(Key::End),
                (b'[', b'5') => Some(Key::PageUp),   // \x1b[5~
                (b'[', b'6') => Some(Key::PageDown), // \x1b[6~
                _ => None,
            };
        }
        // ESC suelto: cancelar
        return Some(Key::Escape);
    }

    match bytes[0] {
        b'\r' | b'\n' => Some(Key::Enter),
        0x7f | 0x08 => Some(Key::Backspace),
        0x06 => Some(Key::PageDown), // Ctrl-f
        0x02 => Some(Key::PageUp),   // Ctrl-b
        0x0e => Some(Key::Down),     // Ctrl-n
        0x10 => Some(Key::Up),       // Ctrl-p
        0x09 => None,                // Tab: ignorar
        _ => std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| s.chars().next())
            .filter(|c| !c.is_control())
            .map(Key::Char),
    }
}

#[derive(Debug)]
enum Key {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Backspace,
    Escape,
}

#[derive(Debug)]
enum Direction {
    Up,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_page_and_nav_keys() {
        assert!(matches!(decode_key(b" "), Some(Key::Char(' '))));
        assert!(matches!(decode_key(b"f"), Some(Key::Char('f'))));
        assert!(matches!(decode_key(b"\x06"), Some(Key::PageDown))); // Ctrl-f
        assert!(matches!(decode_key(b"b"), Some(Key::Char('b'))));
        assert!(matches!(decode_key(b"\x02"), Some(Key::PageUp))); // Ctrl-b
        assert!(matches!(decode_key(b"\x1b[6~"), Some(Key::PageDown)));
        assert!(matches!(decode_key(b"\x1b[5~"), Some(Key::PageUp)));
        assert!(matches!(decode_key(b"\x1b[C"), Some(Key::Right)));
        assert!(matches!(decode_key(b"\x1b[B"), Some(Key::Down)));
        assert!(matches!(decode_key(b"\x1b"), Some(Key::Escape)));
        assert!(matches!(decode_key(b"\r"), Some(Key::Enter)));
        assert!(matches!(decode_key(b"\x7f"), Some(Key::Backspace)));
        assert!(matches!(decode_key(b"q"), Some(Key::Char('q'))));
        assert!(matches!(decode_key(b"/"), Some(Key::Char('/'))));
        // UTF-8 multibyte (é = 0xC3 0xA9)
        assert!(matches!(decode_key("é".as_bytes()), Some(Key::Char('é'))));
        // Varias teclas juntas: se queda con la primera
        assert!(matches!(decode_key(b"\x1b[Cb"), Some(Key::Right)));
        // Bytes vacíos no son ninguna tecla
        assert!(decode_key(b"").is_none());
    }

    #[test]
    fn window_keeps_scroll_and_contains_focus() {
        // h=10, 100 filas. Foco en 50 y scroll en 45: la ventana no se mueve.
        assert_eq!(window_start(100, 50, 45, 10), 45);
        // Foco justo encima de la ventana: se sube lo mínimo.
        assert_eq!(window_start(100, 40, 45, 10), 40);
        // Foco justo debajo de la ventana: se baja lo mínimo.
        assert_eq!(window_start(100, 60, 45, 10), 51);
        // Cerca del final, la ventana se pega al fondo.
        assert_eq!(window_start(100, 99, 45, 10), 90);
        // Todo cabe: siempre arriba.
        assert_eq!(window_start(5, 4, 0, 10), 0);
    }

    #[test]
    fn page_down_keeps_cursor_row_in_the_middle() {
        // h=20, 100 filas, foco en pantalla fila 10 (scroll 40, foco 50).
        let (s, f) = page_move(100, 40, 50, 20, true);
        assert_eq!(s, 60);
        assert_eq!(f, 70);
        assert_eq!(f - s, 10, "el cursor conserva su fila en pantalla");
    }

    #[test]
    fn page_down_at_the_bottom_goes_to_last() {
        let (s, f) = page_move(100, 80, 95, 20, true);
        assert_eq!(s, 80);
        assert_eq!(f, 99);
        // Sin página completa restante también se va al final.
        let (s, f) = page_move(100, 75, 80, 20, true);
        assert_eq!(s, 80);
        assert_eq!(f, 99);
    }

    #[test]
    fn page_up_keeps_cursor_row() {
        let (s, f) = page_move(100, 40, 50, 20, false);
        assert_eq!(s, 20);
        assert_eq!(f, 30);
        assert_eq!(f - s, 10);
        // Ya arriba: al primer elemento.
        assert_eq!(page_move(100, 0, 5, 20, false), (0, 0));
    }

    #[test]
    fn app_page_down_keeps_cursor_screen_row() {
        // Documento con 100 párrafos hermanos para tener varias páginas.
        let ps: String = (0..100).map(|i| format!("<p>P{i}</p>")).collect();
        let html = format!("<html><head><title>t</title></head><body>{ps}</body></html>");
        let mut app = App::new(&html, "t").unwrap();

        // Desplegar <html> y <body> para que los párrafos sean visibles.
        let html_idx = app.flat_html.rows.iter().position(|r| r.tag_name() == "html").unwrap();
        let body_idx = app.flat_html.rows.iter().position(|r| r.tag_name() == "body").unwrap();
        app.flat_html.expand(html_idx);
        app.flat_html.expand(body_idx);

        let h = 18;
        let visible = app.flat_html.visible_rows();
        // Foco en la fila de pantalla 8, vista arriba.
        app.scroll = visible[0];
        app.focus = visible[8];

        let screen_row = |app: &App| {
            let v = app.flat_html.visible_rows();
            v.iter().position(|&r| r == app.focus).unwrap()
                - v.iter().position(|&r| r == app.scroll).unwrap()
        };
        assert_eq!(screen_row(&app), 8);

        app.page(h, true);
        assert_eq!(screen_row(&app), 8, "el cursor conserva su fila al avanzar página");

        app.page(h, false);
        assert_eq!(screen_row(&app), 8, "el cursor conserva su fila al retroceder página");
    }

    #[test]
    fn search_finds_navigates_and_cancels() {
        let html = "<html><body><p>uno</p><p>dos</p><p>uno otra vez</p></body></html>";
        let mut app = App::new(html, "t").unwrap();

        // Escribir "uno" carácter a carácter (búsqueda incremental)
        app.start_search();
        for c in "uno".chars() {
            app.search_input.as_mut().unwrap().push(c);
            app.refresh_search();
        }
        assert_eq!(app.matches.len(), 2, "dos coincidencias");
        // El salto revela ancestros: el foco cae en la primera coincidencia
        let first = app.focus;
        assert!(app.flat_html.visible_rows().contains(&first));

        app.confirm_search();
        assert_eq!(app.query, "uno");
        assert!(app.search_input.is_none());

        // n avanza, N retrocede y da la vuelta
        app.search_next();
        let second = app.focus;
        assert_ne!(second, first);
        app.search_next();
        assert_eq!(app.focus, first, "n da la vuelta");
        app.search_prev();
        assert_eq!(app.focus, second, "N da la vuelta");

        // Cancelar restaura el foco de antes de empezar a buscar
        let origin = app.focus;
        app.start_search();
        app.search_input.as_mut().unwrap().push('x');
        app.refresh_search();
        app.cancel_search();
        assert_eq!(app.focus, origin);
        assert!(app.matches.is_empty());
        assert_eq!(app.query, "");
    }
}
