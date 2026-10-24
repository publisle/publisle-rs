use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[non_exhaustive]
/// Resolved native compiler input supplied by the trusted host.
pub struct Input {
    /// Native input contract version.
    pub schema_version: u32,
    /// Template.
    pub template: String,
    /// Template version.
    pub template_version: String,
    /// Pdf standard.
    pub pdf_standard: Option<String>,
    /// Article.
    pub article: Article,
    /// Citations.
    pub citations: Vec<String>,
    /// Bibliography.
    pub bibliography: Vec<Bibliography>,
    /// Assets.
    pub assets: Vec<Asset>,
    /// Pinned font files supplied by the host.
    pub fonts: Vec<FontSpec>,
    #[serde(default)]
    /// Template data.
    pub template_data: TemplateData,
}
#[derive(Debug, Default, Deserialize)]
/// Template data.
pub struct TemplateData {
    /// Country.
    pub country: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Article.
pub struct Article {
    /// Title.
    pub title: String,
    /// Authors.
    pub authors: Vec<String>,
    #[serde(default)]
    /// Author details.
    pub author_details: Vec<Author>,
    #[serde(default)]
    /// Subjects.
    pub subjects: Vec<String>,
    #[serde(default = "english")]
    /// Language.
    pub language: String,
    #[serde(default)]
    /// Direction.
    pub direction: Option<String>,
    /// Blocks.
    pub blocks: Vec<Block>,
}
fn english() -> String {
    "en".into()
}

#[derive(Debug, Deserialize)]
/// Author.
pub struct Author {
    /// Name.
    pub name: String,
    /// Affiliation.
    pub affiliation: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[non_exhaustive]
/// A resolved article block in source order.
pub enum Block {
    /// Heading.
    Heading {
        /// Heading depth starting at one.
        level: u8,
        /// Inline content in reading order.
        inlines: Vec<Inline>,
        #[serde(default)]
        /// Whether this block belongs to the abstract.
        r#abstract: bool,
        /// Optional cross-reference label.
        label: Option<String>,
    },
    /// Paragraph.
    Paragraph {
        /// Inline content in reading order.
        inlines: Vec<Inline>,
        #[serde(default)]
        /// Whether this block belongs to the abstract.
        r#abstract: bool,
    },
    /// List.
    List {
        /// Ordered.
        ordered: bool,
        /// Items.
        items: Vec<Vec<Inline>>,
    },
    /// Quote.
    Quote {
        /// Quoted paragraphs in reading order.
        paragraphs: Vec<Vec<Inline>>,
    },
    /// Code.
    Code {
        /// Literal code text.
        value: String,
    },
    /// Bounded math syntax tree independent of PDF fonts and layout.
    Math {
        /// Authored text or TeX math source.
        value: String,
        /// Whether this is a numbered display equation.
        display: bool,
        /// Optional cross-reference label.
        label: Option<String>,
    },
    /// Figure.
    Figure {
        /// Src.
        src: String,
        /// Alt.
        alt: String,
        /// Caption inline content.
        caption: Vec<Inline>,
        /// Optional cross-reference label.
        label: Option<String>,
    },
    /// Table.
    Table {
        /// Rectangular table cell contents.
        rows: Vec<Vec<Vec<Inline>>>,
        #[serde(default)]
        /// Caption inline content.
        caption: Vec<Inline>,
        #[serde(default, rename = "headerRows")]
        /// Number of leading header rows.
        header_rows: usize,
        /// Optional cross-reference label.
        label: Option<String>,
    },
    /// Footnote.
    Footnote {
        /// Id.
        id: String,
        /// Paragraphs in reading order.
        paragraphs: Vec<Vec<Inline>>,
    },
    /// Bibliography.
    Bibliography,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
/// Inline.
pub enum Inline {
    /// Text.
    Text {
        /// Authored text or TeX math source.
        value: String,
        /// Mark.
        mark: Option<String>,
        /// Direction.
        direction: Option<String>,
        /// Reference.
        reference: Option<Reference>,
    },
    /// Bounded math syntax tree independent of PDF fonts and layout.
    Math {
        /// TeX math source.
        value: String,
    },
    /// Link.
    Link {
        /// Visible link text.
        value: String,
        /// Destination URL.
        url: String,
    },
    /// Citation.
    Citation {
        /// Resolved citation cluster.
        cluster: serde_json::Value,
        /// Resolved citation text.
        text: Option<String>,
    },
    /// Break.
    Break,
}
#[derive(Clone, Debug, Deserialize)]
/// Reference.
pub struct Reference {
    /// Kind.
    pub kind: String,
    /// Target.
    pub target: String,
}
#[derive(Debug, Deserialize)]
/// Bibliography.
pub struct Bibliography {
    /// Id.
    pub id: String,
    /// Label.
    pub label: String,
    /// Text.
    pub text: String,
}
#[derive(Debug, Deserialize)]
/// Asset.
pub struct Asset {
    /// Source.
    pub source: String,
    /// Filesystem path to the host-supplied resource.
    pub path: PathBuf,
}
#[derive(Debug, Deserialize)]
/// Font spec.
pub struct FontSpec {
    /// Filesystem path to the host-supplied resource.
    pub path: PathBuf,
    /// Font role such as regular, italic, bold, mono, or fallback.
    pub role: String,
    #[serde(default)]
    /// Face index within a font collection.
    pub index: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
/// Compilation outcome, bounded diagnostics, and stage timings.
pub struct Report {
    /// Ok.
    pub ok: bool,
    /// Compiler version.
    pub compiler_version: &'static str,
    /// Pages.
    pub pages: usize,
    /// Diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Timings.
    pub timings: std::collections::BTreeMap<&'static str, u128>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// Diagnostic.
pub struct Diagnostic {
    /// Stable diagnostic identifier.
    pub code: String,
    /// Human-readable diagnostic detail.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Zero-based index of the source article block, when known.
    pub block_index: Option<usize>,
}
impl Diagnostic {
    /// New.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            block_index: None,
        }
    }
    /// At.
    pub fn at(mut self, index: usize) -> Self {
        self.block_index = Some(index);
        self
    }
}
/// A result carrying a structured diagnostic on failure.
pub type Result<T> = std::result::Result<T, Diagnostic>;

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Diagnostic {}

impl Input {
    /// Build a resolved input with no citations, bibliography, assets, or template data.
    /// Set those public fields before compiling when the article requires them.
    pub fn new(
        template: String,
        template_version: String,
        article: Article,
        fonts: Vec<FontSpec>,
    ) -> Self {
        Self {
            schema_version: 1,
            template,
            template_version,
            pdf_standard: None,
            article,
            citations: Vec::new(),
            bibliography: Vec::new(),
            assets: Vec::new(),
            fonts,
            template_data: TemplateData::default(),
        }
    }
}
