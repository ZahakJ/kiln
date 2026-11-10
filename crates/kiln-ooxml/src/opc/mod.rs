// SPDX-License-Identifier: AGPL-3.0-or-later
//! Open Packaging Conventions (ECMA-376 Part 2): the zip container of `.docx`, `.xlsx` and
//! `.pptx` files, its part names, `[Content_Types].xml` and relationships.

mod content_types;
mod package;
mod part_name;
mod rel_types;
mod relationships;

pub use content_types::{
    ContentTypes, DEFAULT_EXTENSION_TYPES, content_type, default_content_type_for_extension,
    is_xml_content_type,
};
pub use package::{CONTENT_TYPES_PART, OpcLimits, OpcPackage, RelationshipOptions, SaveOptions};
pub use part_name::{
    PartNameError, basename_of, directory_of, extension_of, is_rels_part, normalize_part_name,
    part_key, part_name_to_zip_name, percent_decode, relative_target, rels_part_name,
    resolve_target, same_part, source_part_of_rels, validate_part_name, zip_name_to_part_name,
};
pub use rel_types::{rel_type, rel_type_matches, to_transitional_rel_type};
pub use relationships::{Relationship, Relationships};

use crate::xml::{XmlParseError, XmlWriteError};

/// Why a package cannot be opened, read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpcError {
    /// The file is an OLE compound document: a password-protected (encrypted) Office file or a
    /// legacy binary `.doc`/`.xls`/`.ppt`.
    #[error(
        "this file is password-protected or uses a legacy binary Office format (OLE compound file)"
    )]
    EncryptedOrLegacy,
    /// The bytes are not a zip archive.
    #[error("not a zip package")]
    NotZip,
    /// The archive exceeds the limits it is opened with.
    #[error("refused a suspicious package: {0}")]
    ZipBomb(String),
    /// The archive or one of its entries is damaged.
    #[error("corrupt zip: {0}")]
    CorruptZip(String),
    /// The package has no `[Content_Types].xml`.
    #[error("missing [Content_Types].xml")]
    MissingContentTypes,
    /// `[Content_Types].xml` is not readable.
    #[error("unreadable [Content_Types].xml: {0}")]
    ContentTypes(String),
    /// Two zip entries map to the same part name.
    #[error("duplicate part name {0}")]
    DuplicatePart(String),
    /// A part with this name already exists.
    #[error("part {0} already exists")]
    PartExists(String),
    /// No part with this name exists.
    #[error("part {0} does not exist")]
    NoSuchPart(String),
    /// `[Content_Types].xml` is managed by the package and cannot be set directly.
    #[error("[Content_Types].xml is managed by the package")]
    ReservedPartName,
    /// An invalid part name.
    #[error(transparent)]
    PartName(#[from] PartNameError),
    /// A `.rels` part is not readable.
    #[error("unreadable relationships for {source_part}: {message}")]
    Relationships {
        /// The source part.
        source_part: String,
        /// What went wrong.
        message: String,
    },
    /// A relationship id is already used on its source.
    #[error("relationship id {id} already exists on {source_part}")]
    DuplicateRelationshipId {
        /// The id.
        id: String,
        /// The source part.
        source_part: String,
    },
    /// A part is not well-formed XML.
    #[error("{part}: {error}")]
    Xml {
        /// The part.
        part: String,
        /// The parser's complaint.
        error: XmlParseError,
    },
    /// A tree could not be serialised.
    #[error(transparent)]
    XmlWrite(#[from] XmlWriteError),
    /// A naming pattern without `{n}`.
    #[error("part name pattern must contain {{n}}: {0}")]
    BadPattern(String),
    /// Writing the archive failed.
    #[error("cannot write the package: {0}")]
    Write(String),
}
