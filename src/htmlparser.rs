/// Represents an HTML node in the parsed tree
#[derive(Debug, Clone)]
pub enum HtmlNode {
    Element {
        tag: String,
        attributes: Vec<(String, String)>,
        children: Vec<HtmlNode>,
    },
    Text(String),
    Comment(String),
    Doctype(String),
    CData(String),
}

impl HtmlNode {
    pub fn is_void_element(&self) -> bool {
        matches!(
            self.tag_name(),
            "area" | "base" | "br" | "col" | "embed" | "hr" | "img"
                | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
        )
    }

    pub fn tag_name(&self) -> &str {
        match self {
            HtmlNode::Element { tag, .. } => tag.as_str(),
            _ => "",
        }
    }

    pub fn children(&self) -> &[HtmlNode] {
        match self {
            HtmlNode::Element { children, .. } => children,
            _ => &[],
        }
    }
}

/// Elementos cuyo contenido es texto literal (raw text / RCDATA) y no debe
/// parsearse como HTML anidado.
fn is_raw_text_element(tag: &str) -> bool {
    matches!(
        tag.to_lowercase().as_str(),
        "script" | "style" | "textarea" | "title"
    )
}

/// Colapsa el espaciado (saltos de línea, tabuladores, espacios repetidos) para
/// poder mostrar el contenido en una sola línea sin romper el layout del TUI.
fn normalize_space(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_html(html: &str) -> Result<HtmlNode, String> {
    let chars: Vec<char> = html.chars().collect();
    let mut parser = HtmlParser { chars, pos: 0 };
    let root = parser.parse_document();
    Ok(root)
}

/// Indica si el árbol contiene al menos una etiqueta real (no solo el nodo
/// `root` sintético ni texto suelto). Sirve para validar la entrada.
pub fn has_real_element(node: &HtmlNode) -> bool {
    match node {
        HtmlNode::Element { tag, children, .. } => {
            if tag != "root" {
                true
            } else {
                children.iter().any(has_real_element)
            }
        }
        _ => false,
    }
}

struct HtmlParser {
    chars: Vec<char>,
    pos: usize,
}

impl HtmlParser {
    fn parse_document(&mut self) -> HtmlNode {
        let mut children = Vec::new();
        
        // Skip whitespace
        self.skip_whitespace();
        
        // Parse all top-level nodes
        while !self.at_end() {
            self.skip_whitespace();
            if self.at_end() { break; }
            if self.peek() == Some('<') {
                let node = self.parse_node();
                let is_empty_text = matches!(&node, HtmlNode::Text(t) if t.is_empty());
                if !is_empty_text {
                    children.push(node);
                }
            } else {
                let text = self.read_text();
                if !text.is_empty() {
                    children.push(HtmlNode::Text(text));
                }
            }
        }
        
        if children.len() == 1 {
            children.into_iter().next().unwrap()
        } else {
            HtmlNode::Element {
                tag: "root".to_string(),
                attributes: Vec::new(),
                children,
            }
        }
    }

    fn parse_node(&mut self) -> HtmlNode {
        if self.peek() != Some('<') {
            return HtmlNode::Text(self.read_text());
        }
        
        self.advance(); // skip '<'
        
        // Handle closing tags - just skip them
        if self.peek() == Some('/') {
            self.advance(); // skip '/'
            self.read_tag_name();
            self.skip_whitespace();
            if self.peek() == Some('>') {
                self.advance(); // skip '>'
            }
            return HtmlNode::Text(String::new());
        }
        
        // Handle comments, doctype y CDATA
        if self.peek() == Some('!') {
            if self.pos + 2 < self.chars.len() && self.chars[self.pos+1] == '-' && self.chars[self.pos+2] == '-' {
                self.pos += 3; // skip <!--
                let start = self.pos;
                while self.pos + 2 < self.chars.len() {
                    if self.chars[self.pos] == '-' && self.chars[self.pos+1] == '-' && self.chars[self.pos+2] == '>' {
                        break;
                    }
                    self.pos += 1;
                }
                let comment: String = self.chars[start..self.pos].iter().collect();
                self.pos += 3; // skip -->
                return HtmlNode::Comment(comment);
            }
            if self.pos + 7 < self.chars.len() && self.chars[self.pos+1] == '[' {
                let marker: String = self.chars[self.pos..(self.pos + 8).min(self.chars.len())].iter().collect();
                if marker == "[CDATA[" {
                    self.pos += 8; // skip ![CDATA[
                    let start = self.pos;
                    while self.pos + 2 < self.chars.len() {
                        if self.chars[self.pos] == ']' && self.chars[self.pos+1] == ']' && self.chars[self.pos+2] == '>' {
                            break;
                        }
                        self.pos += 1;
                    }
                    let data: String = self.chars[start..self.pos].iter().collect();
                    self.pos += 3; // skip ]]>
                    return HtmlNode::CData(data);
                }
            }
            let doctype = self.read_markup_declaration();
            return HtmlNode::Doctype(doctype);
        }
        
        // Parse element
        let tag = self.read_tag_name();
        
        // Read attributes
        let mut attributes = Vec::new();
        loop {
            self.skip_whitespace();
            if self.at_end() || self.peek() == Some('>') || self.peek() == Some('/') {
                break;
            }
            let attr_name = self.read_attribute_name();
            if attr_name.is_empty() { break; }
            
            self.skip_whitespace();
            if self.peek() == Some('=') {
                self.advance(); // skip '='
                self.skip_whitespace();
                let attr_value = self.read_attribute_value();
                attributes.push((attr_name, attr_value));
            } else {
                attributes.push((attr_name, String::new()));
            }
        }
        
        let void = self.is_void_element(&tag);
        
        // Check for self-closing
        let is_self_closing = if self.peek() == Some('/') {
            self.advance();
            true
        } else {
            false
        };
        
        if self.peek() == Some('>') {
            self.advance(); // skip '>'
        }
        
        if void || is_self_closing {
            return HtmlNode::Element {
                tag: tag.to_lowercase(),
                attributes,
                children: Vec::new(),
            };
        }

        // Elementos de "raw text": su contenido es texto literal (JS, CSS...),
        // no HTML, así que se lee entero hasta la etiqueta de cierre. Sin esto,
        // un `<` dentro de un `<script>` se interpretaría como una etiqueta.
        if is_raw_text_element(&tag) {
            let content = self.read_raw_text(&tag);
            let children = if content.is_empty() {
                Vec::new()
            } else {
                vec![HtmlNode::Text(content)]
            };
            return HtmlNode::Element {
                tag: tag.to_lowercase(),
                attributes,
                children,
            };
        }
        
        // Parse children - THIS IS THE KEY PART
        let mut children = Vec::new();
        loop {
            self.skip_whitespace();
            if self.at_end() { break; }
            
            // Check if we've hit the closing tag for this element
            if self.peek() == Some('<') && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == '/' {
                // This is a closing tag
                let saved_pos = self.pos;
                self.advance(); // skip '<'
                self.advance(); // skip '/'
                let closing_tag = self.read_tag_name();
                self.skip_whitespace();
                if self.peek() == Some('>') {
                    self.advance(); // skip '>'
                }

                // Check if this closing tag matches our opening tag
                // (case-insensitive, como en HTML)
                if closing_tag.to_lowercase() == tag.to_lowercase() {
                    // This is our closing tag, stop parsing children
                    break;
                }
                // Not our closing tag, restore position and continue
                self.pos = saved_pos;
            }

            // Parse a child node (ignora nodos de texto vacíos,
            // p. ej. restos de closing tags huérfanos)
            if self.peek() == Some('<') {
                let node = self.parse_node();
                let is_empty_text = matches!(&node, HtmlNode::Text(t) if t.is_empty());
                if !is_empty_text {
                    children.push(node);
                }
            } else {
                let text = self.read_text();
                if !text.is_empty() {
                    children.push(HtmlNode::Text(text));
                }
            }
        }
        
        HtmlNode::Element {
            tag: tag.to_lowercase(),
            attributes,
            children,
        }
    }

    fn read_tag_name(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn read_attribute_name(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == ':' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn read_attribute_value(&mut self) -> String {
        self.skip_whitespace();
        let quote = match self.peek() {
            Some('"') => {
                self.advance();
                Some('"')
            }
            Some('\'') => {
                self.advance();
                Some('\'')
            }
            _ => None,
        };
        
        let start = self.pos;
        let mut closed = false;
        if let Some(q) = quote {
            while self.pos < self.chars.len() {
                if self.chars[self.pos] == q {
                    closed = true;
                    break;
                }
                self.pos += 1;
            }
            let end = self.pos;
            if closed {
                self.pos += 1; // skip closing quote
            }
            self.chars[start..end].iter().collect::<String>().trim().to_string()
        } else {
            while self.pos < self.chars.len() {
                let ch = self.chars[self.pos];
                if ch == '>' || ch == ' ' || ch == '\t' || ch == '\n' {
                    break;
                }
                self.pos += 1;
            }
            // Collect chars to string safely
            let value: String = self.chars[start..self.pos].iter().collect();
            value.trim().to_string()
        }
    }

    fn read_text(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            if ch == '<' {
                break;
            }
            self.pos += 1;
        }
        // Collect chars to string safely
        let text: String = self.chars[start..self.pos].iter().collect();
        normalize_space(&text)
    }

    fn read_markup_declaration(&mut self) -> String {
        // Estamos en '!' (ya se consumió '<'). Guarda `!...` sin '<' ni '>'.
        let start = self.pos;
        while self.pos < self.chars.len() {
            if self.chars[self.pos] == '>' {
                break;
            }
            self.pos += 1;
        }
        let inner: String = self.chars[start..self.pos].iter().collect();
        if self.pos < self.chars.len() {
            self.pos += 1; // skip '>'
        }
        inner.trim().to_string()
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Lee el contenido literal de un elemento de raw text (`script`, `style`,
    /// `textarea`, `title`) hasta su etiqueta de cierre, sin interpretar `<`.
    /// Consume la etiqueta de cierre y normaliza el espaciado para mostrarlo en
    /// una sola línea.
    fn read_raw_text(&mut self, tag: &str) -> String {
        let lower_tag = tag.to_lowercase();
        let start = self.pos;
        let mut i = self.pos;
        let mut end = self.chars.len();

        while i < self.chars.len() {
            if self.chars[i] == '<' && self.chars.get(i + 1) == Some(&'/') {
                // Leer el nombre tras "</"
                let name_start = i + 2;
                let mut j = name_start;
                while j < self.chars.len()
                    && (self.chars[j].is_alphanumeric() || self.chars[j] == '-' || self.chars[j] == '_')
                {
                    j += 1;
                }
                let name: String = self.chars[name_start..j].iter().collect();
                if name.to_lowercase() == lower_tag {
                    end = i;
                    // Consumir hasta el '>' de la etiqueta de cierre
                    let mut k = j;
                    while k < self.chars.len() && self.chars[k] != '>' {
                        k += 1;
                    }
                    self.pos = if k < self.chars.len() { k + 1 } else { k };
                    return normalize_space(&self.chars[start..end].iter().collect::<String>());
                }
            }
            i += 1;
        }

        // Sin cierre: se toma el resto del documento.
        self.pos = self.chars.len();
        normalize_space(&self.chars[start..end].iter().collect::<String>())
    }

    fn is_void_element(&self, tag: &str) -> bool {
        matches!(
            tag.to_lowercase().as_str(),
            "area" | "base" | "br" | "col" | "embed" | "hr" | "img"
                | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
        )
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_real_element_detects_tags() {
        let doc = parse_html("<p>x</p>").unwrap();
        assert!(has_real_element(&doc));
        let text = parse_html("solo texto").unwrap();
        assert!(!has_real_element(&text));
        let empty = parse_html("").unwrap();
        assert!(!has_real_element(&empty));
        let multi = parse_html("texto <p>x</p> texto").unwrap();
        assert!(has_real_element(&multi));
    }

    #[test]
    fn test_closing_tags_produce_siblings() {
        // Regresión: el `</p>` debe cerrar el elemento; si no, el segundo
        // <p> quedaría anidado dentro del primero y todo el árbol se hunde.
        let html = "<div><p>uno</p><p>dos</p></div>";
        let node = parse_html(html).unwrap();
        match node {
            HtmlNode::Element { tag, children, .. } => {
                assert_eq!(tag, "div");
                assert_eq!(children.len(), 2);
                assert!(children.iter().all(|c| c.tag_name() == "p"));
            }
            _ => panic!("Expected Element"),
        }

        // Los closing tags son case-insensitive en HTML
        let html = "<DIV><P>x</P></DIV>";
        let node = parse_html(html).unwrap();
        assert_eq!(node.tag_name(), "div");
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn test_script_is_raw_text() {
        // El JS puede contener `<`, `>` o incluso HTML en un template literal:
        // no debe interpretarse como marcado.
        let html = r#"<script>const x = 1 < 2 && "<div>a</div>";</script><p>fin</p>"#;
        let node = parse_html(html).unwrap();
        assert_eq!(node.tag_name(), "root");
        let kids = node.children();
        assert_eq!(kids.len(), 2, "script y p deben ser hermanos");
        match &kids[0] {
            HtmlNode::Element { tag, children, .. } => {
                assert_eq!(tag, "script");
                assert_eq!(children.len(), 1);
                match &children[0] {
                    HtmlNode::Text(t) => assert!(t.contains("<div>a</div>"), "contenido crudo: {t}"),
                    _ => panic!("el contenido del script debe ser texto"),
                }
            }
            _ => panic!("el primer hijo debe ser <script>"),
        }
        assert_eq!(kids[1].tag_name(), "p");
    }

    #[test]
    fn test_style_is_raw_text() {
        let html = "<style>a::after{content:'<';}</style><div>x</div>";
        let node = parse_html(html).unwrap();
        assert_eq!(node.tag_name(), "root");
        assert_eq!(node.children()[0].tag_name(), "style");
        assert_eq!(node.children()[1].tag_name(), "div");
    }

    #[test]
    fn test_parse_simple_html() {
        let html = "<html><body><p>Hello</p></body></html>";
        let node = parse_html(html).unwrap();
        // Un único nodo raíz se devuelve sin envoltorio "root"
        assert_eq!(node.tag_name(), "html");
        assert!(!node.children().is_empty());
    }

    #[test]
    fn test_parse_attributes() {
        let html = r#"<div class="test" id="main">Hello</div>"#;
        let node = parse_html(html).unwrap();
        match node {
            HtmlNode::Element { tag, attributes, .. } => {
                assert_eq!(tag, "div");
                assert!(attributes.iter().any(|(k, v)| k == "class" && v == "test"));
            }
            _ => panic!("Expected Element"),
        }
    }

    #[test]
    fn test_parse_void_element() {
        let html = "<img src='test.png' alt='test'>";
        let node = parse_html(html).unwrap();
        match node {
            HtmlNode::Element { tag, .. } => {
                assert_eq!(tag, "img");
            }
            _ => panic!("Expected Element"),
        }
    }

    #[test]
    fn test_parse_comment() {
        let html = "<!-- This is a comment -->";
        let node = parse_html(html).unwrap();
        match node {
            HtmlNode::Comment(c) => {
                assert_eq!(c, " This is a comment ");
            }
            _ => panic!("Expected Comment"),
        }
    }

    #[test]
    fn test_parse_utf8() {
        let html = "<p>Hola mundo con áéíóú</p>";
        let node = parse_html(html).unwrap();
        match node {
            HtmlNode::Element { children, .. } => {
                assert!(children.iter().any(|c| {
                    match c {
                        HtmlNode::Text(t) => t.contains("áéíóú"),
                        _ => false,
                    }
                }));
            }
            _ => panic!("Expected Element"),
        }
    }
}
