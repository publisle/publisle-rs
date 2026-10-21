use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub schema_version: u32,
    pub template: String,
    pub template_version: String,
    pub pdf_standard: Option<String>,
    pub article: Article,
    pub citations: Vec<String>,
    pub bibliography: Vec<Bibliography>,
    pub assets: Vec<Asset>,
    pub fonts: Vec<FontSpec>,
    #[serde(default)]
    pub template_data: TemplateData,
}
#[derive(Debug, Default, Deserialize)]
pub struct TemplateData {
    pub country: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    pub title: String,
    pub authors: Vec<String>,
    #[serde(default)]
    pub author_details: Vec<Author>,
    #[serde(default)]
    pub subjects: Vec<String>,
    #[serde(default = "english")]
    pub language: String,
    #[serde(default)]
    pub direction: Option<String>,
    pub blocks: Vec<Block>,
}
fn english() -> String {
    "en".into()
}

#[derive(Debug, Deserialize)]
pub struct Author {
    pub name: String,
    pub affiliation: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
        #[serde(default)]
        r#abstract: bool,
        label: Option<String>,
    },
    Paragraph {
        inlines: Vec<Inline>,
        #[serde(default)]
        r#abstract: bool,
    },
    List {
        ordered: bool,
        items: Vec<Vec<Inline>>,
    },
    Quote {
        paragraphs: Vec<Vec<Inline>>,
    },
    Code {
        value: String,
    },
    Math {
        value: String,
        display: bool,
        label: Option<String>,
    },
    Figure {
        src: String,
        alt: String,
        caption: Vec<Inline>,
        label: Option<String>,
    },
    Table {
        rows: Vec<Vec<Vec<Inline>>>,
        #[serde(default)]
        caption: Vec<Inline>,
        #[serde(default, rename = "headerRows")]
        header_rows: usize,
        label: Option<String>,
    },
    Footnote {
        id: String,
        paragraphs: Vec<Vec<Inline>>,
    },
    Bibliography,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Inline {
    Text {
        value: String,
        mark: Option<String>,
        direction: Option<String>,
        reference: Option<Reference>,
    },
    Math {
        value: String,
    },
    Link {
        value: String,
        url: String,
    },
    Citation {
        cluster: serde_json::Value,
        text: Option<String>,
    },
    Break,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Reference {
    pub kind: String,
    pub target: String,
}
#[derive(Debug, Deserialize)]
pub struct Bibliography {
    pub id: String,
    pub label: String,
    pub text: String,
}
#[derive(Debug, Deserialize)]
pub struct Asset {
    pub source: String,
    pub path: PathBuf,
}
#[derive(Debug, Deserialize)]
pub struct FontSpec {
    pub path: PathBuf,
    pub role: String,
    #[serde(default)]
    pub index: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub ok: bool,
    pub compiler_version: &'static str,
    pub pages: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub timings: std::collections::BTreeMap<&'static str, u128>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_index: Option<usize>,
}
impl Diagnostic {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            block_index: None,
        }
    }
    pub fn at(mut self, index: usize) -> Self {
        self.block_index = Some(index);
        self
    }
}
pub type Result<T> = std::result::Result<T, Diagnostic>;
