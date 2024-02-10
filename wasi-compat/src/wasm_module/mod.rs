pub(crate) mod varint;

use crate::error::*;

use log::*;
use std::fmt::{self, Write as _};
use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};
use std::path::Path;
use std::str;

pub type Header = [u8; 8];

const WASM_HEADER: Header = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
const WASM_HEADER_COMPONENT: Header = [0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00];

/// A section identifier.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum SectionId {
    CustomSection,
    Type,
    Import,
    Function,
    Table,
    Memory,
    Global,
    Export,
    Start,
    Element,
    Code,
    Data,
    Extension(u8),
}

impl From<u8> for SectionId {
    fn from(v: u8) -> Self {
        match v {
            0 => SectionId::CustomSection,
            1 => SectionId::Type,
            2 => SectionId::Import,
            3 => SectionId::Function,
            4 => SectionId::Table,
            5 => SectionId::Memory,
            6 => SectionId::Global,
            7 => SectionId::Export,
            8 => SectionId::Start,
            9 => SectionId::Element,
            10 => SectionId::Code,
            11 => SectionId::Data,
            x => SectionId::Extension(x),
        }
    }
}

impl From<SectionId> for u8 {
    fn from(v: SectionId) -> Self {
        match v {
            SectionId::CustomSection => 0,
            SectionId::Type => 1,
            SectionId::Import => 2,
            SectionId::Function => 3,
            SectionId::Table => 4,
            SectionId::Memory => 5,
            SectionId::Global => 6,
            SectionId::Export => 7,
            SectionId::Start => 8,
            SectionId::Element => 9,
            SectionId::Code => 10,
            SectionId::Data => 11,
            SectionId::Extension(x) => x,
        }
    }
}

impl fmt::Display for SectionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SectionId::CustomSection => write!(f, "custom section"),
            SectionId::Type => write!(f, "types section"),
            SectionId::Import => write!(f, "imports section"),
            SectionId::Function => write!(f, "functions section"),
            SectionId::Table => write!(f, "table section"),
            SectionId::Memory => write!(f, "memory section"),
            SectionId::Global => write!(f, "global section"),
            SectionId::Export => write!(f, "exports section"),
            SectionId::Start => write!(f, "start section"),
            SectionId::Element => write!(f, "elements section"),
            SectionId::Code => write!(f, "code section"),
            SectionId::Data => write!(f, "data section"),
            SectionId::Extension(x) => write!(f, "section id#{x}"),
        }
    }
}

/// Common functions for a module section.
pub trait SectionLike {
    fn id(&self) -> SectionId;
    fn payload(&self) -> &[u8];
    fn display(&self, verbose: bool) -> String;
}

/// A standard section.
#[derive(Debug, Clone)]
pub struct StandardSection {
    id: SectionId,
    payload: Vec<u8>,
}

impl StandardSection {
    /// Create a new standard section.
    pub fn new(id: SectionId, payload: Vec<u8>) -> Self {
        Self { id, payload }
    }
}

impl SectionLike for StandardSection {
    /// Return the identifier of the section.
    fn id(&self) -> SectionId {
        self.id
    }

    /// Return the payload of the section.
    fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Human-readable representation of the section.
    fn display(&self, _verbose: bool) -> String {
        self.id().to_string()
    }
}

/// A custom section.
#[derive(Debug, Clone, Default)]
pub struct CustomSection {
    name: String,
    payload: Vec<u8>,
}

impl CustomSection {
    /// Create a new custom section.
    pub fn new(name: String, payload: Vec<u8>) -> Self {
        Self { name, payload }
    }

    /// Return the name of the custom section.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the custom section as an array of bytes.
    ///
    /// This includes the data itself, but also the size and name of the custom section.
    pub fn outer_payload(&self) -> Result<Vec<u8>, WSError> {
        let mut writer = io::Cursor::new(vec![]);
        varint::put(&mut writer, self.name.len() as _)?;
        writer.write_all(self.name.as_bytes())?;
        writer.write_all(&self.payload)?;
        Ok(writer.into_inner())
    }
}

impl SectionLike for CustomSection {
    fn id(&self) -> SectionId {
        SectionId::CustomSection
    }

    fn payload(&self) -> &[u8] {
        &self.payload
    }

    fn display(&self, verbose: bool) -> String {
        if !verbose {
            return format!("custom section: [{}]", self.name());
        }

        match self.name() {
            _ => format!("custom section: [{}]", self.name()),
        }
    }
}

/// A WebAssembly module section.
///
/// It is recommended to import the `SectionLike` trait for additional functions.
#[derive(Clone)]
pub enum Section {
    /// A standard section.
    Standard(StandardSection),
    /// A custom section.
    Custom(CustomSection),
}

impl SectionLike for Section {
    fn id(&self) -> SectionId {
        match self {
            Section::Standard(s) => s.id(),
            Section::Custom(s) => s.id(),
        }
    }

    fn payload(&self) -> &[u8] {
        match self {
            Section::Standard(s) => s.payload(),
            Section::Custom(s) => s.payload(),
        }
    }

    fn display(&self, verbose: bool) -> String {
        match self {
            Section::Standard(s) => s.display(verbose),
            Section::Custom(s) => s.display(verbose),
        }
    }
}

impl Section {
    /// Create a new section with the given identifier and payload.
    pub fn new(id: SectionId, payload: Vec<u8>) -> Result<Self, WSError> {
        match id {
            SectionId::CustomSection => {
                let mut reader = io::Cursor::new(payload);
                let name_len = varint::get32(&mut reader)? as usize;
                let mut name_slice = vec![0u8; name_len];
                reader.read_exact(&mut name_slice)?;
                let name = str::from_utf8(&name_slice)?.to_string();
                let mut payload = Vec::new();
                let len = reader.read_to_end(&mut payload)?;
                payload.truncate(len);
                Ok(Section::Custom(CustomSection::new(name, payload)))
            }
            _ => Ok(Section::Standard(StandardSection::new(id, payload))),
        }
    }

    /// Create a section from its standard serialized representation.
    pub fn deserialize(reader: &mut impl Read) -> Result<Option<Self>, WSError> {
        let id = match varint::get7(reader) {
            Ok(id) => SectionId::from(id),
            Err(WSError::Eof) => return Ok(None),
            Err(e) => return Err(e),
        };
        let len = varint::get32(reader)? as usize;
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload)?;
        let section = Section::new(id, payload)?;
        Ok(Some(section))
    }

    /// Serialize a section.
    pub fn serialize(&self, writer: &mut impl Write) -> Result<(), WSError> {
        let outer_payload;
        let payload = match self {
            Section::Standard(s) => s.payload(),
            Section::Custom(s) => {
                outer_payload = s.outer_payload()?;
                &outer_payload
            }
        };
        varint::put(writer, u8::from(self.id()) as _)?;
        varint::put(writer, payload.len() as _)?;
        writer.write_all(payload)?;
        Ok(())
    }
}

impl CustomSection {}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.display(false))
    }
}

impl fmt::Debug for Section {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.display(true))
    }
}

/// A WebAssembly module.
#[derive(Debug, Clone, Default)]
pub struct Module {
    pub header: Header,
    pub sections: Vec<Section>,
}

impl Module {
    /// Deserialize a WebAssembly module from the given reader.
    pub fn deserialize(reader: &mut impl Read) -> Result<Self, WSError> {
        let header = Self::stream_init(reader)?;
        let it = Self::stream(reader)?;
        let mut sections = Vec::new();
        for section in it {
            sections.push(section?);
        }
        Ok(Module { header, sections })
    }

    /// Deserialize a WebAssembly module from the given file.
    pub fn deserialize_from_file(file: impl AsRef<Path>) -> Result<Self, WSError> {
        let fp = File::open(file.as_ref())?;
        Self::deserialize(&mut BufReader::new(fp))
    }

    /// Serialize a WebAssembly module to the given writer.
    pub fn serialize(&self, writer: &mut impl Write) -> Result<(), WSError> {
        writer.write_all(&self.header)?;
        for section in &self.sections {
            section.serialize(writer)?;
        }
        Ok(())
    }

    /// Serialize a WebAssembly module to the given file.
    pub fn serialize_to_file(&self, file: impl AsRef<Path>) -> Result<(), WSError> {
        let fp = File::create(file.as_ref())?;
        self.serialize(&mut BufWriter::new(fp))
    }

    /// Parse the module's header. That function must be called before `stream`.
    pub fn stream_init<T: Read>(reader: &mut T) -> Result<Header, WSError> {
        let mut header = Header::default();
        reader.read_exact(&mut header)?;
        if header == WASM_HEADER {
            return Ok(header);
        }
        if header == WASM_HEADER_COMPONENT {
            debug!("Found a WASI component");
            return Ok(header);
        }
        Err(WSError::UnsupportedModuleType)
    }

    /// Return an iterator over the sections of a WebAssembly module.    
    ///
    /// The module is read in a streaming fashion, and doesn't have to be fully loaded into memory.
    pub fn stream<T: Read>(reader: &mut T) -> Result<SectionsIterator<T>, WSError> {
        Ok(SectionsIterator { reader })
    }
}

/// An iterator over the sections of a WebAssembly module.
pub struct SectionsIterator<'t, T: Read> {
    reader: &'t mut T,
}

impl<'t, T: Read> Iterator for SectionsIterator<'t, T> {
    type Item = Result<Section, WSError>;

    fn next(&mut self) -> Option<Self::Item> {
        match Section::deserialize(self.reader) {
            Err(e) => Some(Err(e)),
            Ok(None) => None,
            Ok(Some(section)) => Some(Ok(section)),
        }
    }
}
