//! A bounded, non-executing parser for Publisle's TeX math contract.
use crate::{
    fonts::{Fonts, Shaped},
    model::{Diagnostic, Result},
};

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
    fn atom(&mut self) -> Result<Math> {
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
                '{' | '}' | '|' | '_' | '%' | '#' | '&' => Ok(Math::Operator(c.to_string())),
                '\\' => Ok(Math::Space(1.0)),
                _ => Err(self.error("Unknown escaped math token.")),
            };
        }
        let command: String = self.input[start..self.cursor].iter().collect();
        let symbol = match command.as_str() {
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
            "sin" | "cos" | "tan" | "exp" | "log" | "ln" | "lim" | "sup" | "inf" | "max"
            | "min" | "det" => Ok(Math::Operator(command)),
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
                let mut rows = Vec::new();
                for row in body.split("\\\\") {
                    rows.push(row.split('&').map(parse).collect::<Result<Vec<_>>>()?);
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

#[derive(Clone, Debug)]
pub struct Run {
    pub shaped: Shaped,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: [f32; 3],
}
#[derive(Clone, Debug)]
pub struct Rule {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub thickness: f32,
}
#[derive(Clone, Debug, Default)]
pub struct BoxLayout {
    pub runs: Vec<Run>,
    pub rules: Vec<Rule>,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub backgrounds: Vec<Background>,
}
#[derive(Clone, Debug)]
pub struct Background {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 3],
    pub border: Option<[f32; 3]>,
}
impl BoxLayout {
    pub fn scale(&mut self, factor: f32) {
        self.width *= factor;
        self.ascent *= factor;
        self.descent *= factor;
        for run in &mut self.runs {
            run.x *= factor;
            run.y *= factor;
            run.size *= factor;
            run.shaped.width *= factor;
            for glyph in &mut run.shaped.glyphs {
                glyph.x *= factor;
                glyph.y *= factor;
                glyph.advance *= factor;
            }
        }
        for rule in &mut self.rules {
            rule.x *= factor;
            rule.y *= factor;
            rule.width *= factor;
            rule.thickness *= factor;
        }
        for background in &mut self.backgrounds {
            background.x *= factor;
            background.y *= factor;
            background.width *= factor;
            background.height *= factor;
        }
    }
    pub fn append(&mut self, mut other: Self, x: f32, y: f32) {
        for run in &mut other.runs {
            run.x += x;
            run.y += y;
        }
        for rule in &mut other.rules {
            rule.x += x;
            rule.y += y;
        }
        for background in &mut other.backgrounds {
            background.x += x;
            background.y += y;
        }
        self.backgrounds.extend(other.backgrounds);
        self.runs.extend(other.runs);
        self.rules.extend(other.rules);
    }
}
pub fn layout(node: &Math, fonts: &mut Fonts, size: f32, color: [f32; 3]) -> Result<BoxLayout> {
    match node {
        Math::Text(text) | Math::Operator(text) | Math::PlainText(text) => {
            let shaped = fonts.shape(text, size, "regular", false)?;
            let width = shaped.width;
            Ok(BoxLayout {
                runs: vec![Run {
                    shaped,
                    x: 0.,
                    y: 0.,
                    size,
                    color,
                }],
                rules: vec![],
                width,
                ascent: size * 0.85,
                descent: size * 0.22,
                backgrounds: vec![],
            })
        }
        Math::Space(w) => Ok(BoxLayout {
            width: w * size,
            ..Default::default()
        }),
        Math::Row(nodes) => {
            let mut out = BoxLayout::default();
            for node in nodes {
                let b = layout(node, fonts, size, color)?;
                let x = out.width;
                out.width += b.width;
                out.ascent = out.ascent.max(b.ascent);
                out.descent = out.descent.max(b.descent);
                out.append(b, x, 0.);
            }
            Ok(out)
        }
        Math::Color(c, node) => layout(node, fonts, size, *c),
        Math::Fraction(a, b) => {
            let a = layout(a, fonts, size * 0.8, color)?;
            let b = layout(b, fonts, size * 0.8, color)?;
            let width = a.width.max(b.width) + size * 0.35;
            let ay = size * 0.3 + a.descent + size * 0.12;
            let by = -size * 0.12 - b.ascent;
            let mut out = BoxLayout {
                width,
                ascent: ay + a.ascent,
                descent: -by + b.descent,
                ..Default::default()
            };
            let ax = (width - a.width) / 2.;
            let bx = (width - b.width) / 2.;
            out.append(a, ax, ay);
            out.append(b, bx, by);
            out.rules.push(Rule {
                x: 0.,
                y: size * 0.18,
                width,
                thickness: (size * 0.045).max(0.35),
            });
            Ok(out)
        }
        Math::Scripts { base, sub, sup } => {
            let mut out = layout(base, fonts, size, color)?;
            if matches!(base.as_ref(), Math::Accent("underbrace", _))
                && sup.is_none()
                && let Some(sub) = sub
            {
                let b = layout(sub, fonts, size * 0.65, color)?;
                let x = (out.width - b.width) / 2.;
                let y = -out.descent - b.ascent - size * 0.1;
                out.descent = -y + b.descent;
                out.append(b, x, y);
                return Ok(out);
            }
            let base_width = out.width;
            let mut script_width: f32 = 0.;
            if let Some(sub) = sub {
                let b = layout(sub, fonts, size * 0.65, color)?;
                let y = -size * 0.3 - b.ascent * 0.25;
                script_width = script_width.max(b.width);
                out.descent = out.descent.max(-y + b.descent);
                out.append(b, base_width, y);
            }
            if let Some(sup) = sup {
                let b = layout(sup, fonts, size * 0.65, color)?;
                let y = size * 0.58;
                script_width = script_width.max(b.width);
                out.ascent = out.ascent.max(y + b.ascent);
                out.append(b, base_width, y);
            }
            out.width += script_width;
            Ok(out)
        }
        Math::IndexedRoot(index, node) => {
            let root = layout(&Math::Root(node.clone()), fonts, size, color)?;
            let index = layout(index, fonts, size * 0.55, color)?;
            let shift = index.width.max(size * 0.2);
            let y = size * 0.6;
            let mut out = BoxLayout {
                width: root.width + shift,
                ascent: root.ascent.max(y + index.ascent),
                descent: root.descent,
                ..Default::default()
            };
            out.append(index, 0., y);
            out.append(root, shift, 0.);
            Ok(out)
        }
        Math::Root(node) => {
            let b = layout(node, fonts, size, color)?;
            let root = layout(&Math::Operator("√".into()), fonts, size * 1.2, color)?;
            let rw = root.width;
            let mut out = BoxLayout {
                width: rw + b.width,
                ascent: b.ascent + size * 0.12,
                descent: b.descent,
                ..Default::default()
            };
            out.append(root, 0., 0.);
            out.rules.push(Rule {
                x: rw,
                y: b.ascent + size * 0.08,
                width: b.width,
                thickness: size * 0.04,
            });
            out.append(b, rw, 0.);
            Ok(out)
        }
        Math::Accent(kind, node) => {
            let mut out = layout(node, fonts, size, color)?;
            if *kind == "underbrace" {
                out.rules.push(Rule {
                    x: 0.,
                    y: -out.descent - size * 0.12,
                    width: out.width,
                    thickness: size * 0.04,
                });
                out.descent += size * 0.3;
                return Ok(out);
            }
            let y = out.ascent + size * 0.08;
            out.rules.push(Rule {
                x: 0.,
                y,
                width: out.width,
                thickness: size * 0.04,
            });
            if *kind == "arrow" {
                let arrow = layout(&Math::Operator("›".into()), fonts, size * 0.5, color)?;
                let x = out.width - arrow.width;
                out.append(arrow, x, y - size * 0.2);
            }
            out.ascent += size * 0.2;
            Ok(out)
        }
        Math::Over(base, label) => {
            let base = layout(base, fonts, size, color)?;
            let label = layout(label, fonts, size * 0.65, color)?;
            let width = base.width.max(label.width);
            let y = base.ascent + label.descent + size * 0.15;
            let mut out = BoxLayout {
                width,
                ascent: y + label.ascent,
                descent: base.descent,
                ..Default::default()
            };
            let x = (width - base.width) / 2.;
            out.append(base, x, 0.);
            let x = (width - label.width) / 2.;
            out.append(label, x, y);
            Ok(out)
        }
        Math::Background(background, border, node) => {
            let b = layout(node, fonts, size, color)?;
            let padding = size * 0.2;
            let mut out = BoxLayout {
                width: b.width + 2. * padding,
                ascent: b.ascent + padding,
                descent: b.descent + padding,
                ..Default::default()
            };
            out.backgrounds.push(Background {
                x: 0.,
                y: -out.descent,
                width: out.width,
                height: out.ascent + out.descent,
                color: *background,
                border: *border,
            });
            out.append(b, padding, 0.);
            Ok(out)
        }
        Math::Grid(rows) => {
            let mut cells = Vec::new();
            let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
            let mut widths = vec![0f32; cols];
            for row in rows {
                let mut cell_row = Vec::new();
                for (i, cell) in row.iter().enumerate() {
                    let b = layout(cell, fonts, size, color)?;
                    widths[i] = widths[i].max(b.width);
                    cell_row.push(b);
                }
                cells.push(cell_row);
            }
            let row_height = cells
                .iter()
                .flatten()
                .map(|b| b.ascent + b.descent)
                .fold(size * 1.4, f32::max)
                + size * 0.3;
            let height = row_height * cells.len() as f32;
            let mut out = BoxLayout {
                width: widths.iter().sum::<f32>() + size * (cols.saturating_sub(1)) as f32,
                ascent: height * 0.5,
                descent: height * 0.5,
                ..Default::default()
            };
            for (r, row) in cells.into_iter().enumerate() {
                let mut x = 0.;
                for (c, b) in row.into_iter().enumerate() {
                    let y = height * 0.5 - row_height * (r as f32 + 0.8);
                    out.append(b, x, y);
                    x += widths[c] + size;
                }
            }
            Ok(out)
        }
    }
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
                .contains("<mroot><mi>x</mi><mrow><mn>3</mn></mrow></mroot>")
        );
        for source in [r"\sqrt[]{x}", r"\sqrt[3{x}", r"\sqrt[\input{x}]{y}"] {
            assert!(parse(source).is_err(), "{source}");
        }
    }
}
