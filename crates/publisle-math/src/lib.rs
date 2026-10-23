//! A bounded, non-executing parser for Publisle's TeX math contract.
/// A bounded math parsing failure. Arbitrary TeX execution is never supported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
}
impl Diagnostic {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Diagnostic {}
pub type Result<T> = std::result::Result<T, Diagnostic>;

#[derive(Clone, Debug)]
pub enum Math {
    Row(Vec<Math>),
    Text(String),
    PlainText(String),
    Operator(String),
    Space(f32),
    Fraction(Box<Math>, Box<Math>),
    Root(Box<Math>),
    IndexedRoot(Box<Math>, Box<Math>),
    Scripts {
        base: Box<Math>,
        sub: Option<Box<Math>>,
        sup: Option<Box<Math>>,
    },
    Color([f32; 3], Box<Math>),
    Accent(&'static str, Box<Math>),
    Over(Box<Math>, Box<Math>),
    Background([f32; 3], Option<[f32; 3]>, Box<Math>),
    Grid(Vec<Vec<Math>>),
}
pub fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
impl Math {
    pub fn mathml(&self) -> String {
        match self {
            Self::Row(nodes) => format!(
                "<mrow>{}</mrow>",
                nodes.iter().map(Self::mathml).collect::<String>()
            ),
            Self::Text(s) => {
                if s.chars().all(|c| c.is_ascii_digit()) {
                    format!("<mn>{}</mn>", xml(s))
                } else {
                    format!("<mi>{}</mi>", xml(s))
                }
            }
            Self::PlainText(s) => format!("<mtext>{}</mtext>", xml(s)),
            Self::Background(c, border, a) => {
                let content = format!(
                    "<mstyle mathbackground=\"#{:02x}{:02x}{:02x}\">{}</mstyle>",
                    (c[0] * 255.).round() as u8,
                    (c[1] * 255.).round() as u8,
                    (c[2] * 255.).round() as u8,
                    a.mathml()
                );
                if border.is_some() {
                    format!("<menclose notation=\"box\">{content}</menclose>")
                } else {
                    content
                }
            }
            Self::Operator(s) => format!("<mo>{}</mo>", xml(s)),
            Self::Space(w) => format!("<mspace width=\"{w}em\"/>"),
            Self::Fraction(a, b) => format!("<mfrac>{}{}</mfrac>", a.mathml(), b.mathml()),
            Self::Root(a) => format!("<msqrt>{}</msqrt>", a.mathml()),
            Self::IndexedRoot(index, a) => {
                format!("<mroot>{}{}</mroot>", a.mathml(), index.mathml())
            }
            Self::Scripts { base, sub, sup } => match (sub, sup) {
                (Some(a), Some(b)) => format!(
                    "<msubsup>{}{}{}</msubsup>",
                    base.mathml(),
                    a.mathml(),
                    b.mathml()
                ),
                (Some(a), None) => format!("<msub>{}{}</msub>", base.mathml(), a.mathml()),
                (None, Some(a)) => format!("<msup>{}{}</msup>", base.mathml(), a.mathml()),
                _ => base.mathml(),
            },
            Self::Color(c, a) => format!(
                "<mstyle mathcolor=\"#{:02x}{:02x}{:02x}\">{}</mstyle>",
                (c[0] * 255.) as u8,
                (c[1] * 255.) as u8,
                (c[2] * 255.) as u8,
                a.mathml()
            ),
            Self::Accent("underbrace", a) => format!(
                "<munder accentunder=\"true\">{}<mo>⏟</mo></munder>",
                a.mathml()
            ),
            Self::Over(a, b) => format!("<mover>{}{}</mover>", a.mathml(), b.mathml()),
            Self::Accent(kind, a) => format!(
                "<mover accent=\"true\">{}<mo>{}</mo></mover>",
                a.mathml(),
                if *kind == "bar" { "¯" } else { "→" }
            ),
            Self::Grid(rows) => format!(
                "<mtable>{}</mtable>",
                rows.iter()
                    .map(|row| format!(
                        "<mtr>{}</mtr>",
                        row.iter()
                            .map(|cell| format!("<mtd>{}</mtd>", cell.mathml()))
                            .collect::<String>()
                    ))
                    .collect::<String>()
            ),
        }
    }
}

struct Parser {
    input: Vec<char>,
    cursor: usize,
    depth: usize,
    nodes: usize,
}
pub fn parse(value: &str) -> Result<Math> {
    if value.len() > 64 * 1024 {
        return Err(Diagnostic::new("math-limit", "Math source exceeds 64 KiB."));
    }
    let mut parser = Parser {
        input: value.chars().collect(),
        cursor: 0,
        depth: 0,
        nodes: 0,
    };
    let result = parser.row(false)?;
    if parser.cursor != parser.input.len() {
        return Err(parser.error("Unexpected closing math group."));
    }
    Ok(result)
}
impl Parser {
    fn error(&self, message: &str) -> Diagnostic {
        Diagnostic::new(
            "unsupported-math",
            format!("{message} At math character {}.", self.cursor),
        )
    }
    fn skip(&mut self) {
        while self
            .input
            .get(self.cursor)
            .is_some_and(|c| c.is_whitespace())
        {
            self.cursor += 1;
        }
    }
    fn group_text(&mut self) -> Result<String> {
        self.skip();
        if self.input.get(self.cursor) != Some(&'{') {
            return Err(self.error("Expected a braced argument."));
        }
        self.cursor += 1;
        let start = self.cursor;
        let mut depth = 1;
        while let Some(&c) = self.input.get(self.cursor) {
            self.cursor += 1;
            if c == '{' {
                depth += 1;
            }
            if c == '}' {
                depth -= 1;
                if depth == 0 {
                    return Ok(self.input[start..self.cursor - 1].iter().collect());
                }
            }
        }
        Err(self.error("Unclosed argument."))
    }
    fn argument(&mut self) -> Result<Math> {
        self.skip();
        self.atom()
    }
    fn row(&mut self, group: bool) -> Result<Math> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self.error("Math nesting exceeds 64 levels."));
        }
        let mut row = Vec::new();
        loop {
            self.skip();
            match self.input.get(self.cursor) {
                None => {
                    if group {
                        return Err(self.error("Unclosed math group."));
                    }
                    break;
                }
                Some('}') => {
                    if group {
                        self.cursor += 1;
                    }
                    break;
                }
                _ => {}
            }
            let marker: Vec<char> = "\\color".chars().collect();
            if self.input[self.cursor..].starts_with(&marker)
                && !self
                    .input
                    .get(self.cursor + marker.len())
                    .is_some_and(|c| c.is_ascii_alphabetic())
            {
                if !group {
                    return Err(self.error("Color declarations require a scoped math group."));
                }
                self.cursor += marker.len();
                let name = self.group_text()?;
                let c = color(&name).ok_or_else(|| self.error("Unknown named math color."))?;
                row.push(Math::Color(c, Box::new(self.row(true)?)));
                self.depth -= 1;
                return Ok(Math::Row(row));
            }
            let mut base = self.atom()?;
            let mut sub = None;
            let mut sup = None;
            loop {
                self.skip();
                match self.input.get(self.cursor) {
                    Some('_') => {
                        if sub.is_some() {
                            return Err(self.error("Repeated subscript."));
                        }
                        self.cursor += 1;
                        sub = Some(Box::new(self.argument()?));
                    }
                    Some('^') => {
                        if sup.is_some() {
                            return Err(self.error("Repeated superscript."));
                        }
                        self.cursor += 1;
                        sup = Some(Box::new(self.argument()?));
                    }
                    _ => break,
                }
            }
            if sub.is_some() || sup.is_some() {
                base = Math::Scripts {
                    base: Box::new(base),
                    sub,
                    sup,
                };
            }
            row.push(base);
        }
        self.depth -= 1;
        Ok(Math::Row(row))
    }
    fn fragment(&mut self, value: &str) -> Result<Math> {
        let outer_input = std::mem::replace(&mut self.input, value.chars().collect());
        let outer_cursor = self.cursor;
        self.cursor = 0;
        let result = self.row(false);
        let complete = self.cursor == self.input.len();
        self.input = outer_input;
        self.cursor = outer_cursor;
        let node = result?;
        if !complete {
            return Err(self.error("Unexpected closing math group."));
        }
        Ok(node)
    }
    fn atom(&mut self) -> Result<Math> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self.error("Math nesting exceeds 64 levels."));
        }
        let result = self.atom_inner();
        self.depth -= 1;
        result
    }
    fn atom_inner(&mut self) -> Result<Math> {
        self.nodes += 1;
        if self.nodes > 8192 {
            return Err(self.error("Math exceeds 8192 nodes."));
        }
        let c = *self
            .input
            .get(self.cursor)
            .ok_or_else(|| self.error("Missing math argument."))?;
        self.cursor += 1;
        if c == '{' {
            return self.row(true);
        }
        if c == '}' || c == '_' || c == '^' {
            return Err(self.error("Unexpected math token."));
        }
        if c != '\\' {
            return Ok(if c.is_alphanumeric() {
                Math::Text(c.to_string())
            } else {
                Math::Operator(c.to_string())
            });
        }
        let start = self.cursor;
        while self
            .input
            .get(self.cursor)
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            self.cursor += 1;
        }
        if start == self.cursor {
            let c = *self
                .input
                .get(self.cursor)
                .ok_or_else(|| self.error("Incomplete command."))?;
            self.cursor += 1;
            return match c {
                ',' | ';' | ':' | ' ' | '!' => Ok(Math::Space(if c == '!' { -0.15 } else { 0.2 })),
                '{' | '}' | '|' | '_' | '%' | '#' | '&' | '$' => Ok(Math::Operator(c.to_string())),
                '\\' => Ok(Math::Space(1.0)),
                _ => Err(self.error("Unknown escaped math token.")),
            };
        }
        let command: String = self.input[start..self.cursor].iter().collect();
        let symbol = match command.as_str() {
            "rho" => "ρ",
            "xi" => "ξ",
            "eta" => "η",
            "zeta" => "ζ",
            "kappa" => "κ",
            "nu" => "ν",
            "chi" => "χ",
            "psi" => "ψ",
            "iota" => "ι",
            "Sigma" => "Σ",
            "Pi" => "Π",
            "Phi" => "Φ",
            "Psi" => "Ψ",
            "Lambda" => "Λ",
            "Theta" => "Θ",
            "vartheta" => "ϑ",
            "ell" => "ℓ",
            "hbar" => "ℏ",
            "aleph" => "ℵ",
            "forall" => "∀",
            "exists" => "∃",
            "emptyset" => "∅",
            "subset" => "⊂",
            "subseteq" => "⊆",
            "supset" => "⊃",
            "supseteq" => "⊇",
            "cup" => "∪",
            "cap" => "∩",
            "setminus" => "∖",
            "mapsto" => "↦",
            "implies" | "Longrightarrow" => "⟹",
            "iff" => "⟺",
            "ll" => "≪",
            "gg" => "≫",
            "propto" => "∝",
            "mid" => "∣",
            "circ" => "∘",
            "oplus" => "⊕",
            "otimes" => "⊗",
            "wedge" => "∧",
            "vee" => "∨",
            "neg" => "¬",
            "cdotp" => "⋅",
            "ast" => "∗",
            "star" => "⋆",
            "dagger" => "†",
            "prime" => "′",
            "oint" => "∮",
            "iiint" => "∭",
            "coprod" => "∐",
            "bigcup" => "⋃",
            "bigoplus" => "⨁",
            "lesssim" => "≲",
            "rightleftharpoons" => "⇌",
            "uparrow" => "↑",
            "hookrightarrow" => "↪",
            "Re" => "ℜ",
            "Im" => "ℑ",
            "angle" => "∠",
            "triangle" => "△",
            "degree" => "°",
            "checkmark" => "✓",
            "pi" => "π",
            "alpha" => "α",
            "beta" => "β",
            "gamma" => "γ",
            "delta" => "δ",
            "epsilon" | "varepsilon" => "ε",
            "theta" => "θ",
            "lambda" => "λ",
            "mu" => "μ",
            "sigma" => "σ",
            "omega" => "ω",
            "phi" | "varphi" => "φ",
            "tau" => "τ",
            "Delta" => "Δ",
            "Gamma" => "Γ",
            "Omega" => "Ω",
            "sum" => "∑",
            "prod" => "∏",
            "int" => "∫",
            "iint" => "∬",
            "infty" => "∞",
            "cdot" => "·",
            "times" => "×",
            "pm" => "±",
            "mp" => "∓",
            "le" | "leq" => "≤",
            "ge" | "geq" => "≥",
            "ne" | "neq" => "≠",
            "approx" => "≈",
            "sim" => "∼",
            "equiv" => "≡",
            "in" => "∈",
            "notin" => "∉",
            "perp" => "⊥",
            "partial" => "∂",
            "nabla" => "∇",
            "to" | "rightarrow" => "→",
            "longrightarrow" => "⟶",
            "leftarrow" => "←",
            "leftrightarrow" => "↔",
            "Rightarrow" => "⇒",
            "dots" | "ldots" => "…",
            "cdots" => "⋯",
            "langle" => "⟨",
            "rangle" => "⟩",
            "lvert" | "rvert" | "vert" => "|",
            "lVert" | "rVert" => "‖",
            "{grade}" => "°",
            _ => "",
        };
        if !symbol.is_empty() {
            return Ok(if symbol.chars().all(char::is_alphabetic) {
                Math::Text(symbol.into())
            } else {
                Math::Operator(symbol.into())
            });
        }
        match command.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let a = self.argument()?;
                let b = self.argument()?;
                Ok(Math::Fraction(Box::new(a), Box::new(b)))
            }
            "sqrt" => {
                self.skip();
                if self.input.get(self.cursor) == Some(&'[') {
                    self.cursor += 1;
                    let start = self.cursor;
                    let mut braces = 0usize;
                    loop {
                        match self.input.get(self.cursor) {
                            Some(']') if braces == 0 => break,
                            Some('{') => braces += 1,
                            Some('}') if braces > 0 => braces -= 1,
                            None => return Err(self.error("Unclosed root index.")),
                            _ => {}
                        }
                        self.cursor += 1;
                    }
                    if self.cursor == start {
                        return Err(self.error("Root index cannot be empty."));
                    }
                    // Parse with the same bounded parser so an index cannot
                    // reset the depth/node budget through recursive parse calls.
                    let index_source: Vec<char> = self.input[start..self.cursor].to_vec();
                    let outer_input = std::mem::replace(&mut self.input, index_source);
                    let outer_cursor = self.cursor + 1;
                    self.cursor = 0;
                    let index = self.row(false)?;
                    self.input = outer_input;
                    self.cursor = outer_cursor;
                    return Ok(Math::IndexedRoot(
                        Box::new(index),
                        Box::new(self.argument()?),
                    ));
                }
                Ok(Math::Root(Box::new(self.argument()?)))
            }
            "text" | "textrm" | "operatorname" | "mathrm" | "mathbf" | "mathit" | "mathsf"
            | "mathbb" | "mathcal" => {
                if self
                    .input
                    .get(self.cursor)
                    .is_some_and(|c| c.is_whitespace())
                {
                    self.skip();
                }
                if self.input.get(self.cursor) != Some(&'{') {
                    let value = self.argument()?;
                    return Ok(if command == "mathbb" {
                        match value {
                            Math::Text(text) => Math::Text(blackboard(&text)),
                            other => other,
                        }
                    } else {
                        value
                    });
                }
                let mut text = self.group_text()?;
                if text.contains('\\') || text.contains('{') {
                    return Err(
                        self.error("Nested commands in text/font arguments are unsupported.")
                    );
                }
                if command == "mathbb" {
                    text = blackboard(&text);
                }
                Ok(if command == "text" || command == "textrm" {
                    Math::PlainText(text)
                } else {
                    Math::Text(text)
                })
            }
            "colorbox" | "fcolorbox" => {
                let first = self.group_text()?;
                let first = color(&first).ok_or_else(|| self.error("Unknown named math color."))?;
                let (background, border) = if command == "fcolorbox" {
                    let second = self.group_text()?;
                    (
                        color(&second).ok_or_else(|| self.error("Unknown named math color."))?,
                        Some(first),
                    )
                } else {
                    (first, None)
                };
                let text = self.group_text()?;
                if text.contains('\\') || text.contains('{') {
                    return Err(self.error("Boxed math labels require plain text."));
                }
                Ok(Math::Background(
                    background,
                    border,
                    Box::new(Math::PlainText(text)),
                ))
            }
            "textcolor" => {
                let name = self.group_text()?;
                let color = color(&name).ok_or_else(|| self.error("Unknown named math color."))?;
                Ok(Math::Color(color, Box::new(self.argument()?)))
            }
            "color" => {
                Err(self.error("Use scoped textcolor rather than an unscoped color declaration."))
            }
            "overline" | "bar" => Ok(Math::Accent("bar", Box::new(self.argument()?))),
            "vec" | "overrightarrow" => Ok(Math::Accent("arrow", Box::new(self.argument()?))),
            "underbrace" => Ok(Math::Accent("underbrace", Box::new(self.argument()?))),
            "xrightarrow" => Ok(Math::Over(
                Box::new(Math::Operator("⟶".into())),
                Box::new(self.argument()?),
            )),
            "left" | "right" | "bigl" | "bigr" | "big" | "Big" => {
                self.skip();
                if self.input.get(self.cursor) == Some(&'.') {
                    self.cursor += 1;
                    Ok(Math::Space(0.0))
                } else {
                    self.atom()
                }
            }
            "quad" => Ok(Math::Space(1.0)),
            "qquad" => Ok(Math::Space(2.0)),
            "sinh" | "cosh" | "tanh" | "arg" | "gcd" | "Pr" | "bmod" | "sin" | "cos" | "tan"
            | "exp" | "log" | "ln" | "lim" | "sup" | "inf" | "max" | "min" | "det" => {
                Ok(Math::Operator(command))
            }
            "begin" => {
                let env = self.group_text()?;
                if ![
                    "matrix", "pmatrix", "bmatrix", "cases", "aligned", "align", "align*",
                ]
                .contains(&env.as_str())
                {
                    return Err(self.error("Unsupported math environment."));
                }
                let remaining: String = self.input[self.cursor..].iter().collect();
                let marker = format!("\\end{{{env}}}");
                let end = remaining
                    .find(&marker)
                    .ok_or_else(|| self.error("Unclosed math environment."))?;
                let body = &remaining[..end];
                if body.contains(r"\begin") {
                    return Err(self.error("Nested math environments are unsupported."));
                }
                let mut rows = Vec::new();
                for row in body.split("\\\\") {
                    rows.push(
                        row.split('&')
                            .map(|cell| self.fragment(cell))
                            .collect::<Result<Vec<_>>>()?,
                    );
                }
                self.cursor += remaining[..end + marker.len()].chars().count();
                let grid = Math::Grid(rows);
                Ok(match env.as_str() {
                    "pmatrix" => Math::Row(vec![
                        Math::Operator("(".into()),
                        grid,
                        Math::Operator(")".into()),
                    ]),
                    "bmatrix" => Math::Row(vec![
                        Math::Operator("[".into()),
                        grid,
                        Math::Operator("]".into()),
                    ]),
                    "cases" => Math::Row(vec![Math::Operator("{".into()), grid]),
                    _ => grid,
                })
            }
            _ => Err(self.error(&format!("Unsupported command \\{command}."))),
        }
    }
}
fn color(name: &str) -> Option<[f32; 3]> {
    Some(match name.to_ascii_lowercase().as_str() {
        "royalblue" => [0.255, 0.412, 0.882],
        "forestgreen" => [0.133, 0.545, 0.133],
        "orange" | "darkorange" => [1.0, 0.55, 0.0],
        "red" => [0.85, 0.05, 0.05],
        "blue" => [0.0, 0.0, 0.9],
        "green" => [0.0, 0.5, 0.0],
        "purple" => [0.5, 0.0, 0.5],
        "black" => [0.0; 3],
        "white" => [1.0; 3],
        "lightyellow" => [1., 1., 0.878],
        _ => return None,
    })
}
fn blackboard(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'R' => 'ℝ',
            'C' => 'ℂ',
            'N' => 'ℕ',
            'Z' => 'ℤ',
            'Q' => 'ℚ',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scripts_and_color_preserve_semantics() {
        let m = parse(r"X[k]=\sum_{n=0}^{N-1}\textcolor{RoyalBlue}{x[n]}e^{-2\pi ikn/N}")
            .unwrap()
            .mathml();
        assert!(m.contains("<msubsup>"));
        assert!(m.contains("mathcolor=\"#4169e0\""));
        assert!(m.contains("π"));
    }
    #[test]
    fn reject_execution_and_unclosed_groups() {
        for value in [r"\input{secret}", r"\write18{whoami}", r"\frac{x}", "x^{2"] {
            assert!(parse(value).is_err(), "{value}");
        }
    }
    #[test]
    fn fractions_and_roots() {
        let m = parse(r"\frac{1}{\sqrt{x^2+1}}").unwrap().mathml();
        assert!(m.contains("<mfrac>"));
        assert!(m.contains("<msqrt>"));
        assert!(m.contains("<msup>"));
    }
    #[test]
    fn environments() {
        assert!(
            parse(r"\begin{pmatrix}1&2\\3&4\end{pmatrix}")
                .unwrap()
                .mathml()
                .contains("<mtable>")
        );
    }
    #[test]
    fn supports_scoped_colors_boxed_labels_and_unbraced_blackboard_symbols() {
        let math=parse(r"{\color{RoyalBlue}a_n\cos(nx)}+\textcolor{black}{\colorbox{lightyellow}{Residual energy}}+\fcolorbox{RoyalBlue}{white}{Orthogonality}+\mathbb R").unwrap().mathml();
        assert!(math.contains("mathbackground"));
        assert!(math.contains("<mtext>Residual energy</mtext>"));
        assert!(math.contains("<menclose"));
        assert!(math.contains("ℝ"));
        assert!(
            parse(r"\sqrt[3]{x}")
                .unwrap()
                .mathml()
                .contains("<mroot><mrow><mi>x</mi></mrow><mrow><mn>3</mn></mrow></mroot>")
        );
        for source in [r"\sqrt[]{x}", r"\sqrt[3{x}", r"\sqrt[\input{x}]{y}"] {
            assert!(parse(source).is_err(), "{source}");
        }
    }
}

#[cfg(test)]
mod extraction_regressions {
    use super::*;
    #[test]
    fn common_greek_and_relations_from_gap_report() {
        let value = parse(r"\rho+\xi+\Sigma \subseteq \Phi \mapsto \forall x")
            .unwrap()
            .mathml();
        for symbol in ["ρ", "ξ", "Σ", "⊆", "Φ", "↦", "∀"] {
            assert!(value.contains(symbol));
        }
    }
    #[test]
    fn unbraced_commands_share_the_nesting_budget() {
        let value = format!("{}x", r"\frac".repeat(200));
        assert!(parse(&value).unwrap_err().message.contains("nesting"));
    }
    #[test]
    fn environment_cells_share_the_node_budget() {
        let value = format!(
            r"\begin{{matrix}}{}\end{{matrix}}",
            vec!["x".repeat(5000); 2].join("&")
        );
        assert!(parse(&value).unwrap_err().message.contains("8192"));
    }
    #[test]
    fn matrix_cells_cannot_discard_unmatched_closers() {
        assert!(parse(r"\begin{matrix}x}ignored\end{matrix}").is_err());
    }
}
