//! The OPC container: opening, inspecting and saving a `.docx` without losing
//! anything.
//!
//! ## Scope of this step
//!
//! This is the *container*, not WordprocessingML. It reads the zip, keeps every
//! part verbatim, resolves the main document through the relationship graph, and
//! writes the package back out unchanged. It does not model paragraphs, styles or
//! runs — that is [ADR-0005](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0005-ooxml-lossless-layer.md)'s
//! next step.
//!
//! What it establishes is the property everything else depends on: **a document
//! that is opened and saved with no edits comes back byte-identical.** Without
//! that, every later feature is built on a foundation that quietly reformats the
//! user's file.
//!
//! ## Why parts are stored as raw bytes
//!
//! A part is kept as it arrived and written back as it arrived. Parts are not
//! parsed on open, not normalised, and not round-tripped through a model. The
//! only exception is a part the caller explicitly asks to replace.
//!
//! The alternative — parse everything on open — is the obvious design and the
//! wrong one. It means a bug in the reader can damage a document the user merely
//! *opened*, which is the worst possible time.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use crate::ooxml::tree::{Node, ParseError};

/// An error opening or saving a package.
#[derive(Debug)]
pub enum OpcError {
    Io(std::io::Error),
    Zip(String),
    /// The zip is fine but it is not an OOXML package.
    NotAPackage(String),
    /// No main document could be resolved through the relationship graph.
    NoMainDocument(String),
    /// A part's XML could not be parsed.
    Xml {
        part: String,
        source: ParseError,
    },
}

impl fmt::Display for OpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpcError::Io(e) => write!(f, "i/o error: {e}"),
            OpcError::Zip(m) => write!(f, "not a readable zip package: {m}"),
            OpcError::NotAPackage(m) => write!(f, "not an OOXML package: {m}"),
            OpcError::NoMainDocument(m) => {
                write!(f, "no main document relationship found: {m}")
            }
            OpcError::Xml { part, source } => write!(f, "invalid XML in {part}: {source}"),
        }
    }
}

impl std::error::Error for OpcError {}

impl From<std::io::Error> for OpcError {
    fn from(e: std::io::Error) -> Self {
        OpcError::Io(e)
    }
}

/// One part of the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    name: String,
    bytes: Vec<u8>,
}

impl Part {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Parse this part as XML.
    ///
    /// The bytes are untouched; parsing produces a separate tree. Callers that
    /// want to keep their edits must `set_part_bytes` explicitly.
    pub fn parse_xml(&self) -> Result<Node, OpcError> {
        Node::parse(&self.bytes).map_err(|source| OpcError::Xml {
            part: self.name.clone(),
            source,
        })
    }
}

/// Relationship type for the main document part.
const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";

/// An OOXML package.
#[derive(Debug, Clone)]
pub struct Package {
    /// Parts in name order. A `BTreeMap` so output is deterministic — a package
    /// that reorders its own entries on every save would defeat byte-identity.
    parts: BTreeMap<String, Part>,
    /// Name of the part holding the main document, resolved on open.
    main_document: Option<String>,
}

impl Package {
    /// Open a package from disk.
    pub fn open(path: &Path) -> Result<Self, OpcError> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    /// Open a package from memory.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, OpcError> {
        let mut file = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| OpcError::Zip(e.to_string()))?;

        let mut parts = BTreeMap::new();
        for i in 0..file.len() {
            let mut entry = file.by_index(i).map_err(|e| OpcError::Zip(e.to_string()))?;
            let name = entry.name().to_string();

            // Directory entries carry no content and would otherwise appear as
            // empty parts.
            if name.ends_with('/') {
                continue;
            }

            let mut data = Vec::with_capacity(entry.size() as usize);
            std::io::Read::read_to_end(&mut entry, &mut data)?;

            parts.insert(name.clone(), Part { name, bytes: data });
        }

        if !parts.contains_key("[Content_Types].xml") {
            return Err(OpcError::NotAPackage(
                "missing [Content_Types].xml, which every OOXML package must have".into(),
            ));
        }

        let mut pkg = Package {
            parts,
            main_document: None,
        };
        pkg.main_document = pkg.resolve_main_document()?;
        Ok(pkg)
    }

    /// Find the main document by walking the root relationships.
    ///
    /// Hardcoding `word/document.xml` is a classic bug: it works for files Word
    /// wrote and nothing else. A document produced by another tool can put the
    /// main part anywhere, and the root relationship names it.
    fn resolve_main_document(&self) -> Result<Option<String>, OpcError> {
        let Some(rels) = self.parts.get("_rels/.rels") else {
            return Err(OpcError::NoMainDocument(
                "package has no _rels/.rels, so it declares no relationships".into(),
            ));
        };

        let tree = Node::parse(&rels.bytes).map_err(|source| OpcError::Xml {
            part: "_rels/.rels".into(),
            source,
        })?;

        let mut rels_found: Vec<&Node> = Vec::new();
        tree.find_all("Relationship", &mut rels_found);

        for rel in rels_found {
            let Node::Element(rel) = rel else {
                continue;
            };
            if rel.attr("Type") == Some(REL_OFFICE_DOCUMENT) {
                let target = rel.attr("Target").unwrap_or_default();
                if target.is_empty() {
                    continue;
                }
                // External targets are URLs, not parts.
                if target.contains("://") {
                    continue;
                }
                let resolved = target.trim_start_matches('/').to_string();
                if self.parts.contains_key(&resolved) {
                    return Ok(Some(resolved));
                }
            }
        }

        Err(OpcError::NoMainDocument(format!(
            "no relationship of type {REL_OFFICE_DOCUMENT} points at a part in this package"
        )))
    }

    /// Every part, in name order.
    pub fn parts(&self) -> Vec<&Part> {
        self.parts.values().collect()
    }

    /// Look up a part by name.
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts.get(name)
    }

    /// Raw bytes of a part.
    pub fn part_bytes(&self, name: &str) -> Option<&[u8]> {
        self.parts.get(name).map(|p| p.bytes.as_slice())
    }

    /// Replace a part's contents.
    ///
    /// The one mutating operation in this type, and it is explicit on purpose:
    /// nothing in the reader can change a document by accident.
    pub fn set_part_bytes(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), OpcError> {
        let part = self
            .parts
            .get_mut(name)
            .ok_or_else(|| OpcError::NotAPackage(format!("no such part: {name}")))?;
        part.bytes = bytes;
        Ok(())
    }

    /// The main document's part name, if the package has one.
    pub fn main_document_part(&self) -> Option<&str> {
        self.main_document.as_deref()
    }

    /// Save to a path.
    ///
    /// Writes every part verbatim, in name order, with no re-compression of
    /// untouched parts and no added metadata. That is what makes the
    /// round-trip tests meaningful.
    pub fn save(&self, path: &Path) -> Result<(), OpcError> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Serialise to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, OpcError> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            for part in self.parts.values() {
                writer
                    .start_file(&part.name, options)
                    .map_err(|e| OpcError::Zip(e.to_string()))?;
                std::io::Write::write_all(&mut writer, &part.bytes)?;
            }
            writer.finish().map_err(|e| OpcError::Zip(e.to_string()))?;
        }
        Ok(buf)
    }
}
