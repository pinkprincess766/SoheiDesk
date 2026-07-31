//! External literature: DOI/Crossref, arXiv, PubMed + bibliography formats.

use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioItem {
    pub id: String,
    pub source: String,
    pub external_id: Option<String>,
    pub title: String,
    pub authors: Option<String>,
    pub year: Option<String>,
    pub journal: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub bibtex: Option<String>,
    pub data_json: Option<String>,
    pub document_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteratureHit {
    pub source: String,
    pub external_id: String,
    pub title: String,
    pub authors: String,
    pub year: Option<String>,
    pub journal: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub abstract_text: Option<String>,
    pub bibtex: Option<String>,
}

fn client() -> AppResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent("SoheiDesk/0.3 (research desktop; local)")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Message(format!("http client: {e}")))
}

/// Resolve DOI via Crossref.
pub fn resolve_doi(doi: &str) -> AppResult<LiteratureHit> {
    let doi = doi.trim().trim_start_matches("https://doi.org/");
    let doi = doi.trim_start_matches("http://doi.org/");
    let url = format!(
        "https://api.crossref.org/works/{}",
        urlencoding::encode(doi)
    );
    let client = client()?;
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| AppError::Message(format!("Crossref request failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Message(format!(
            "Crossref HTTP {}",
            resp.status()
        )));
    }
    let v: serde_json::Value = resp
        .json()
        .map_err(|e| AppError::Message(format!("Crossref JSON: {e}")))?;
    let msg = &v["message"];
    let title = msg["title"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|t| t.as_str())
        .unwrap_or("(untitled)")
        .to_string();
    let authors = msg["author"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let given = a["given"].as_str().unwrap_or("");
                    let family = a["family"].as_str().unwrap_or("");
                    if family.is_empty() && given.is_empty() {
                        None
                    } else if given.is_empty() {
                        Some(family.to_string())
                    } else {
                        Some(format!("{family}, {given}"))
                    }
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    let year = msg["published-print"]["date-parts"]
        .as_array()
        .or_else(|| msg["published-online"]["date-parts"].as_array())
        .or_else(|| msg["created"]["date-parts"].as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|y| y.as_i64())
        .map(|y| y.to_string());
    let journal = msg["container-title"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let doi_str = msg["DOI"].as_str().unwrap_or(doi).to_string();
    let url = format!("https://doi.org/{doi_str}");
    let bibtex = Some(to_bibtex(
        "crossref",
        &doi_str,
        &title,
        &authors,
        year.as_deref(),
        journal.as_deref(),
        Some(&doi_str),
        Some(&url),
    ));

    Ok(LiteratureHit {
        source: "doi".into(),
        external_id: doi_str.clone(),
        title,
        authors,
        year,
        journal,
        doi: Some(doi_str),
        url: Some(url),
        abstract_text: msg["abstract"].as_str().map(strip_jats),
        bibtex,
    })
}

fn strip_jats(s: &str) -> String {
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(s, "").to_string()
}

/// Search arXiv Atom API.
pub fn search_arxiv(query: &str, max_results: usize) -> AppResult<Vec<LiteratureHit>> {
    let q = urlencoding::encode(query);
    let n = max_results.clamp(1, 25);
    let url =
        format!("http://export.arxiv.org/api/query?search_query=all:{q}&start=0&max_results={n}");
    let client = client()?;
    let text = client
        .get(&url)
        .send()
        .map_err(|e| AppError::Message(format!("arXiv request: {e}")))?
        .text()
        .map_err(|e| AppError::Message(format!("arXiv body: {e}")))?;

    parse_arxiv_atom(&text)
}

fn parse_arxiv_atom(xml: &str) -> AppResult<Vec<LiteratureHit>> {
    // lightweight split by <entry>
    let mut hits = Vec::new();
    for part in xml.split("<entry>").skip(1) {
        let entry = part.split("</entry>").next().unwrap_or("");
        let id = extract_tag(entry, "id").unwrap_or_default();
        let title = extract_tag(entry, "title")
            .unwrap_or_else(|| "(untitled)".into())
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let summary = extract_tag(entry, "summary")
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "));
        let published = extract_tag(entry, "published");
        let year = published
            .as_ref()
            .and_then(|p| p.get(0..4).map(|s| s.to_string()));
        let mut authors = Vec::new();
        for a in entry.split("<author>").skip(1) {
            if let Some(name) = extract_tag(a, "name") {
                authors.push(name);
            }
        }
        let arxiv_id = id
            .rsplit('/')
            .next()
            .unwrap_or(&id)
            .replace("abs/", "")
            .trim()
            .to_string();
        let pdf_url = format!("https://arxiv.org/pdf/{arxiv_id}.pdf");
        let abs_url = format!("https://arxiv.org/abs/{arxiv_id}");
        let authors_s = authors.join("; ");
        let bibtex = Some(to_bibtex(
            "arxiv",
            &arxiv_id,
            &title,
            &authors_s,
            year.as_deref(),
            Some("arXiv"),
            None,
            Some(&abs_url),
        ));
        hits.push(LiteratureHit {
            source: "arxiv".into(),
            external_id: arxiv_id,
            title,
            authors: authors_s,
            year,
            journal: Some("arXiv".into()),
            doi: None,
            url: Some(pdf_url),
            abstract_text: summary,
            bibtex,
        });
    }
    Ok(hits)
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = xml.find(&open)?;
    let after = &xml[start..];
    let gt = after.find('>')?;
    let content_start = gt + 1;
    let rest = &after[content_start..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim().to_string())
}

/// PubMed esearch + esummary (minimal).
pub fn search_pubmed(query: &str, max_results: usize) -> AppResult<Vec<LiteratureHit>> {
    let q = urlencoding::encode(query);
    let n = max_results.clamp(1, 25);
    let client = client()?;
    let search_url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&retmode=json&retmax={n}&term={q}"
    );
    let search_v: serde_json::Value = client
        .get(&search_url)
        .send()
        .map_err(|e| AppError::Message(format!("PubMed search: {e}")))?
        .json()
        .map_err(|e| AppError::Message(format!("PubMed search JSON: {e}")))?;
    let ids: Vec<String> = search_v["esearchresult"]["idlist"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let id_list = ids.join(",");
    let sum_url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&retmode=json&id={id_list}"
    );
    let sum_v: serde_json::Value = client
        .get(&sum_url)
        .send()
        .map_err(|e| AppError::Message(format!("PubMed summary: {e}")))?
        .json()
        .map_err(|e| AppError::Message(format!("PubMed summary JSON: {e}")))?;

    let mut hits = Vec::new();
    let result = &sum_v["result"];
    for id in &ids {
        let item = &result[id];
        if item.is_null() {
            continue;
        }
        let title = item["title"].as_str().unwrap_or("(untitled)").to_string();
        let authors = item["authors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x["name"].as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        let year = item["pubdate"]
            .as_str()
            .and_then(|d| d.get(0..4).map(|s| s.to_string()));
        let journal = item["fulljournalname"]
            .as_str()
            .or_else(|| item["source"].as_str())
            .map(|s| s.to_string());
        let doi = item["elocationid"]
            .as_str()
            .and_then(|s| {
                if s.to_ascii_lowercase().contains("doi") {
                    s.split_whitespace().last().map(|x| x.to_string())
                } else {
                    None
                }
            })
            .or_else(|| {
                item["articleids"].as_array().and_then(|arr| {
                    arr.iter().find_map(|a| {
                        if a["idtype"].as_str() == Some("doi") {
                            a["value"].as_str().map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                })
            });
        let url = format!("https://pubmed.ncbi.nlm.nih.gov/{id}/");
        let bibtex = Some(to_bibtex(
            "pubmed",
            id,
            &title,
            &authors,
            year.as_deref(),
            journal.as_deref(),
            doi.as_deref(),
            Some(&url),
        ));
        hits.push(LiteratureHit {
            source: "pubmed".into(),
            external_id: id.clone(),
            title,
            authors,
            year,
            journal,
            doi,
            url: Some(url),
            abstract_text: None,
            bibtex,
        });
    }
    Ok(hits)
}

#[allow(clippy::too_many_arguments)]
pub fn to_bibtex(
    entry_type_prefix: &str,
    key_base: &str,
    title: &str,
    authors: &str,
    year: Option<&str>,
    journal: Option<&str>,
    doi: Option<&str>,
    url: Option<&str>,
) -> String {
    let key = format!(
        "{}_{}",
        entry_type_prefix,
        key_base
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
    );
    let mut out = format!("@article{{{key},\n");
    out.push_str(&format!("  title = {{{title}}},\n"));
    if !authors.is_empty() {
        let bib_authors = authors.replace(';', " and ");
        out.push_str(&format!("  author = {{{bib_authors}}},\n"));
    }
    if let Some(y) = year {
        out.push_str(&format!("  year = {{{y}}},\n"));
    }
    if let Some(j) = journal {
        out.push_str(&format!("  journal = {{{j}}},\n"));
    }
    if let Some(d) = doi {
        out.push_str(&format!("  doi = {{{d}}},\n"));
    }
    if let Some(u) = url {
        out.push_str(&format!("  url = {{{u}}},\n"));
    }
    out.push_str("}\n");
    out
}

/// Citation styles.
pub fn format_citation(item: &BiblioItem, style: &str) -> String {
    let authors = item.authors.clone().unwrap_or_default();
    let year = item.year.clone().unwrap_or_else(|| "n.d.".into());
    let title = &item.title;
    let journal = item.journal.clone().unwrap_or_default();
    let doi = item.doi.clone().unwrap_or_default();

    match style.to_ascii_lowercase().as_str() {
        "bibtex" => item.bibtex.clone().unwrap_or_else(|| {
            to_bibtex(
                "item",
                &item.id[..8.min(item.id.len())],
                title,
                &authors,
                item.year.as_deref(),
                item.journal.as_deref(),
                item.doi.as_deref(),
                item.url.as_deref(),
            )
        }),
        "ris" => {
            let mut r = String::from("TY  - JOUR\n");
            r.push_str(&format!("TI  - {title}\n"));
            for a in authors.split(';') {
                let a = a.trim();
                if !a.is_empty() {
                    r.push_str(&format!("AU  - {a}\n"));
                }
            }
            r.push_str(&format!("PY  - {year}\n"));
            if !journal.is_empty() {
                r.push_str(&format!("JO  - {journal}\n"));
            }
            if !doi.is_empty() {
                r.push_str(&format!("DO  - {doi}\n"));
            }
            if let Some(u) = &item.url {
                r.push_str(&format!("UR  - {u}\n"));
            }
            r.push_str("ER  - \n");
            r
        }
        "apa" => {
            // Author, A. A. (Year). Title. Journal. https://doi.org/...
            format!(
                "{authors} ({year}). {title}. {journal}{}{}",
                if journal.is_empty() { "" } else { "." },
                if doi.is_empty() {
                    String::new()
                } else {
                    format!(" https://doi.org/{doi}")
                }
            )
        }
        "gost" | "gost-r" => {
            // Simplified GOST-like: Authors Title // Journal. — Year.
            format!(
                "{authors} {title} // {journal}. — {year}.{}",
                if doi.is_empty() {
                    String::new()
                } else {
                    format!(" — DOI: {doi}.")
                }
            )
        }
        other => format!("({other} unsupported) {authors} ({year}). {title}"),
    }
}

fn map_biblio(row: &rusqlite::Row<'_>) -> rusqlite::Result<BiblioItem> {
    Ok(BiblioItem {
        id: row.get(0)?,
        source: row.get(1)?,
        external_id: row.get(2)?,
        title: row.get(3)?,
        authors: row.get(4)?,
        year: row.get(5)?,
        journal: row.get(6)?,
        doi: row.get(7)?,
        url: row.get(8)?,
        bibtex: row.get(9)?,
        data_json: row.get(10)?,
        document_id: row.get(11)?,
        created_at: row.get(12)?,
    })
}

pub fn list_biblio(db: &DbState) -> AppResult<Vec<BiblioItem>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, source, external_id, title, authors, year, journal, doi, url, bibtex, data_json, document_id, created_at
             FROM bibliography_items ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], map_biblio)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

pub fn save_hit(db: &DbState, hit: &LiteratureHit) -> AppResult<BiblioItem> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let data = serde_json::to_string(hit)?;
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO bibliography_items
             (id, source, external_id, title, authors, year, journal, doi, url, bibtex, data_json, document_id, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,NULL,?12)",
            params![
                id,
                hit.source,
                hit.external_id,
                hit.title,
                hit.authors,
                hit.year,
                hit.journal,
                hit.doi,
                hit.url,
                hit.bibtex,
                data,
                now
            ],
        )?;
        Ok(())
    })?;
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT id, source, external_id, title, authors, year, journal, doi, url, bibtex, data_json, document_id, created_at
             FROM bibliography_items WHERE id = ?1",
            params![id],
            map_biblio,
        )
        .map_err(|e| AppError::Message(format!("biblio load: {e}")))
    })
}

pub fn delete_biblio(db: &DbState, id: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        conn.execute("DELETE FROM bibliography_items WHERE id = ?1", params![id])?;
        Ok(())
    })
}

pub fn export_bibliography(db: &DbState, style: &str) -> AppResult<String> {
    let items = list_biblio(db)?;
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if style == "bibtex" || style == "ris" {
            out.push_str(&format_citation(item, style));
            if !out.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(&format!("{}. {}\n\n", i + 1, format_citation(item, style)));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bibtex_contains_title() {
        let b = to_bibtex(
            "t",
            "key1",
            "My Title",
            "Doe, J",
            Some("2020"),
            Some("Nature"),
            Some("10.1/x"),
            None,
        );
        assert!(b.contains("My Title"));
        assert!(b.contains("@article"));
    }

    #[test]
    fn extract_tag_works() {
        let xml = "<title>Hello</title>";
        assert_eq!(extract_tag(xml, "title").unwrap(), "Hello");
    }
}
