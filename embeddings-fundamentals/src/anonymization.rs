//! Helper for the anonymization review (PRD §11, PLAN §4.4).
//!
//! It reports what a reviewer must inspect: identifier-like filenames, PDF information
//! dictionary keys and XMP element names, image EXIF tags, image XMP element names,
//! JPEG IPTC (APP13) blocks, and PNG text-chunk keywords. A file that cannot be parsed
//! produces a finding asking for a manual review, never a silent "clean". It only lists
//! keys, never values, and it never decides that a source is anonymized: only the
//! reviewer's checks in the manifest make a source eligible.

use std::collections::BTreeSet;
use std::path::Path;

use regex::Regex;
use serde::Serialize;

/// Something a reviewer has to look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Where the finding is: `filename`, `pdf-info`, `pdf-xmp`, `exif`, `xmp`, `iptc`,
    /// `png-text`, or `parse`.
    pub area: &'static str,
    /// Human-readable description (keys or patterns, never the values).
    pub detail: String,
}

/// PDF information-dictionary keys that commonly carry identifying text.
const RISKY_PDF_KEYS: &[&str] = &["Author", "Title", "Subject", "Keywords", "Creator"];

fn finding(area: &'static str, detail: impl Into<String>) -> Finding {
    Finding {
        area,
        detail: detail.into(),
    }
}

/// Flags path components that look like identifiers (CPF, dates, two capitalized
/// words, two or more consecutive upper-case words such as `MARIA DA SILVA`), and any
/// free-form name: anything beyond lower-case ASCII letters, digits, `-`, `_`, and `.`
/// (spaces, capitals, accents), where a name can hide in a form no pattern anticipates.
///
/// Every component of `file` is checked, directories included, because a manifest
/// records the whole relative path (Grok G4-6: `MARIA SILVA/lab-001.pdf`). Pass the
/// path as the manifest records it, relative to the manifest, not an absolute path.
/// The match itself is never reported. A finding asks a reviewer to look; it never
/// decides.
#[must_use]
pub fn filename_findings(file: &Path) -> Vec<Finding> {
    let components: Vec<_> = file
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let last = components.len().saturating_sub(1);
    components
        .iter()
        .enumerate()
        .flat_map(|(i, name)| {
            let what = if i == last {
                "filename"
            } else {
                "directory name"
            };
            component_findings(name, what)
        })
        .collect()
}

/// The findings of one path component (`what` is `filename` or `directory name`).
fn component_findings(name: &str, what: &str) -> Vec<Finding> {
    let patterns: [(&str, &str); 4] = [
        (r"\d{3}\.?\d{3}\.?\d{3}-?\d{2}", "CPF-like number"),
        (
            r"\d{2}[/._-]\d{2}[/._-]\d{4}|\d{4}-\d{2}-\d{2}",
            "date-like sequence",
        ),
        (
            r"\b[A-ZÁÉÍÓÚÂÊÔÃÕÇ][a-záéíóúâêôãõç]+[ _-][A-ZÁÉÍÓÚÂÊÔÃÕÇ][a-záéíóúâêôãõç]+\b",
            "capitalized name pair",
        ),
        (
            r"(?:^|[^\p{L}])\p{Lu}{2,}(?:[ _.-]+\p{Lu}+)*[ _.-]+\p{Lu}{2,}(?:$|[^\p{L}])",
            "upper-case word sequence (possible name)",
        ),
    ];
    let mut findings: Vec<Finding> = patterns
        .iter()
        .filter(|(pattern, _)| Regex::new(pattern).is_ok_and(|re| re.is_match(name)))
        .map(|(_, label)| finding("filename", format!("{what} contains a {label}")))
        .collect();
    let slug = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'));
    if !slug {
        findings.push(finding(
            "filename",
            format!("free-form {what} (spaces, capitals, or accents) may hold a name"),
        ));
    }
    findings
}

/// Element and attribute names (`prefix:name`) inside an XMP packet, excluding the
/// packet scaffolding and namespace declarations. Values are never collected.
fn xmp_keys(packet: &str) -> BTreeSet<String> {
    let scaffolding = [
        "x:xmpmeta",
        "x:xmptk",
        "rdf:RDF",
        "rdf:Description",
        "rdf:Seq",
        "rdf:Bag",
        "rdf:Alt",
        "rdf:li",
        "rdf:about",
        "rdf:parseType",
        "xml:lang",
    ];
    let mut keys = BTreeSet::new();
    for pattern in [
        r"<([A-Za-z][\w.-]*:[A-Za-z][\w.-]*)",
        r"\s([A-Za-z][\w.-]*:[A-Za-z][\w.-]*)\s*=",
    ] {
        if let Ok(re) = Regex::new(pattern) {
            keys.extend(
                re.captures_iter(packet)
                    .filter_map(|c| c.get(1).map(|m| m.as_str().to_owned()))
                    .filter(|k| !scaffolding.contains(&k.as_str()) && !k.starts_with("xmlns:")),
            );
        }
    }
    keys
}

/// IPTC IIM dataset names found in a JPEG APP13 block (`record:dataset`, with common
/// names). Only the dataset tags are read, never their values.
fn iptc_datasets(bytes: &[u8]) -> BTreeSet<String> {
    let names: [((u8, u8), &str); 12] = [
        ((2, 5), "ObjectName"),
        ((2, 25), "Keywords"),
        ((2, 55), "DateCreated"),
        ((2, 80), "By-line"),
        ((2, 85), "By-lineTitle"),
        ((2, 90), "City"),
        ((2, 95), "Province-State"),
        ((2, 101), "Country"),
        ((2, 110), "Credit"),
        ((2, 115), "Source"),
        ((2, 116), "CopyrightNotice"),
        ((2, 120), "Caption-Abstract"),
    ];
    let mut found = BTreeSet::new();
    let Some(start) = find(bytes, b"Photoshop 3.0\0") else {
        return found;
    };
    let block = &bytes[start..];
    let mut i = 0;
    while i + 5 <= block.len() {
        // IIM dataset: 0x1C, record (1..=9), dataset, 2-byte big-endian length.
        if block[i] == 0x1C && (1..=9).contains(&block[i + 1]) {
            let (record, dataset) = (block[i + 1], block[i + 2]);
            let len = usize::from(u16::from_be_bytes([block[i + 3], block[i + 4]]));
            if i + 5 + len <= block.len() {
                let name = names
                    .iter()
                    .find(|(key, _)| *key == (record, dataset))
                    .map_or_else(
                        || format!("{record}:{dataset}"),
                        |(_, n)| format!("{record}:{dataset} {n}"),
                    );
                found.insert(name);
                i += 5 + len;
                continue;
            }
        }
        i += 1;
    }
    found
}

fn xmp_finding(area: &'static str, packet: &str) -> Finding {
    let keys = xmp_keys(packet);
    if keys.is_empty() {
        finding(area, "XMP packet present (no element names found)")
    } else {
        finding(
            area,
            format!(
                "XMP keys present: {}",
                keys.into_iter().collect::<Vec<_>>().join(", ")
            ),
        )
    }
}

/// Lists risky PDF information keys and the element names of the catalog's XMP stream.
#[must_use]
pub fn pdf_findings(path: &Path) -> Vec<Finding> {
    let doc = match lopdf::Document::load(path) {
        Ok(doc) => doc,
        Err(e) => {
            return vec![finding(
                "parse",
                format!("PDF cannot be parsed ({e}); review manually"),
            )];
        }
    };
    let mut findings = Vec::new();
    if let Ok(info) = doc
        .trailer
        .get(b"Info")
        .and_then(|o| doc.dereference(o))
        .and_then(|(_, o)| o.as_dict())
    {
        for key in RISKY_PDF_KEYS {
            if info.has(key.as_bytes()) {
                findings.push(finding(
                    "pdf-info",
                    format!("information dictionary has /{key}"),
                ));
            }
        }
    }
    if let Ok(catalog) = doc.catalog()
        && let Ok(metadata) = catalog.get(b"Metadata")
    {
        let packet = doc
            .dereference(metadata)
            .ok()
            .and_then(|(_, o)| o.as_stream().ok())
            .and_then(|stream| {
                stream
                    .decompressed_content()
                    .ok()
                    .or_else(|| Some(stream.content.clone()))
            });
        match packet {
            Some(bytes) => findings.push(xmp_finding("pdf-xmp", &String::from_utf8_lossy(&bytes))),
            None => findings.push(finding(
                "parse",
                "XMP /Metadata stream cannot be read; review manually",
            )),
        }
    }
    findings
}

/// Byte offset of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// PNG `tEXt`/`zTXt`/`iTXt` keywords.
fn png_text_keywords(bytes: &[u8]) -> Vec<String> {
    let mut keywords = Vec::new();
    let mut pos = 8; // PNG signature
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        let Ok(len) = usize::try_from(len) else { break };
        let kind = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        let Some(data_end) = data_start
            .checked_add(len)
            .filter(|end| *end <= bytes.len())
        else {
            break;
        };
        if matches!(kind, b"tEXt" | b"zTXt" | b"iTXt") {
            let data = &bytes[data_start..data_end];
            let keyword_end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
            keywords.push(String::from_utf8_lossy(&data[..keyword_end]).into_owned());
        }
        pos = data_end + 4; // skip CRC
    }
    keywords
}

/// Lists image metadata: EXIF tags, XMP element names, IPTC blocks, PNG text keywords.
#[must_use]
pub fn image_findings(path: &Path) -> Vec<Finding> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            return vec![finding(
                "parse",
                format!("image cannot be read ({e}); review manually"),
            )];
        }
    };
    let mut findings = Vec::new();
    match exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&bytes)) {
        Ok(exif) => {
            let tags: Vec<String> = exif.fields().map(|f| f.tag.to_string()).collect();
            if !tags.is_empty() {
                findings.push(finding(
                    "exif",
                    format!("EXIF tags present: {}", tags.join(", ")),
                ));
            }
        }
        Err(exif::Error::NotFound(_)) => {}
        Err(e) => findings.push(finding(
            "parse",
            format!("EXIF cannot be parsed ({e}); review manually"),
        )),
    }
    if let Some(start) =
        find(&bytes, b"<x:xmpmeta").or_else(|| find(&bytes, b"http://ns.adobe.com/xap/1.0/"))
    {
        let end = find(&bytes[start..], b"</x:xmpmeta>").map_or(bytes.len(), |e| start + e);
        findings.push(xmp_finding(
            "xmp",
            &String::from_utf8_lossy(&bytes[start..end]),
        ));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) && find(&bytes, b"Photoshop 3.0\0").is_some() {
        let datasets = iptc_datasets(&bytes);
        findings.push(if datasets.is_empty() {
            finding(
                "iptc",
                "JPEG APP13 (IPTC/Photoshop) block present; no IIM datasets parsed",
            )
        } else {
            finding(
                "iptc",
                format!(
                    "IPTC datasets present: {}",
                    datasets.into_iter().collect::<Vec<_>>().join(", ")
                ),
            )
        });
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let keywords = png_text_keywords(&bytes);
        if !keywords.is_empty() {
            findings.push(finding(
                "png-text",
                format!("PNG text keywords: {}", keywords.join(", ")),
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus(file: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/corpus")
            .join(file)
    }

    #[test]
    fn identifier_like_filenames_are_flagged() {
        let labels = |name: &str| -> Vec<String> {
            filename_findings(Path::new(name))
                .into_iter()
                .map(|f| f.detail)
                .collect()
        };
        assert_eq!(labels("123.456.789-09.pdf").len(), 1);
        assert_eq!(labels("laudo-12-03-2024.pdf").len(), 1);
        let pair = labels("Maria Silva.pdf");
        assert!(
            pair.iter().any(|d| d.contains("capitalized name pair")),
            "{pair:?}"
        );
        assert!(pair.iter().any(|d| d.contains("free-form")), "{pair:?}");
        assert!(filename_findings(Path::new("lab-searchable-001.pdf")).is_empty());
        assert!(filename_findings(Path::new("labs/2024/lab-searchable-001.pdf")).is_empty());
        // Regression (Grok G4-6): a directory component is checked like the file name.
        let nested = filename_findings(Path::new("sub/FULANO DE TAL/lab-001.pdf"));
        assert!(
            nested
                .iter()
                .any(|f| f.detail.starts_with("directory name contains")
                    && f.detail.contains("upper-case word sequence")),
            "{nested:?}"
        );
        assert!(nested.iter().all(|f| !f.detail.contains("FULANO")));
        // Regression (real sample exams): names in upper case, with accents and
        // particles, were not flagged at all.
        for name in [
            "MARIA DA SILVA - LAUDO.pdf",
            "JOÃO P. SOUZA - RELATORIO.pdf",
            "ANA_LIMA_exame.pdf",
            "EXAME E CONTROLE.pdf",
        ] {
            let findings = filename_findings(Path::new(name));
            assert!(
                findings
                    .iter()
                    .any(|f| f.detail.contains("upper-case word sequence")),
                "{name}: {findings:?}"
            );
            assert!(
                findings
                    .iter()
                    .all(|f| !f.detail.contains("MARIA") && !f.detail.contains("SOUZA")),
                "the match itself is never reported"
            );
        }
        // A single upper-case token (an acronym) is not a name sequence, but any capital
        // makes the name free-form.
        assert_eq!(
            labels("RX-001.png"),
            ["free-form filename (spaces, capitals, or accents) may hold a name"]
        );
        // Lower-case words with spaces or accents are free-form too.
        assert_eq!(labels("maria silva.pdf").len(), 1);
        assert_eq!(labels("joão-silva.pdf").len(), 1);
    }

    #[test]
    fn synthetic_fixtures_have_no_risky_metadata() {
        assert_eq!(pdf_findings(&corpus("lab-searchable-001.pdf")), vec![]);
        assert_eq!(image_findings(&corpus("chest-001.png")), vec![]);
        assert_eq!(image_findings(&corpus("abd-us-001.jpg")), vec![]);
    }

    #[test]
    fn jpeg_xmp_and_iptc_are_reported_by_key() {
        let original = std::fs::read(corpus("abd-us-001.jpg")).expect("jpeg");
        let segment = |marker: u8, payload: &[u8]| -> Vec<u8> {
            let len = u16::try_from(payload.len() + 2).expect("len").to_be_bytes();
            [&[0xFF, marker][..], &len, payload].concat()
        };
        let xmp = segment(
            0xE1,
            b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta><rdf:RDF><rdf:Description><dc:creator>X</dc:creator></rdf:Description></rdf:RDF></x:xmpmeta>",
        );
        // 8BIM resource 0x0404 holding one IIM dataset 2:80 (By-line) with value "Fulano".
        let iptc = segment(
            0xED,
            b"Photoshop 3.0\08BIM\x04\x04\0\0\0\0\0\x0b\x1c\x02\x50\x00\x06Fulano",
        );
        let tagged = [&original[..2], &xmp, &iptc, &original[2..]].concat();
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("tagged.jpg");
        std::fs::write(&path, tagged).expect("write");
        let findings = image_findings(&path);
        assert!(
            findings
                .iter()
                .any(|f| f.area == "xmp" && f.detail.contains("dc:creator")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.area == "iptc" && f.detail.contains("2:80 By-line")),
            "{findings:?}"
        );
        assert!(
            findings.iter().all(|f| !f.detail.contains("Fulano")),
            "IPTC values are never reported"
        );
        assert!(
            findings.iter().all(|f| !f.detail.contains(">X<")),
            "values are never reported"
        );
    }

    #[test]
    fn attribute_form_xmp_keys_are_listed_without_values() {
        let packet = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" dc:creator="Fulano" photoshop:City="Recife"/></rdf:RDF></x:xmpmeta>"#;
        let keys: Vec<String> = xmp_keys(packet).into_iter().collect();
        assert_eq!(
            keys,
            vec!["dc:creator".to_owned(), "photoshop:City".to_owned()]
        );
        let f = xmp_finding("xmp", packet);
        assert!(
            !f.detail.contains("Fulano") && !f.detail.contains("Recife"),
            "{f:?}"
        );
    }

    #[test]
    fn png_text_chunks_are_reported_by_keyword() {
        let original = std::fs::read(corpus("chest-001.png")).expect("png");
        let data = b"Author\0Fulano";
        let mut chunk = u32::try_from(data.len())
            .expect("len")
            .to_be_bytes()
            .to_vec();
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&[0, 0, 0, 0]); // CRC is not checked by the scanner
        let tagged = [&original[..33], &chunk, &original[33..]].concat(); // after IHDR
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("tagged.png");
        std::fs::write(&path, tagged).expect("write");
        let findings = image_findings(&path);
        assert!(
            findings
                .iter()
                .any(|f| f.area == "png-text" && f.detail.contains("Author")),
            "{findings:?}"
        );
        assert!(
            findings.iter().all(|f| !f.detail.contains("Fulano")),
            "values are never reported"
        );
    }

    #[test]
    fn unparsable_files_ask_for_manual_review() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("broken.pdf");
        std::fs::write(&path, b"%PDF-1.5 garbage").expect("write");
        assert!(pdf_findings(&path).iter().any(|f| f.area == "parse"));
        let jpg = dir.path().join("broken.jpg");
        std::fs::write(
            &jpg,
            [
                0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x10, b'E', b'x', b'i', b'f', 0, 0, 1, 2, 3,
            ],
        )
        .expect("write");
        assert!(
            image_findings(&jpg).iter().any(|f| f.area == "parse"),
            "{:?}",
            image_findings(&jpg)
        );
    }

    #[test]
    fn pdf_xmp_keys_and_info_keys_are_listed() {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut doc = Document::with_version("1.5");
        let xmp = doc.add_object(Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            b"<x:xmpmeta><rdf:RDF><rdf:Description><dc:creator>Y</dc:creator><pdf:Producer>p</pdf:Producer></rdf:Description></rdf:RDF></x:xmpmeta>".to_vec(),
        ));
        let pages = doc.add_object(
            dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 },
        );
        let catalog = doc
            .add_object(dictionary! { "Type" => "Catalog", "Pages" => pages, "Metadata" => xmp });
        doc.trailer.set("Root", catalog);
        let info = doc.add_object(dictionary! { "Author" => Object::string_literal("Y") });
        doc.trailer.set("Info", info);
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("meta.pdf");
        doc.save(&path).expect("save");
        let findings = pdf_findings(&path);
        assert!(
            findings
                .iter()
                .any(|f| f.area == "pdf-info" && f.detail.contains("/Author")),
            "{findings:?}"
        );
        assert!(
            findings.iter().any(|f| f.area == "pdf-xmp"
                && f.detail.contains("dc:creator")
                && f.detail.contains("pdf:Producer")),
            "{findings:?}"
        );
    }
}
