//! Detección del tipo de entrada y parseo de respuestas HTTP.
//!
//! `sniff` distingue entre HTML, una respuesta HTTP (con o sin cuerpo), una
//! entrada vacía y "cualquier otra cosa" (JSON, texto, cabeceras sueltas...).

/// Cabeceras de una respuesta HTTP (sin el cuerpo).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpHeaders {
    pub status_line: String,
    pub headers: Vec<(String, String)>,
}

/// Respuesta HTTP completa: cabeceras + cuerpo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpResponse {
    pub meta: HttpHeaders,
    pub body: String,
}

/// Clasificación de la entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    /// Parece un documento/fragmento HTML.
    Html,
    /// Respuesta HTTP: `curl -i` (cabeceras + cuerpo) o `curl -I` (solo cabeceras).
    Http(HttpResponse),
    /// Solo espacios en blanco.
    Empty,
    /// Ni HTML ni HTTP.
    Other,
}

/// Devuelve `true` si el texto (ya sin espacios iniciales) empieza como HTML:
/// `<` seguido de letra, `!` (doctype/comentario), `?` (declaración) o `/`.
pub fn looks_like_html(text: &str) -> bool {
    let mut chars = text.chars();
    if chars.next() != Some('<') {
        return false;
    }
    match chars.next() {
        Some(c) => c.is_ascii_alphabetic() || c == '!' || c == '?' || c == '/',
        None => false,
    }
}

/// Quita BOM y espacios iniciales.
fn trim_start(text: &str) -> &str {
    text.trim_start_matches(|c: char| c == '\u{feff}' || c.is_whitespace())
}

fn is_http_status_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    line.starts_with("HTTP/") && bytes.get(5).is_some_and(|c| c.is_ascii_digit())
}

/// Intenta interpretar `input` como una respuesta HTTP.
///
/// Acepta finales de línea `\r\n` y `\n`; el cuerpo es lo que sigue a la primera
/// línea vacía. Si no hay línea vacía, no hay cuerpo.
pub fn parse_http(input: &str) -> Option<HttpResponse> {
    let first_end = input.find('\n').unwrap_or(input.len());
    let status_line = input[..first_end]
        .trim_end_matches('\r')
        .trim_end()
        .to_string();
    if !is_http_status_line(&status_line) {
        return None;
    }

    let mut headers = Vec::new();
    let mut body = String::new();
    let mut rest = if first_end < input.len() {
        &input[first_end + 1..]
    } else {
        ""
    };

    loop {
        let (line, next) = match rest.find('\n') {
            Some(nl) => (&rest[..nl], Some(&rest[nl + 1..])),
            None => (rest, None),
        };
        let line = line.trim_end_matches('\r');

        if line.is_empty() {
            // Línea en blanco: empieza el cuerpo.
            if let Some(next) = next {
                body = next.to_string();
            }
            break;
        }

        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        } else {
            // Línea sin `:`: se conserva tal cual para no perder información.
            headers.push((line.trim().to_string(), String::new()));
        }

        match next {
            Some(next) => rest = next,
            None => break,
        }
    }

    Some(HttpResponse {
        meta: HttpHeaders {
            status_line,
            headers,
        },
        body,
    })
}

/// Clasifica la entrada.
pub fn sniff(input: &str) -> InputKind {
    let text = trim_start(input);
    if text.is_empty() {
        return InputKind::Empty;
    }
    if let Some(resp) = parse_http(text) {
        return InputKind::Http(resp);
    }
    if looks_like_html(text) {
        return InputKind::Html;
    }
    InputKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_documents_and_fragments() {
        assert_eq!(sniff("<!DOCTYPE html><html></html>"), InputKind::Html);
        assert_eq!(sniff("<html lang=\"es\">"), InputKind::Html);
        assert_eq!(sniff("<HTML>"), InputKind::Html);
        assert_eq!(sniff("\u{feff}  \n<div>x</div>"), InputKind::Html);
        assert_eq!(sniff("<!-- comentario -->"), InputKind::Html);
        assert_eq!(sniff("<?xml version=\"1.0\"?><svg/>"), InputKind::Html);
        assert_eq!(sniff("</div>"), InputKind::Html);
    }

    #[test]
    fn non_html() {
        assert_eq!(sniff(""), InputKind::Empty);
        assert_eq!(sniff("   \n\t "), InputKind::Empty);
        assert_eq!(sniff("{\"a\": 1}"), InputKind::Other);
        assert_eq!(sniff("Hola mundo"), InputKind::Other);
        assert_eq!(sniff("a < b"), InputKind::Other);
        assert_eq!(sniff("HTTP"), InputKind::Other);
    }

    #[test]
    fn parses_headers_only() {
        let input = "HTTP/2 200 \r\ndate: Thu, 23 Oct 2026 10:00:00 GMT\r\ncontent-type: text/html\r\n\r\n";
        match sniff(input) {
            InputKind::Http(resp) => {
                assert_eq!(resp.meta.status_line, "HTTP/2 200");
                assert_eq!(resp.meta.headers.len(), 2);
                assert_eq!(resp.meta.headers[1].0, "content-type");
                assert_eq!(resp.meta.headers[1].1, "text/html");
                assert!(resp.body.is_empty());
            }
            other => panic!("esperaba Http, {other:?}"),
        }
    }

    #[test]
    fn parses_headers_and_body() {
        let input = "HTTP/1.1 200 OK\nContent-Type: text/html\n\n<html><body>x</body></html>";
        let resp = parse_http(input).unwrap();
        assert_eq!(resp.meta.status_line, "HTTP/1.1 200 OK");
        assert_eq!(resp.meta.headers[0], ("Content-Type".to_string(), "text/html".to_string()));
        assert_eq!(resp.body, "<html><body>x</body></html>");
    }

    #[test]
    fn headers_with_colons_and_no_body() {
        let input = "HTTP/1.1 301 Moved Permanently\r\nlocation: https://x/y:8080\r\n\r\n";
        let resp = parse_http(input).unwrap();
        assert_eq!(
            resp.meta.headers[0],
            ("location".to_string(), "https://x/y:8080".to_string())
        );
        assert!(resp.body.is_empty());
    }
}
