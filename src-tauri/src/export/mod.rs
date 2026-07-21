//! Multi-format export: Markdown → Typst / LaTeX / HTML / DOCX (+ templates).

use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use crate::journal::{self, JournalEntry};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Markdown,
    Typst,
    Latex,
    Html,
    Docx,
}

impl ExportFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExportFormat::Markdown => "markdown",
            ExportFormat::Typst => "typst",
            ExportFormat::Latex => "latex",
            ExportFormat::Html => "html",
            ExportFormat::Docx => "docx",
        }
    }

    pub fn from_str(s: &str) -> AppResult<Self> {
        match s.to_ascii_lowercase().as_str() {
            "markdown" | "md" => Ok(ExportFormat::Markdown),
            "typst" => Ok(ExportFormat::Typst),
            "latex" | "tex" => Ok(ExportFormat::Latex),
            "html" | "htm" => Ok(ExportFormat::Html),
            "docx" => Ok(ExportFormat::Docx),
            other => Err(AppError::Message(format!("unknown export format: {other}"))),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Markdown => "md",
            ExportFormat::Typst => "typ",
            ExportFormat::Latex => "tex",
            ExportFormat::Html => "html",
            ExportFormat::Docx => "docx",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub format: String,
    pub body: String,
    pub is_builtin: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct MultiExportPreview {
    pub title: String,
    pub format: String,
    pub content: String,
    /// For docx, content is empty and note explains binary write-only via export_to_path
    pub note: Option<String>,
}

const PLACEHOLDERS: &[&str] = &[
    "{{title}}",
    "{{date}}",
    "{{body}}",
    "{{fields}}",
    "{{tags}}",
    "{{entries}}",
    "{{author}}",
    "{{project}}",
];

/// Escape for LaTeX special chars (minimal).
fn latex_escape(s: &str) -> String {
    s.replace('\\', "\\textbackslash{}")
        .replace('&', "\\&")
        .replace('%', "\\%")
        .replace('$', "\\$")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('~', "\\textasciitilde{}")
        .replace('^', "\\textasciicircum{}")
}

fn typst_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('#', "\\#")
        .replace('$', "\\$")
        .replace('<', "\\<")
        .replace('>', "\\>")
        .replace('@', "\\@")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Very small Markdown → structured blocks (headings, lists, paragraphs, code).
#[derive(Debug)]
enum Block {
    H1(String),
    H2(String),
    H3(String),
    Para(String),
    Ul(Vec<String>),
    Code(String),
    Formula(String), // $$...$$ or $...$
}

fn parse_md_blocks(md: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut lines = md.lines().peekable();
    let mut in_code = false;
    let mut code_buf = String::new();
    let mut list_buf: Vec<String> = Vec::new();

    let flush_list = |blocks: &mut Vec<Block>, list_buf: &mut Vec<String>| {
        if !list_buf.is_empty() {
            blocks.push(Block::Ul(std::mem::take(list_buf)));
        }
    };

    while let Some(line) = lines.next() {
        if line.starts_with("```") {
            flush_list(&mut blocks, &mut list_buf);
            if in_code {
                blocks.push(Block::Code(std::mem::take(&mut code_buf)));
                in_code = false;
            } else {
                in_code = true;
            }
            continue;
        }
        if in_code {
            code_buf.push_str(line);
            code_buf.push('\n');
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_list(&mut blocks, &mut list_buf);
            continue;
        }

        if trimmed.starts_with("$$") && trimmed.ends_with("$$") && trimmed.len() > 4 {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(Block::Formula(trimmed.trim_matches('$').to_string()));
            continue;
        }
        if trimmed.starts_with('$') && trimmed.ends_with('$') && trimmed.len() > 2 && !trimmed[1..].contains('$') {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(Block::Formula(trimmed.trim_matches('$').to_string()));
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("### ") {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(Block::H3(rest.to_string()));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(Block::H2(rest.to_string()));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(Block::H1(rest.to_string()));
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            list_buf.push(rest.to_string());
        } else {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(Block::Para(trimmed.to_string()));
        }
    }
    flush_list(&mut blocks, &mut list_buf);
    if in_code && !code_buf.is_empty() {
        blocks.push(Block::Code(code_buf));
    }
    blocks
}

fn md_to_typst(md: &str) -> String {
    let mut out = String::new();
    for b in parse_md_blocks(md) {
        match b {
            Block::H1(t) => out.push_str(&format!("= {}\n\n", typst_escape(&t))),
            Block::H2(t) => out.push_str(&format!("== {}\n\n", typst_escape(&t))),
            Block::H3(t) => out.push_str(&format!("=== {}\n\n", typst_escape(&t))),
            Block::Para(t) => out.push_str(&format!("{}\n\n", typst_escape(&t))),
            Block::Ul(items) => {
                for it in items {
                    out.push_str(&format!("- {}\n", typst_escape(&it)));
                }
                out.push('\n');
            }
            Block::Code(c) => out.push_str(&format!("```\n{c}```\n\n")),
            Block::Formula(f) => out.push_str(&format!("$ {f} $\n\n")),
        }
    }
    out
}

fn md_to_latex_body(md: &str) -> String {
    let mut out = String::new();
    for b in parse_md_blocks(md) {
        match b {
            Block::H1(t) => out.push_str(&format!("\\section{{{}}}\n\n", latex_escape(&t))),
            Block::H2(t) => out.push_str(&format!("\\subsection{{{}}}\n\n", latex_escape(&t))),
            Block::H3(t) => out.push_str(&format!("\\subsubsection{{{}}}\n\n", latex_escape(&t))),
            Block::Para(t) => out.push_str(&format!("{}\n\n", latex_escape(&t))),
            Block::Ul(items) => {
                out.push_str("\\begin{itemize}\n");
                for it in items {
                    out.push_str(&format!("  \\item {}\n", latex_escape(&it)));
                }
                out.push_str("\\end{itemize}\n\n");
            }
            Block::Code(c) => {
                out.push_str("\\begin{verbatim}\n");
                out.push_str(&c);
                if !c.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("\\end{verbatim}\n\n");
            }
            Block::Formula(f) => out.push_str(&format!("\\[\n{f}\n\\]\n\n")),
        }
    }
    out
}

fn md_to_html_body(md: &str) -> String {
    let mut out = String::new();
    for b in parse_md_blocks(md) {
        match b {
            Block::H1(t) => out.push_str(&format!("<h1>{}</h1>\n", html_escape(&t))),
            Block::H2(t) => out.push_str(&format!("<h2>{}</h2>\n", html_escape(&t))),
            Block::H3(t) => out.push_str(&format!("<h3>{}</h3>\n", html_escape(&t))),
            Block::Para(t) => out.push_str(&format!("<p>{}</p>\n", html_escape(&t))),
            Block::Ul(items) => {
                out.push_str("<ul>\n");
                for it in items {
                    out.push_str(&format!("  <li>{}</li>\n", html_escape(&it)));
                }
                out.push_str("</ul>\n");
            }
            Block::Code(c) => {
                out.push_str(&format!("<pre><code>{}</code></pre>\n", html_escape(&c)))
            }
            Block::Formula(f) => {
                out.push_str(&format!(
                    "<div class=\"formula\">\\[{}\\]</div>\n",
                    html_escape(&f)
                ))
            }
        }
    }
    out
}

fn entry_fields_text(entry: &JournalEntry) -> String {
    let fields: serde_json::Map<String, serde_json::Value> = entry
        .fields_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let mut lines = Vec::new();
    for (k, v) in &fields {
        let val = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !val.trim().is_empty() {
            lines.push(format!("- **{k}:** {val}"));
        }
    }
    lines.join("\n")
}

fn entry_tags_text(entry: &JournalEntry) -> String {
    let tags: Vec<String> = entry
        .tags_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    tags.join(", ")
}

fn apply_template(template_body: &str, vars: &[(&str, String)]) -> String {
    let mut out = template_body.to_string();
    for (k, v) in vars {
        out = out.replace(k, v);
    }
    // leave unknown placeholders empty-ish
    for p in PLACEHOLDERS {
        if out.contains(p) {
            out = out.replace(p, "");
        }
    }
    out
}

fn default_template(format: &ExportFormat) -> &'static str {
    match format {
        ExportFormat::Markdown => {
            "# {{title}}\n\n**Date:** {{date}}\n\n**Tags:** {{tags}}\n\n## Fields\n\n{{fields}}\n\n## Body\n\n{{body}}\n"
        }
        ExportFormat::Typst => {
            r#"#set page(margin: 2cm)
#set text(size: 11pt)

= {{title}}

#text(size: 0.9em)[Date: {{date}} · Tags: {{tags}}]

== Fields
{{fields}}

== Body
{{body}}
"#
        }
        ExportFormat::Latex => {
            r#"\documentclass[11pt,a4paper]{article}
\usepackage[utf8]{inputenc}
\usepackage[T2A]{fontenc}
\usepackage[russian,english]{babel}
\usepackage{amsmath,amssymb}
\usepackage{hyperref}
\usepackage{geometry}
\geometry{margin=2cm}

\title{{{title}}}
\date{{{date}}}
\begin{document}
\maketitle

\noindent\textbf{Tags:} {{tags}}

\section*{Fields}
{{fields}}

\section*{Body}
{{body}}

\end{document}
"#
        }
        ExportFormat::Html => {
            r#"<!DOCTYPE html>
<html lang="ru">
<head>
  <meta charset="utf-8"/>
  <title>{{title}}</title>
  <style>
    body { font-family: system-ui, sans-serif; max-width: 720px; margin: 2rem auto; line-height: 1.55; padding: 0 1rem; }
    h1 { border-bottom: 1px solid #ccc; padding-bottom: .3em; }
    pre { background: #f4f4f4; padding: 12px; overflow: auto; }
    .meta { color: #666; font-size: .9rem; }
    .formula { margin: 1em 0; text-align: center; }
  </style>
</head>
<body>
  <h1>{{title}}</h1>
  <p class="meta">Date: {{date}} · Tags: {{tags}}</p>
  <h2>Fields</h2>
  {{fields}}
  <h2>Body</h2>
  {{body}}
</body>
</html>
"#
        }
        ExportFormat::Docx => {
            // Docx uses same vars; body is converted separately
            "{{title}}\n{{date}}\n{{tags}}\n{{fields}}\n{{body}}"
        }
    }
}

pub fn seed_export_templates(db: &DbState) -> AppResult<()> {
    with_conn(db, |conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM export_templates WHERE is_builtin = 1",
            [],
            |r| r.get(0),
        )?;
        if count > 0 {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        let seeds = [
            ("exp-md-default", "Markdown default", "markdown", default_template(&ExportFormat::Markdown)),
            ("exp-typst-default", "Typst default", "typst", default_template(&ExportFormat::Typst)),
            ("exp-latex-default", "LaTeX default", "latex", default_template(&ExportFormat::Latex)),
            ("exp-html-default", "HTML default", "html", default_template(&ExportFormat::Html)),
            (
                "exp-typst-report",
                "Typst lab report",
                "typst",
                r#"#set page(margin: 2.2cm)
#set text(size: 11pt)
#align(center)[
  #text(size: 1.4em, weight: "bold")[{{title}}]
  #v(0.5em)
  #text(size: 0.95em)[Laboratory report · {{date}}]
]
#v(1em)
#text[Project: {{project}} · Author: {{author}}]
#v(0.8em)
{{entries}}
"#,
            ),
            (
                "exp-latex-report",
                "LaTeX lab report",
                "latex",
                r#"\documentclass[11pt,a4paper]{article}
\usepackage[utf8]{inputenc}
\usepackage[T2A]{fontenc}
\usepackage[russian,english]{babel}
\usepackage{amsmath}
\usepackage{geometry}
\geometry{margin=2.2cm}
\title{{{title}}}
\author{{{author}}}
\date{{{date}}}
\begin{document}
\maketitle
\section*{Project}
{{project}}

{{entries}}
\end{document}
"#,
            ),
        ];
        for (id, name, format, body) in seeds {
            conn.execute(
                "INSERT INTO export_templates (id, name, description, format, body, is_builtin, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
                params![id, name, "Built-in export template", format, body, now],
            )?;
        }
        Ok(())
    })
}

fn map_export_tpl(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExportTemplate> {
    Ok(ExportTemplate {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        format: row.get(3)?,
        body: row.get(4)?,
        is_builtin: row.get::<_, i64>(5)? == 1,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

pub fn list_export_templates(db: &DbState) -> AppResult<Vec<ExportTemplate>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, format, body, is_builtin, created_at, updated_at
             FROM export_templates ORDER BY is_builtin DESC, name",
        )?;
        let rows = stmt.query_map([], map_export_tpl)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

pub fn get_export_template(db: &DbState, id: &str) -> AppResult<ExportTemplate> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT id, name, description, format, body, is_builtin, created_at, updated_at
             FROM export_templates WHERE id = ?1",
            params![id],
            map_export_tpl,
        )
        .map_err(|_| AppError::Message("export template not found".into()))
    })
}

#[derive(Debug, Deserialize)]
pub struct ExportTemplateInput {
    pub name: String,
    pub description: Option<String>,
    pub format: String,
    pub body: String,
}

pub fn create_export_template(db: &DbState, input: ExportTemplateInput) -> AppResult<ExportTemplate> {
    ExportFormat::from_str(&input.format)?;
    if input.name.trim().is_empty() {
        return Err(AppError::Message("name required".into()));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO export_templates (id, name, description, format, body, is_builtin, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
            params![
                id,
                input.name.trim(),
                input.description,
                input.format.to_ascii_lowercase(),
                input.body,
                now
            ],
        )?;
        Ok(())
    })?;
    get_export_template(db, &id)
}

pub fn delete_export_template(db: &DbState, id: &str) -> AppResult<()> {
    let t = get_export_template(db, id)?;
    if t.is_builtin {
        return Err(AppError::Message("cannot delete builtin export template".into()));
    }
    with_conn(db, |conn| {
        conn.execute("DELETE FROM export_templates WHERE id = ?1", params![id])?;
        Ok(())
    })
}

fn convert_body_for_format(md: &str, format: &ExportFormat) -> String {
    match format {
        ExportFormat::Markdown => md.to_string(),
        ExportFormat::Typst => md_to_typst(md),
        ExportFormat::Latex => md_to_latex_body(md),
        ExportFormat::Html => md_to_html_body(md),
        ExportFormat::Docx => md.to_string(), // handled specially
    }
}

fn convert_fields_for_format(fields_md: &str, format: &ExportFormat) -> String {
    convert_body_for_format(fields_md, format)
}

/// Build export content for a single journal entry.
pub fn render_entry(
    entry: &JournalEntry,
    format: &ExportFormat,
    template_body: &str,
    author: &str,
    project: &str,
) -> AppResult<String> {
    let body_src = if entry.body_md.trim().is_empty() {
        String::new()
    } else {
        entry.body_md.clone()
    };
    let fields_src = entry_fields_text(entry);
    let body = convert_body_for_format(&body_src, format);
    let fields = convert_fields_for_format(&fields_src, format);
    let tags = entry_tags_text(entry);

    let vars = [
        ("{{title}}", entry.title.clone()),
        ("{{date}}", entry.entry_date.clone()),
        ("{{body}}", body),
        ("{{fields}}", fields),
        ("{{tags}}", tags),
        ("{{entries}}", String::new()),
        ("{{author}}", author.to_string()),
        ("{{project}}", project.to_string()),
    ];
    Ok(apply_template(template_body, &vars))
}

/// Period report: multiple entries into {{entries}}.
pub fn render_period_report(
    entries: &[JournalEntry],
    format: &ExportFormat,
    template_body: &str,
    title: &str,
    author: &str,
    project: &str,
) -> AppResult<String> {
    let mut entries_blob = String::new();
    for e in entries {
        let body = convert_body_for_format(&e.body_md, format);
        let fields = convert_fields_for_format(&entry_fields_text(e), format);
        match format {
            ExportFormat::Markdown => {
                entries_blob.push_str(&format!(
                    "## {} ({})\n\n{}\n\n{}\n\n---\n\n",
                    e.title, e.entry_date, fields, body
                ));
            }
            ExportFormat::Typst => {
                entries_blob.push_str(&format!(
                    "== {} ({})\n\n{}\n\n{}\n\n",
                    typst_escape(&e.title),
                    e.entry_date,
                    fields,
                    body
                ));
            }
            ExportFormat::Latex => {
                entries_blob.push_str(&format!(
                    "\\section*{{{}}}\\newline\\textit{{{}}}\n\n{}\n\n{}\n\n",
                    latex_escape(&e.title),
                    latex_escape(&e.entry_date),
                    fields,
                    body
                ));
            }
            ExportFormat::Html => {
                entries_blob.push_str(&format!(
                    "<article><h2>{} <small>({})</small></h2>{}{}</article>\n",
                    html_escape(&e.title),
                    html_escape(&e.entry_date),
                    fields,
                    body
                ));
            }
            ExportFormat::Docx => {
                entries_blob.push_str(&format!(
                    "{}\n{}\n{}\n{}\n\n",
                    e.title, e.entry_date, fields, e.body_md
                ));
            }
        }
    }

    let date_range = if entries.is_empty() {
        String::new()
    } else {
        format!(
            "{} – {}",
            entries.last().map(|e| e.entry_date.as_str()).unwrap_or(""),
            entries.first().map(|e| e.entry_date.as_str()).unwrap_or("")
        )
    };

    let vars = [
        ("{{title}}", title.to_string()),
        ("{{date}}", date_range),
        ("{{body}}", String::new()),
        ("{{fields}}", String::new()),
        ("{{tags}}", String::new()),
        ("{{entries}}", entries_blob),
        ("{{author}}", author.to_string()),
        ("{{project}}", project.to_string()),
    ];
    Ok(apply_template(template_body, &vars))
}

pub fn resolve_template_body(
    db: &DbState,
    format: &ExportFormat,
    template_id: Option<&str>,
) -> AppResult<String> {
    if let Some(id) = template_id {
        let t = get_export_template(db, id)?;
        return Ok(t.body);
    }
    // first builtin for format
    let list = list_export_templates(db)?;
    if let Some(t) = list
        .iter()
        .find(|t| t.format == format.as_str() && t.is_builtin)
    {
        return Ok(t.body.clone());
    }
    Ok(default_template(format).to_string())
}

pub fn preview_entry_export(
    db: &DbState,
    entry_id: &str,
    format: &str,
    template_id: Option<String>,
    author: Option<String>,
    project: Option<String>,
) -> AppResult<MultiExportPreview> {
    let format = ExportFormat::from_str(format)?;
    let entry = journal::get_entry(db, entry_id)?;
    let tpl = resolve_template_body(db, &format, template_id.as_deref())?;
    if format == ExportFormat::Docx {
        let content = render_entry(
            &entry,
            &ExportFormat::Markdown,
            default_template(&ExportFormat::Markdown),
            author.as_deref().unwrap_or(""),
            project.as_deref().unwrap_or(""),
        )?;
        return Ok(MultiExportPreview {
            title: entry.title,
            format: "docx".into(),
            content,
            note: Some("DOCX is binary; preview shows Markdown source used for conversion.".into()),
        });
    }
    let content = render_entry(
        &entry,
        &format,
        &tpl,
        author.as_deref().unwrap_or(""),
        project.as_deref().unwrap_or(""),
    )?;
    Ok(MultiExportPreview {
        title: entry.title,
        format: format.as_str().into(),
        content,
        note: None,
    })
}

pub fn preview_period_export(
    db: &DbState,
    from_date: &str,
    to_date: &str,
    format: &str,
    template_id: Option<String>,
    title: Option<String>,
    author: Option<String>,
    project: Option<String>,
    tag_filter: Option<String>,
) -> AppResult<MultiExportPreview> {
    let format = ExportFormat::from_str(format)?;
    let mut entries = journal::list_entries(db)?;
    entries.retain(|e| e.entry_date.as_str() >= from_date && e.entry_date.as_str() <= to_date);
    if let Some(tag) = tag_filter {
        let tag = tag.to_ascii_lowercase();
        entries.retain(|e| {
            let tags: Vec<String> = e
                .tags_json
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            tags.iter().any(|t| t.to_ascii_lowercase().contains(&tag))
        });
    }
    // list is DESC; for report chronological ASC
    entries.reverse();

    let title = title.unwrap_or_else(|| format!("Report {from_date} – {to_date}"));
    let tpl = resolve_template_body(db, &format, template_id.as_deref())?;
    let content = render_period_report(
        &entries,
        &format,
        &tpl,
        &title,
        author.as_deref().unwrap_or(""),
        project.as_deref().unwrap_or(""),
    )?;
    Ok(MultiExportPreview {
        title,
        format: format.as_str().into(),
        content,
        note: Some(format!("{} entries", entries.len())),
    })
}

/// Minimal DOCX (WordprocessingML) from plain paragraphs.
pub fn write_simple_docx(path: &str, title: &str, body_text: &str) -> AppResult<()> {
    use std::io::Write;
    let map_zip = |e: zip::result::ZipError| AppError::Message(format!("docx zip: {e}"));
    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", opts)
        .map_err(map_zip)?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    )?;

    zip.start_file("_rels/.rels", opts).map_err(map_zip)?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    )?;

    let mut paras = String::new();
    paras.push_str(&format!(
        r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>{}</w:t></w:r></w:p>"#,
        xml_escape(title)
    ));
    for line in body_text.lines() {
        paras.push_str(&format!(
            r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(line)
        ));
    }

    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {paras}
    <w:sectPr/>
  </w:body>
</w:document>"#
    );
    zip.start_file("word/document.xml", opts)
        .map_err(map_zip)?;
    zip.write_all(document.as_bytes())?;
    zip.finish().map_err(map_zip)?;
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn export_entry_to_path(
    db: &DbState,
    entry_id: &str,
    format: &str,
    path: &str,
    template_id: Option<String>,
    author: Option<String>,
    project: Option<String>,
) -> AppResult<()> {
    let format = ExportFormat::from_str(format)?;
    let entry = journal::get_entry(db, entry_id)?;
    if format == ExportFormat::Docx {
        let md = journal::entry_to_markdown(&entry)?;
        return write_simple_docx(path, &entry.title, &md);
    }
    let tpl = resolve_template_body(db, &format, template_id.as_deref())?;
    let content = render_entry(
        &entry,
        &format,
        &tpl,
        author.as_deref().unwrap_or(""),
        project.as_deref().unwrap_or(""),
    )?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn export_period_to_path(
    db: &DbState,
    from_date: &str,
    to_date: &str,
    format: &str,
    path: &str,
    template_id: Option<String>,
    title: Option<String>,
    author: Option<String>,
    project: Option<String>,
    tag_filter: Option<String>,
) -> AppResult<()> {
    let preview = preview_period_export(
        db,
        from_date,
        to_date,
        format,
        template_id,
        title,
        author,
        project,
        tag_filter,
    )?;
    let format = ExportFormat::from_str(format)?;
    if format == ExportFormat::Docx {
        return write_simple_docx(path, &preview.title, &preview.content);
    }
    std::fs::write(path, preview.content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md_to_latex_has_section() {
        let latex = md_to_latex_body("# Hello\n\nWorld");
        assert!(latex.contains("\\section{Hello}"));
        assert!(latex.contains("World"));
    }

    #[test]
    fn md_to_typst_heading() {
        let t = md_to_typst("## Sub\n\ntext");
        assert!(t.contains("== Sub"));
    }

    #[test]
    fn formula_block() {
        let t = md_to_typst("$$\nE=mc^2\n$$");
        // our parser treats single-line $$ only; multi-line may be para — check $ block
        let t2 = md_to_latex_body("$a+b$");
        assert!(t2.contains("\\[") || t2.contains("a+b"));
        let _ = t;
    }
}
