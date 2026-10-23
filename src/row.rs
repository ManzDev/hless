use crate::htmlparser::HtmlNode;

/// A row in the flattened HTML tree
#[derive(Debug, Clone)]
pub struct Row {
    /// The HTML node
    pub node: HtmlNode,
    /// The indent level (0 = top level)
    pub indent: usize,
    /// Whether this row is collapsed (children hidden)
    pub collapsed: bool,
    /// Index of the closing tag (if this is an opening tag)
    pub closing_index: Option<usize>,
    /// Parent row index (for navigation)
    pub parent: Option<usize>,
}

impl Row {
    pub fn new(node: HtmlNode, indent: usize, parent: Option<usize>) -> Self {
        Row {
            node,
            indent,
            collapsed: false,
            closing_index: None,
            parent,
        }
    }

    pub fn tag_name(&self) -> &str {
        match &self.node {
            HtmlNode::Element { .. } => self.node.tag_name(),
            HtmlNode::Text(_) => "#text",
            HtmlNode::Comment(_) => "#comment",
            HtmlNode::Doctype(_) => "#doctype",
            HtmlNode::CData(_) => "#cdata",
        }
    }

    pub fn attributes(&self) -> &[(String, String)] {
        match &self.node {
            HtmlNode::Element { attributes, .. } => attributes,
            _ => &[],
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(&self.node, HtmlNode::Text(_))
    }

    pub fn is_comment(&self) -> bool {
        matches!(&self.node, HtmlNode::Comment(_))
    }

    pub fn text_content(&self) -> &str {
        match &self.node {
            HtmlNode::Text(text) => text.as_str(),
            _ => "",
        }
    }

    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn is_void_element(&self) -> bool {
        self.node.is_void_element()
    }

    pub fn has_children(&self) -> bool {
        !self.node.children().is_empty()
    }

    /// Texto sobre el que se aplica la búsqueda (etiqueta, atributos, texto,
    /// comentario, ...).
    pub fn search_text(&self) -> String {
        match &self.node {
            HtmlNode::Element { tag, attributes, .. } => {
                let mut s = tag.clone();
                for (k, v) in attributes {
                    s.push(' ');
                    s.push_str(k);
                    s.push('=');
                    s.push_str(v);
                }
                s
            }
            HtmlNode::Text(t) => t.clone(),
            HtmlNode::Comment(c) => c.clone(),
            HtmlNode::Doctype(d) => d.clone(),
            HtmlNode::CData(d) => d.clone(),
        }
    }
}

/// Flat representation of the HTML tree for display
pub struct FlatHtml {
    pub rows: Vec<Row>,
}

impl FlatHtml {
    pub fn from_html(html: &HtmlNode) -> Self {
        let mut rows = Vec::new();
        // El parser envuelve varios nodos raíz en un <root> sintético:
        // no se muestra, sus hijos pasan a ser las filas de nivel 0.
        if let HtmlNode::Element { tag, children, .. } = html {
            if tag == "root" {
                for child in children {
                    if let HtmlNode::Text(text) = child {
                        if text.trim().is_empty() {
                            continue;
                        }
                    }
                    Self::flatten(child, 0, None, &mut rows);
                }
                return Self { rows };
            }
        }
        Self::flatten(html, 0, None, &mut rows);
        Self { rows }
    }

    fn flatten(node: &HtmlNode, indent: usize, parent: Option<usize>, rows: &mut Vec<Row>) {
        let row = Row::new(node.clone(), indent, parent);
        let current_idx = rows.len();
        rows.push(row);

        // Check if this node has children (only elements)
        if let HtmlNode::Element { children, .. } = &node {
            if !children.is_empty() {
                // Find the closing index (it's the last child + 1)
                let closing_idx = current_idx + 1; // Will be updated after processing children
                
                // Update the opening row with closing index
                rows[current_idx].closing_index = Some(closing_idx);
                
                // Process children - skip empty text nodes
                for child in children {
                    // Skip empty text nodes (whitespace only)
                    if let HtmlNode::Text(text) = child {
                        if text.trim().is_empty() {
                            continue;
                        }
                    }
                    Self::flatten(child, indent + 1, Some(current_idx), rows);
                }
                
                // Update closing index to point to the row after all children
                let last_child_idx = rows.len() - 1;
                rows[current_idx].closing_index = Some(last_child_idx + 1);
            }
        }
    }

    pub fn is_collapsed(&self, index: usize) -> bool {
        self.rows[index].is_collapsed()
    }

    /// Estado de vista inicial: deja plegados todos los elementos con hijos,
    /// de modo que se descubren los niveles uno a uno al expandir (→).
    /// Devuelve el índice donde debe quedar el foco (`<html>` si existe).
    pub fn collapse_all_default(&mut self) -> usize {
        let mut focus = 0;
        let mut found_html = false;
        for (i, row) in self.rows.iter_mut().enumerate() {
            if row.has_children() {
                row.collapsed = true;
                if !found_html && row.tag_name() == "html" {
                    focus = i;
                    found_html = true;
                }
            }
        }
        focus
    }

    pub fn toggle_collapsed(&mut self, index: usize) {
        self.rows[index].toggle_collapsed();
    }

    pub fn prev_visible_row(&self, mut index: usize) -> Option<usize> {
        while index > 0 {
            index -= 1;
            if self.is_row_visible(index) {
                return Some(index);
            }
        }
        None
    }

    pub fn next_visible_row(&self, mut index: usize) -> Option<usize> {
        while index < self.rows.len() - 1 {
            index += 1;
            if self.is_row_visible(index) {
                return Some(index);
            }
        }
        None
    }

    pub fn collapse(&mut self, index: usize) {
        if self.rows[index].has_children() && !self.is_collapsed(index) {
            self.toggle_collapsed(index);
        }
    }

    pub fn expand(&mut self, index: usize) {
        if self.rows[index].has_children() && self.is_collapsed(index) {
            self.toggle_collapsed(index);
        }
    }

    /// Ancestro visible más cercano (siguiendo la cadena de padres).
    /// Devuelve `index` si no tiene padre.
    pub fn first_visible_ancestor(&self, index: usize) -> usize {
        let mut current = index;
        while let Some(parent) = self.rows.get(current).and_then(|r| r.parent) {
            if self.is_row_visible(parent) {
                return parent;
            }
            current = parent;
        }
        current
    }

    /// Índices de las filas visibles (respetando el plegado).
    pub fn visible_rows(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&i| self.is_row_visible(i))
            .collect()
    }

    /// Filas cuyo texto contiene `query` (ya en minúsculas), en orden.
    pub fn search_rows(&self, query_lower: &str) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.search_text().to_lowercase().contains(query_lower))
            .map(|(i, _)| i)
            .collect()
    }

    /// Despliega los ancestros de `index` para que la fila quede visible.
    pub fn reveal(&mut self, index: usize) {
        let mut current = index;
        while let Some(parent) = self.rows[current].parent {
            self.rows[parent].collapsed = false;
            current = parent;
        }
    }

    pub fn last_visible_item(&self) -> usize {
        let mut i = self.rows.len();
        while i > 0 {
            i -= 1;
            if self.is_row_visible(i) {
                return i;
            }
        }
        0
    }

    pub fn is_row_visible(&self, index: usize) -> bool {
        // Walk up the parent chain to check if any ancestor is collapsed
        let mut current = index;
        while let Some(parent) = self.rows[current].parent {
            if self.is_collapsed(parent) {
                return false;
            }
            current = parent;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::htmlparser::HtmlNode;

    fn el(tag: &str, children: Vec<HtmlNode>) -> HtmlNode {
        HtmlNode::Element {
            tag: tag.to_string(),
            attributes: vec![],
            children,
        }
    }

    /// a, b, c con un hijo de texto cada uno. Tras plegar todo: solo a, b, c.
    fn sample() -> FlatHtml {
        let root = el(
            "root",
            vec![
                el("a", vec![HtmlNode::Text("x".into())]),
                el("b", vec![HtmlNode::Text("y".into())]),
                el("c", vec![HtmlNode::Text("z".into())]),
            ],
        );
        let mut flat = FlatHtml::from_html(&root);
        flat.collapse_all_default();
        flat
    }

    #[test]
    fn visible_rows_respects_folding() {
        let mut flat = sample();
        // Plegado: a(0), b(2), c(4)
        assert_eq!(flat.visible_rows(), vec![0, 2, 4]);
        flat.expand(0); // despliega 'a' -> aparece su texto (fila 1)
        assert_eq!(flat.visible_rows(), vec![0, 1, 2, 4]);
    }

    #[test]
    fn search_rows_and_reveal() {
        let mut flat = sample();
        // 'x' está en el texto de 'a' (fila 1), oculta al estar plegado
        assert_eq!(flat.search_rows("x"), vec![1]);
        assert_eq!(flat.search_rows("b"), vec![2]);
        assert!(flat.search_rows("nada").is_empty());

        flat.reveal(1);
        assert!(flat.visible_rows().contains(&1));
    }
}
