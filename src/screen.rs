use crate::row::FlatHtml;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Colors and styling for HTML tags
pub struct Colors {
    pub tag: String,
    pub attr_name: String,
    pub attr_value: String,
    pub text: String,
    pub comment: String,
    pub indent: String,
    pub indicator: String,
    pub focus: String,
}

impl Colors {
    pub fn new() -> Self {
        Colors {
            tag: "\x1b[36m".to_string(),       // Cyan
            attr_name: "\x1b[33m".to_string(), // Yellow
            attr_value: "\x1b[32m".to_string(), // Green
            text: "\x1b[0m".to_string(),       // Default
            comment: "\x1b[35m".to_string(),   // Magenta
            indent: "\x1b[90m".to_string(),    // Gray
            indicator: "\x1b[90m".to_string(), // Gray
            focus: "\x1b[1;32m".to_string(),   // Bold green
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Colors::new()
    }
}

pub const RESET: &str = "\x1b[0m";
/// Limpia hasta final de línea: evita restos al repintar líneas más cortas.
pub const CLEAR_EOL: &str = "\x1b[K";

fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

/// Ancho visible de una cadena, ignorando secuencias ANSI `\x1b[...m`.
pub fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Saltar secuencia ANSI hasta la 'm' incluida
            for c2 in chars.by_ref() {
                if c2 == 'm' {
                    break;
                }
            }
            continue;
        }
        width += char_width(c);
    }
    width
}

/// Trunca una cadena (que puede contener ANSI) a `max_width` columnas visibles.
/// Si tiene que cortar, reserva una columna para `…`. Garantiza RESET al final.
pub fn truncate_ansi(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if visible_width(s) <= max_width {
        let mut out = s.to_string();
        if !s.ends_with(RESET) {
            out.push_str(RESET);
        }
        return out;
    }

    // Necesitamos cortar: dejamos sitio para "…"
    let budget = max_width.saturating_sub(1);
    let mut out = String::new();
    let mut width = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Copiar secuencia ANSI completa sin contar ancho
            out.push(c);
            for c2 in chars.by_ref() {
                out.push(c2);
                if c2 == 'm' {
                    break;
                }
            }
            continue;
        }
        let w = char_width(c);
        if width + w > budget {
            break;
        }
        out.push(c);
        width += w;
    }
    out.push('…');
    out.push_str(RESET);
    out
}

/// Quita las secuencias ANSI de una cadena (para repintarla con otro estilo).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c2 in chars.by_ref() {
                if c2 == 'm' || c2 == 'K' {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Trunca texto plano por ancho visible (sin romper UTF-8).
fn truncate_plain(s: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }
    let budget = max_width.saturating_sub(1);
    let mut out = String::new();
    let mut width = 0;
    for c in s.chars() {
        let w = char_width(c);
        if width + w > budget {
            break;
        }
        out.push(c);
        width += w;
    }
    out.push('…');
    out
}

/// Resaltado de una fila en función de la búsqueda activa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Highlight {
    None,
    /// Coincide con la búsqueda.
    Match,
    /// Es la coincidencia actual (donde está el cursor de búsqueda).
    Active,
}

/// Resultado de la búsqueda para el render: filas que coinciden (ordenadas) y
/// la coincidencia actual.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hits<'a> {
    pub matches: &'a [usize],
    pub active: Option<usize>,
}

/// Renders a row to a string, garantizando que ocupa como máximo `width` columnas.
pub fn render_row(
    row: &crate::row::Row,
    colors: &Colors,
    is_focus: bool,
    width: usize,
    hl: Highlight,
) -> String {
    let w = width.max(1);

    // Build the content based on node type (con colores, sin contar aún el ancho)
    // Se usan los helpers de Row para que formen parte del render real.
    let content = if row.is_text() {
        // El texto puede ser muy largo: se recorta en render final por ancho,
        // aquí solo se evita el corte a 40 bytes que rompía UTF-8.
        let t = truncate_plain(row.text_content(), 200);
        format!("{}\"{}\"", colors.text, t)
    } else if row.is_comment() {
        let inner = match &row.node {
            crate::htmlparser::HtmlNode::Comment(c) => c.as_str(),
            _ => "",
        };
        let c = truncate_plain(inner, 200);
        format!("{}<!--{}-->", colors.comment, c)
    } else {
        match &row.node {
            crate::htmlparser::HtmlNode::Element { .. } => {
                let tag = row.tag_name();
                let attributes = row.attributes();
                // `<tag` + attrs + `>` para no romper el ancho con piezas sueltas
                let mut s = String::new();
                s.push_str(&format!("{}<{}", colors.tag, tag));
                for (k, v) in attributes.iter() {
                    s.push_str(&format!(
                        " {}{}{}=\"{}{}{}\"",
                        colors.attr_name, k, colors.text, colors.attr_value, v, colors.tag
                    ));
                }
                s.push('>');

                let is_void = row.is_void_element();
                if is_void {
                    s
                } else if row.has_children() {
                    if row.is_collapsed() {
                        s.push_str(&format!(" {}[...]{}", colors.indicator, RESET));
                        s
                    } else {
                        s
                    }
                } else {
                    s
                }
            }
            crate::htmlparser::HtmlNode::Text(_) => {
                // Ya tratado arriba; inalcanzable.
                format!("{}\"{}\"", colors.text, row.text_content())
            }
            crate::htmlparser::HtmlNode::Comment(_) => {
                // Ya tratado arriba; inalcanzable.
                format!("{}<!--{}-->", colors.comment, row.text_content())
            }
            crate::htmlparser::HtmlNode::Doctype(content) => {
                if content.is_empty() {
                    format!("{}<!DOCTYPE html>", colors.comment)
                } else {
                    // `content` ya incluye la `!` inicial (p. ej. `!DOCTYPE html`)
                    format!("{}<{}>{}", colors.comment, content, RESET)
                }
            }
            crate::htmlparser::HtmlNode::CData(data) => {
                format!("{}<![CDATA[{}]]>{}", colors.comment, data, RESET)
            }
        }
    };

    // Prefijo de foco (2 columnas fijas para mantener alineación)
    let prefix = if is_focus {
        format!("{}▶ {}", colors.focus, RESET)
    } else {
        "  ".to_string()
    };

    // Indentación: 2 espacios por nivel. Si el nivel es muy profundo,
    // se limita para dejar siempre sitio al contenido.
    let max_indent_cols = w.saturating_sub(6).min(row.indent * 2);
    let indent_cols = (row.indent * 2).min(max_indent_cols);
    let indent_str = " ".repeat(indent_cols);
    let indent_colored = format!("{}{}{}", colors.indent, indent_str, RESET);

    // Indicador de folding separado del foco para no confundirlos
    let fold = if row.is_void_element() {
        "  ".to_string()
    } else if row.has_children() {
        if row.is_collapsed() {
            format!("{}▸{} ", colors.indicator, RESET)
        } else {
            format!("{}▾{} ", colors.indicator, RESET)
        }
    } else {
        "  ".to_string()
    };

    let full = format!("{}{}{}{}{}", prefix, indent_colored, fold, content, RESET);

    // En las coincidencias se repinta la línea entera con un único estilo: así
    // el resaltado cubre todo el ancho (los RESET internos de la fila ya no lo
    // cortan a mitad) y se lee bien sobre cualquier color de sintaxis.
    if hl != Highlight::None {
        let plain = truncate_plain(&strip_ansi(&full), w);
        let vw = UnicodeWidthStr::width(plain.as_str());
        let pad = " ".repeat(w.saturating_sub(vw));
        let style = match hl {
            Highlight::Active => "\x1b[48;5;33m\x1b[38;5;231m", // azul, texto claro
            _ => "\x1b[48;5;238m\x1b[38;5;253m",                 // gris tenue
        };
        return format!("{}{}{}{}{}", style, plain, pad, RESET, CLEAR_EOL);
    }

    format!("{}{}", truncate_ansi(&full, w), CLEAR_EOL)
}

/// Renderiza `FlatHtml` ajustado a `width x height`, mostrando la ventana que
/// empieza en la posición visible `start`. No produce wrapping: cada fila ocupa
/// exactamente una línea. `hits` resalta las coincidencias de búsqueda.
pub fn render_html(
    flat_html: &FlatHtml,
    colors: &Colors,
    width: usize,
    screen_height: usize,
    focus: usize,
    start: usize,
    hits: Hits,
) -> String {
    let w = width.max(1);
    if screen_height == 0 {
        return String::new();
    }

    // Índices visibles (respeta folding)
    let visible: Vec<usize> = (0..flat_html.rows.len())
        .filter(|&i| flat_html.is_row_visible(i))
        .collect();

    if visible.is_empty() {
        return (0..screen_height)
            .map(|_| format!("~{}", CLEAR_EOL))
            .collect::<Vec<_>>()
            .join("\r\n")
            + "\r\n";
    }

    let start = start.min(visible.len().saturating_sub(screen_height));

    let mut output = String::new();
    for (n, &idx) in visible.iter().skip(start).take(screen_height).enumerate() {
        if n > 0 {
            output.push_str("\r\n");
        }
        let row = &flat_html.rows[idx];
        let hl = if Some(idx) == hits.active {
            Highlight::Active
        } else if hits.matches.binary_search(&idx).is_ok() {
            Highlight::Match
        } else {
            Highlight::None
        };
        output.push_str(&render_row(row, colors, idx == focus, w, hl));
    }
    // Rellenar con líneas vacías (limpias) para borrar restos de frames previos
    let rendered = visible.len().saturating_sub(start).min(screen_height);
    for n in rendered..screen_height {
        if n > 0 {
            output.push_str("\r\n");
        }
        output.push_str(CLEAR_EOL);
    }
    output.push_str("\r\n");
    output
}

// --- Paleta de las barras (fondo a todo el ancho) ---
// Solo se fijan colores de primer plano/bold; el fondo de la barra lo pone
// `bar()`, que además cierra con RESET.
pub const BAR_BG_HEADER: &str = "\x1b[48;5;238m";
pub const BAR_BG_FOOTER: &str = "\x1b[48;5;236m";
pub const FG_ACCENT: &str = "\x1b[1;38;5;111m"; // azul claro (marca/teclas)
pub const FG_STRONG: &str = "\x1b[1;38;5;255m"; // blanco (valores)
pub const FG_DIM: &str = "\x1b[22;38;5;245m"; // gris (etiquetas/ayuda)
pub const FG_SEP: &str = "\x1b[22;38;5;240m"; // gris oscuro (separadores)
pub const FG_KEY: &str = "\x1b[1;38;5;216m"; // naranja suave (teclas)

/// Compone una barra de ancho completo: `left` pegado a la izquierda y `right`
/// pegado a la derecha, sobre el fondo `bg`. Ambos extremos admiten ANSI.
/// Si no cabe todo, se recorta por la derecha. La barra siempre ocupa `width`.
pub fn bar(left: &str, right: &str, width: usize, bg: &str) -> String {
    let w = width.max(1);
    let lw = visible_width(left);
    let rw = visible_width(right);

    let mut out = String::with_capacity(w + 16);
    out.push_str(bg);
    if lw + rw < w {
        out.push_str(left);
        out.push_str(&" ".repeat(w - lw - rw));
        out.push_str(right);
    } else {
        // No cabe: se prioriza lo de la izquierda y se recorta con "…".
        let truncated = truncate_ansi(left, w);
        let tw = visible_width(&truncated);
        out.push_str(&truncated);
        if tw < w {
            // `truncate_ansi` termina en RESET: reaplicamos el fondo al rellenar.
            out.push_str(bg);
            out.push_str(&" ".repeat(w - tw));
        }
    }
    out.push_str(RESET);
    out.push_str(CLEAR_EOL);
    out
}

/// Línea del popup de cabeceras: texto suelto o par clave/valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupLine {
    /// Texto sin estructura (p. ej. la status line).
    Text(String),
    /// `clave: valor`.
    Kv { key: String, value: String },
}

impl PopupLine {
    /// Ancho visible de la línea completa.
    pub fn width(&self) -> usize {
        match self {
            PopupLine::Text(s) => UnicodeWidthStr::width(s.as_str()),
            PopupLine::Kv { key, value } => {
                UnicodeWidthStr::width(key.as_str()) + 2 + UnicodeWidthStr::width(value.as_str())
            }
        }
    }

    /// Segmentos (texto, color) que componen la línea.
    fn segments(&self) -> Vec<(String, &'static str)> {
        match self {
            PopupLine::Text(s) => vec![(s.clone(), FG_ACCENT)],
            PopupLine::Kv { key, value } => vec![
                (key.clone(), FG_KEY),
                (": ".to_string(), FG_SEP),
                (value.clone(), FG_STRONG),
            ],
        }
    }
}

/// Recorta segmentos con color: salta `skip` columnas y muestra hasta `max_w`.
fn slice_segments(segments: &[(String, &'static str)], skip: usize, max_w: usize) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    let mut written = 0usize;

    for (text, color) in segments {
        let mut color_set = false;
        for ch in text.chars() {
            let w = char_width(ch);
            if col < skip {
                col += w;
                continue;
            }
            if written + w > max_w {
                out.push_str(RESET);
                return out;
            }
            if !color_set {
                out.push_str(color);
                color_set = true;
            }
            out.push(ch);
            written += w;
            col += w;
        }
    }
    out.push_str(RESET);
    out
}

/// Renderiza una ventana emergente con borde, ocupando exactamente
/// `width x height`. `scroll` es la primera línea visible y `hscroll` el
/// desplazamiento horizontal en columnas. La primera línea (status HTTP) se
/// resalta y las cabeceras muestran la clave en otro color.
pub fn render_popup(
    title: &str,
    lines: &[PopupLine],
    width: usize,
    height: usize,
    scroll: usize,
    hscroll: usize,
) -> String {
    let w = width.max(1);
    if height == 0 {
        return String::new();
    }

    let mut rows: Vec<String> = Vec::with_capacity(height);

    if w >= 10 && height >= 3 {
        let inner_w = w - 4;
        let inner_h = height - 2;

        // Borde superior con el título embebido.
        let title_trunc = truncate_plain(title, w.saturating_sub(8));
        let tw = UnicodeWidthStr::width(title_trunc.as_str());
        let mut top = format!("╭─ {} ", title_trunc);
        let fill = w.saturating_sub(tw + 5); // "╭─ " + título + " " + "╮"
        top.push_str(&"─".repeat(fill));
        top.push('╮');
        rows.push(format!("{}{}{}{}", FG_ACCENT, top, RESET, CLEAR_EOL));

        // Líneas de contenido, con scroll vertical y horizontal.
        for i in 0..inner_h {
            let idx = scroll + i;
            let txt = match lines.get(idx) {
                Some(line) => slice_segments(&line.segments(), hscroll, inner_w),
                None => RESET.to_string(),
            };
            let vw = visible_width(&txt);
            let pad = " ".repeat(inner_w.saturating_sub(vw));
            rows.push(format!(
                "{b}│ {r}{txt}{pad}{b} │{r}{k}",
                b = FG_SEP,
                r = RESET,
                txt = txt,
                pad = pad,
                k = CLEAR_EOL
            ));
        }

        // Borde inferior.
        let bottom = format!("╰{}╯", "─".repeat(w.saturating_sub(2)));
        rows.push(format!("{}{}{}{}", FG_SEP, bottom, RESET, CLEAR_EOL));
    } else {
        // Espacio demasiado pequeño: texto plano truncado.
        for i in 0..height {
            let txt = match lines.get(scroll + i) {
                Some(line) => slice_segments(&line.segments(), hscroll, w),
                None => RESET.to_string(),
            };
            rows.push(format!("{}{}", txt, CLEAR_EOL));
        }
    }

    let mut out = rows.join("\r\n");
    out.push_str("\r\n");
    out
}

/// Muestra un aviso en la primera línea del área de contenido (resto vacío).
pub fn render_notice(msg: &str, width: usize, height: usize) -> String {
    let w = width.max(1);
    if height == 0 {
        return String::new();
    }
    let mut out = String::new();
    for i in 0..height {
        if i > 0 {
            out.push_str("\r\n");
        }
        if i == 0 {
            let txt = truncate_plain(msg, w.saturating_sub(1));
            out.push_str(&format!(" {}{}{}{}", FG_DIM, txt, RESET, CLEAR_EOL));
        } else {
            out.push_str(CLEAR_EOL);
        }
    }
    out.push_str("\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::htmlparser::HtmlNode;
    use crate::row::{FlatHtml, Row};

    fn colors() -> Colors {
        Colors::new()
    }

    #[test]
    fn row_never_exceeds_width() {
        let row = Row::new(
            HtmlNode::Element {
                tag: "div".to_string(),
                attributes: vec![
                    ("class".to_string(), "una-clase-muy-larga-para-forzar-overflow".to_string()),
                    ("id".to_string(), "otro-atributo-largo".to_string()),
                ],
                children: vec![HtmlNode::Text("hola".to_string())],
            },
            10,
            None,
        );
        let out = render_row(&row, &colors(), false, 20, Highlight::None);
        assert!(visible_width(&out) <= 20, "ancho={}", visible_width(&out));
        // No debe haber wrapping
        assert!(!out.trim_end_matches(|c| c == '\n' || c == '\r').contains('\n'));
    }

    #[test]
    fn long_text_does_not_panic_nor_wrap() {
        let row = Row::new(HtmlNode::Text("áéíóú ".repeat(100)), 0, None);
        let out = render_row(&row, &colors(), true, 30, Highlight::None);
        assert!(visible_width(&out) <= 30);
    }

    #[test]
    fn render_html_keeps_focus_visible_and_fills_height() {
        let root = HtmlNode::Element {
            tag: "root".to_string(),
            attributes: vec![],
            children: (0..50)
                .map(|i| HtmlNode::Element {
                    tag: format!("p{i}"),
                    attributes: vec![],
                    children: vec![HtmlNode::Text("x".repeat(200))],
                })
                .collect(),
        };
        let flat = FlatHtml::from_html(&root);
        let out = render_html(&flat, &colors(), 40, 10, flat.rows.len() - 1, 0, Hits::default());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 10);
        for l in lines {
            assert!(visible_width(l) <= 40, "línea demasiado ancha: {l}");
        }
    }

    #[test]
    fn highlighted_row_is_full_width_and_single_style() {
        let row = Row::new(
            HtmlNode::Element {
                tag: "div".to_string(),
                attributes: vec![("class".to_string(), "container".to_string())],
                children: vec![HtmlNode::Text("hola".to_string())],
            },
            2,
            None,
        );
        let out = render_row(&row, &colors(), false, 60, Highlight::Active);
        // Ancho exacto y un único estilo (más RESET y limpiar-línea)
        assert_eq!(visible_width(&out), 60);
        let escapes = out.matches('\x1b').count();
        assert_eq!(escapes, 4, "dos códigos de estilo + RESET + limpiar-línea: {out:?}");
        assert!(strip_ansi(&out).contains("<div class=\"container\">"));
    }

    #[test]
    fn popup_fits_width_and_height() {
        let lines: Vec<PopupLine> = (0..10)
            .map(|i| PopupLine::Kv {
                key: format!("header-{i}"),
                value: "x".repeat(80),
            })
            .collect();
        let out = render_popup("HTTP headers", &lines, 40, 6, 0, 0);
        let ls: Vec<&str> = out.lines().collect();
        assert_eq!(ls.len(), 6, "debe ocupar exactamente la altura");
        for l in &ls {
            assert!(visible_width(l) <= 40, "línea demasiado ancha: {l:?}");
        }
        // Con el borde dibujado, título presente
        assert!(strip_ansi(&out).contains("HTTP headers"));
        assert!(strip_ansi(&out).contains('╭'));
        assert!(strip_ansi(&out).contains('╯'));
    }

    #[test]
    fn popup_colors_the_key_and_hscrolls() {
        let lines = vec![PopupLine::Kv {
            key: "content-type".to_string(),
            value: "text/html; charset=utf-8".to_string(),
        }];
        // Sin scroll horizontal: la clave va en color de tecla (naranja 216)
        let out = render_popup("t", &lines, 40, 3, 0, 0);
        assert!(out.contains(FG_KEY), "la clave debe ir resaltada");
        assert!(strip_ansi(&out).contains("content-type: text/html"));

        // Con scroll horizontal no se sale del ancho y se recorta por la izquierda
        let out = render_popup("t", &lines, 20, 3, 0, 10);
        for l in out.lines() {
            assert!(visible_width(l) <= 20);
        }
        let plain = strip_ansi(&out);
        assert!(!plain.contains("content-type"), "se ha desplazado: {plain:?}");
        assert!(plain.contains(": text/html"), "queda la cola visible: {plain:?}");
    }

    #[test]
    fn popup_minimal_space_falls_back_to_plain() {
        let lines = vec![PopupLine::Text("HTTP/2 200".to_string())];
        let out = render_popup("t", &lines, 8, 2, 0, 0);
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn truncate_ansi_ignores_escape_codes() {
        let s = format!("{}hola{}mundo", "\x1b[36m", RESET);
        let t = truncate_ansi(&s, 6);
        assert!(visible_width(&t) <= 6);
    }
}
