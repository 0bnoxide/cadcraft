//! The package ZIP: central-directory parsing, safety checks and bounded entry streams.
//!
//! Written from the PKWARE APPNOTE. The central directory is not trusted on its own: every
//! entry's local header must agree with it, and each entry's compressed data must exactly fill
//! the gap up to the next header. Entry streams enforce size limits while inflating and check the
//! declared size and CRC-32 at the end.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

use sha2::{Digest, Sha256};

use crate::{Issue, codes};

const MANIFEST_NAME: &str = "handoff.json";
const SURFACE_NAME: &str = "surface.landxml";
pub(crate) const MANIFEST_LIMIT: u64 = 1024 * 1024;
pub(crate) const SURFACE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const MAX_RATIO: u64 = 100;

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const DESCRIPTOR_SIG: u32 = 0x0807_4b50;
const LOCAL_LEN: u64 = 30;
const CENTRAL_LEN: usize = 46;
const EOCD_LEN: u64 = 22;
const ZIP64_LOCATOR_LEN: u64 = 20;
const ZIP64_EOCD_MIN_LEN: u64 = 56;
const FLAG_ENCRYPTED: u16 = 0x0001;
const FLAG_DESCRIPTOR: u16 = 0x0008;
const FLAG_UTF8: u16 = 0x0800;
const HOST_MSDOS: u8 = 0;
const HOST_NTFS: u8 = 10;
const DOS_NON_REGULAR: u32 = 0x0458;
const UNIX_TYPE_MASK: u32 = 0xf000;
const UNIX_REGULAR: u32 = 0x8000;
/// The contract allows two entries; anything far beyond that is hostile, not a package.
const MAX_ENTRIES: u64 = 4096;
/// Central directory size cap (4096 entries with generous names and extras).
const MAX_CENTRAL_BYTES: u64 = 64 * 1024 * 1024;

/// A reportable problem found while reading an entry's data, carried inside `io::Error`
/// so it passes through decoders and parsers unchanged.
#[derive(Debug)]
pub(crate) struct EntryFault(pub Issue);

impl fmt::Display for EntryFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.message)
    }
}

impl std::error::Error for EntryFault {}

fn fault(code: &'static str, message: impl Into<String>) -> io::Error {
    io::Error::other(EntryFault(Issue::error(code, message)))
}

/// The entry fault inside `e`, if `e` carries one.
pub(crate) fn fault_of(e: &io::Error) -> Option<Issue> {
    e.get_ref().and_then(|inner| inner.downcast_ref::<EntryFault>()).map(|f| f.0.clone())
}

/// Structural failure while opening: either the archive is invalid (ZIP001) or the file could
/// not be read.
enum OpenError {
    Invalid(&'static str),
    Io(io::Error),
}

impl From<io::Error> for OpenError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::UnexpectedEof { OpenError::Invalid("The ZIP archive is truncated.") } else { OpenError::Io(e) }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    version_needed: u16,
    host: u8,
    flags: u16,
    method: u16,
    crc: u32,
    compressed_size: u64,
    size: u64,
    offset: u64,
    external_attrs: u32,
    /// Start of the compressed data, set by the local-header preflight.
    data_offset: u64,
}

impl Entry {
    fn dos_attrs(&self, mask: u32) -> bool {
        (self.host == HOST_MSDOS || self.host == HOST_NTFS) && self.external_attrs & mask == mask
    }

    fn is_directory(&self) -> bool {
        self.name.ends_with('/') || self.name.ends_with('\\') || self.dos_attrs(0x10)
    }

    fn is_regular_file(&self) -> bool {
        if self.is_directory() {
            return false;
        }
        // Stricter than the reference validator, which checks MS-DOS hosts only: an NTFS-host
        // entry with the reparse-point or device attribute is not a regular file either.
        if (self.host == HOST_MSDOS || self.host == HOST_NTFS) && self.external_attrs & DOS_NON_REGULAR != 0 {
            return false;
        }
        let file_type = (self.external_attrs >> 16) & UNIX_TYPE_MASK;
        if file_type != 0 {
            return file_type == UNIX_REGULAR;
        }
        !self.dos_attrs(0x08)
    }

    fn exceeds_ratio(&self) -> bool {
        self.compressed_size.checked_mul(MAX_RATIO).is_some_and(|max| self.size > max)
    }

    fn needs_zip64(&self) -> bool {
        self.size >= u64::from(u32::MAX) || self.compressed_size >= u64::from(u32::MAX)
    }
}

/// An opened, structurally checked package.
pub(crate) struct Bundle<R> {
    source: R,
    manifest: Entry,
    surface: Entry,
}

impl<R: Read + Seek> Bundle<R> {
    /// Reads and checks the archive structure. `Ok(Err(issue))` is an invalid package.
    pub(crate) fn open(mut source: R) -> io::Result<Result<Self, Issue>> {
        let entries = match read_entries(&mut source) {
            Ok(entries) => entries,
            Err(OpenError::Invalid(why)) => {
                log::debug!("handoff ZIP rejected: {why}");
                return Ok(Err(Issue::error(codes::INVALID_ARCHIVE, "The ZIP archive cannot be read.")));
            }
            Err(OpenError::Io(e)) => return Err(e),
        };
        if let Some(issue) = validate_entries(&entries) {
            return Ok(Err(issue));
        }
        let (Some(manifest), Some(surface)) = (find(&entries, MANIFEST_NAME), find(&entries, SURFACE_NAME)) else {
            return Ok(Err(Issue::error(codes::MISSING_REQUIRED_ENTRY, "The ZIP is missing a required entry.")));
        };
        Ok(Ok(Self { source, manifest, surface }))
    }

    /// The manifest bytes (at most 1 MiB).
    pub(crate) fn read_manifest(&mut self) -> io::Result<Vec<u8>> {
        let entry = self.manifest.clone();
        let mut reader = EntryReader::open(&mut self.source, &entry, MANIFEST_LIMIT)?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Lower-case hex SHA-256 of the uncompressed surface.
    pub(crate) fn surface_sha256(&mut self) -> io::Result<String> {
        let entry = self.surface.clone();
        let mut reader = EntryReader::open(&mut self.source, &entry, SURFACE_LIMIT)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(buf.get(..n).unwrap_or_default());
        }
        Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// A fresh stream of the uncompressed surface.
    pub(crate) fn open_surface(&mut self) -> io::Result<EntryReader<'_>> {
        let entry = self.surface.clone();
        EntryReader::open(&mut self.source, &entry, SURFACE_LIMIT)
    }
}

fn find(entries: &[Entry], name: &str) -> Option<Entry> {
    entries.iter().find(|e| e.name == name).cloned()
}

fn validate_entries(entries: &[Entry]) -> Option<Issue> {
    let err = |code, message: &str| Some(Issue::error(code, message));
    if entries.iter().any(|e| !is_safe_name(&e.name)) {
        return err(codes::UNSAFE_ENTRY_NAME, "The ZIP contains an unsafe entry name.");
    }
    let mut folded: Vec<String> = entries.iter().map(|e| e.name.to_uppercase()).collect();
    folded.sort();
    folded.dedup();
    if folded.len() != entries.len() {
        return err(codes::DUPLICATE_ENTRY_NAME, "The ZIP contains duplicate entry names.");
    }
    if entries.iter().any(|e| e.name != MANIFEST_NAME && e.name != SURFACE_NAME) {
        return err(codes::UNEXPECTED_ENTRY, "The ZIP contains an unexpected entry.");
    }
    let (Some(manifest), Some(surface)) = (find(entries, MANIFEST_NAME), find(entries, SURFACE_NAME)) else {
        return err(codes::MISSING_REQUIRED_ENTRY, "The ZIP is missing a required entry.");
    };
    if entries.iter().any(|e| !e.is_regular_file()) {
        return err(codes::NON_REGULAR_ENTRY, "The ZIP contains a non-regular entry.");
    }
    if entries.iter().any(|e| e.flags & FLAG_ENCRYPTED != 0) {
        return err(codes::ENCRYPTED_ENTRY, "The ZIP contains an encrypted entry.");
    }
    if entries.iter().any(|e| e.method != 0 && e.method != 8) {
        return err(codes::UNSUPPORTED_COMPRESSION, "The ZIP uses an unsupported compression method.");
    }
    let too_big = i64::MAX.unsigned_abs();
    if entries.iter().any(|e| e.size > too_big || e.compressed_size > too_big) {
        return err(codes::INVALID_ARCHIVE, "The ZIP contains an entry with an invalid declared size.");
    }
    if manifest.size > MANIFEST_LIMIT {
        return err(codes::MANIFEST_TOO_LARGE, "The manifest exceeds the declared size limit.");
    }
    if surface.size > SURFACE_LIMIT {
        return err(codes::SURFACE_TOO_LARGE, "The surface exceeds the declared size limit.");
    }
    if manifest.exceeds_ratio() || surface.exceeds_ratio() {
        return err(codes::COMPRESSION_RATIO_EXCEEDED, "The ZIP contains an entry with an excessive compression ratio.");
    }
    None
}

fn is_safe_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('/') || name.starts_with('\\') || name.contains('\\') {
        return false;
    }
    let bytes = name.as_bytes();
    if bytes.get(1) == Some(&b':') && bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return false;
    }
    !name.split('/').any(|segment| segment == "." || segment == "..")
}

// ----- reading the structure -----

fn u16_at(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2).map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    b.get(at..at + 8).map_or(0, |s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}

fn read_at<R: Read + Seek>(source: &mut R, offset: u64, len: usize) -> Result<Vec<u8>, OpenError> {
    source.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    source.read_exact(&mut buf)?;
    Ok(buf)
}

/// Where the central directory sits, as the end records describe it.
struct Directory {
    start: u64,
    size: u64,
    count: u64,
}

fn read_entries<R: Read + Seek>(source: &mut R) -> Result<Vec<Entry>, OpenError> {
    let len = source.seek(SeekFrom::End(0))?;
    let dir = find_directory(source, len)?;
    if dir.count > MAX_ENTRIES || dir.size > MAX_CENTRAL_BYTES {
        return Err(OpenError::Invalid("too many entries"));
    }
    let size = usize::try_from(dir.size).map_err(|_| OpenError::Invalid("directory size"))?;
    let central = read_at(source, dir.start, size)?;
    let mut entries = Vec::new();
    let mut at = 0usize;
    for _ in 0..dir.count {
        let (entry, next) = parse_central(&central, at)?;
        entries.push(entry);
        at = next;
    }
    preflight(source, &mut entries, &dir, len)?;
    Ok(entries)
}

fn find_directory<R: Read + Seek>(source: &mut R, len: u64) -> Result<Directory, OpenError> {
    let tail_len = len.min(EOCD_LEN + u64::from(u16::MAX));
    if tail_len < EOCD_LEN {
        return Err(OpenError::Invalid("no end record"));
    }
    let tail_start = len - tail_len;
    let tail = read_at(source, tail_start, usize::try_from(tail_len).map_err(|_| OpenError::Invalid("tail"))?)?;
    let last = tail.len().saturating_sub(22);
    for index in (0..=last).rev() {
        if u32_at(&tail, index) != EOCD_SIG {
            continue;
        }
        let comment = usize::from(u16_at(&tail, index + 20));
        if index + 22 + comment != tail.len() {
            continue;
        }
        let eocd_at = tail_start + index as u64;
        let disk = u16_at(&tail, index + 4);
        let cd_disk = u16_at(&tail, index + 6);
        let count = u16_at(&tail, index + 10);
        let size = u32_at(&tail, index + 12);
        let offset = u32_at(&tail, index + 16);
        let dir = if offset == u32::MAX || size == u32::MAX || count == u16::MAX {
            read_zip64_directory(source, eocd_at, len)?
        } else {
            if disk != 0 || cd_disk != 0 {
                return Err(OpenError::Invalid("multi-disk"));
            }
            Directory { start: u64::from(offset), size: u64::from(size), count: u64::from(count) }
        };
        if dir.start > eocd_at.saturating_sub(4) || dir.start.checked_add(dir.size).is_none_or(|end| end > eocd_at) {
            return Err(OpenError::Invalid("central directory offset"));
        }
        if u32_at(&read_at(source, dir.start, 4)?, 0) != CENTRAL_SIG && dir.count > 0 {
            return Err(OpenError::Invalid("central directory signature"));
        }
        return Ok(dir);
    }
    Err(OpenError::Invalid("no end record"))
}

fn read_zip64_directory<R: Read + Seek>(source: &mut R, eocd_at: u64, len: u64) -> Result<Directory, OpenError> {
    let locator_at = eocd_at.checked_sub(ZIP64_LOCATOR_LEN).ok_or(OpenError::Invalid("zip64 locator"))?;
    let locator = read_at(source, locator_at, 20)?;
    if u32_at(&locator, 0) != ZIP64_LOCATOR_SIG {
        return Err(OpenError::Invalid("zip64 locator"));
    }
    let record_at = u64_at(&locator, 8);
    if record_at > len.saturating_sub(ZIP64_EOCD_MIN_LEN) {
        return Err(OpenError::Invalid("zip64 record"));
    }
    let record = read_at(source, record_at, 56)?;
    if u32_at(&record, 0) != ZIP64_EOCD_SIG {
        return Err(OpenError::Invalid("zip64 record"));
    }
    let expected = locator_at.checked_sub(record_at).and_then(|d| d.checked_sub(12));
    if expected != Some(u64_at(&record, 4)) {
        return Err(OpenError::Invalid("zip64 record length"));
    }
    if u32_at(&record, 16) != 0 || u32_at(&record, 20) != 0 {
        return Err(OpenError::Invalid("multi-disk"));
    }
    Ok(Directory { start: u64_at(&record, 48), size: u64_at(&record, 40), count: u64_at(&record, 32) })
}

fn decode_name(raw: &[u8], flags: u16) -> Result<String, OpenError> {
    if flags & FLAG_UTF8 != 0 {
        return String::from_utf8(raw.to_vec()).map_err(|_| OpenError::Invalid("entry name"));
    }
    // Code page 437 for ASCII is ASCII; other bytes cannot form an allowed entry name, so any
    // stable mapping keeps the verdict (unexpected entry) unchanged.
    Ok(raw.iter().map(|&b| if b.is_ascii() { char::from(b) } else { char::from_u32(0x2580 + u32::from(b)).unwrap_or('?') }).collect())
}

fn parse_central(central: &[u8], at: usize) -> Result<(Entry, usize), OpenError> {
    let header = central.get(at..at + CENTRAL_LEN).ok_or(OpenError::Invalid("central record"))?;
    if u32_at(header, 0) != CENTRAL_SIG {
        return Err(OpenError::Invalid("central record signature"));
    }
    let name_len = usize::from(u16_at(header, 28));
    let extra_len = usize::from(u16_at(header, 30));
    let comment_len = usize::from(u16_at(header, 32));
    let name_at = at + CENTRAL_LEN;
    let extra_at = name_at + name_len;
    let next = extra_at + extra_len + comment_len;
    let raw_name = central.get(name_at..extra_at).ok_or(OpenError::Invalid("central name"))?;
    let extra = central.get(extra_at..extra_at + extra_len).ok_or(OpenError::Invalid("central extra"))?;
    if central.len() < next {
        return Err(OpenError::Invalid("central comment"));
    }
    let flags = u16_at(header, 8);
    let mut entry = Entry {
        name: decode_name(raw_name, flags)?,
        version_needed: u16_at(header, 6),
        host: u16_at(header, 4).to_be_bytes()[0],
        flags,
        method: u16_at(header, 10),
        crc: u32_at(header, 16),
        compressed_size: u64::from(u32_at(header, 20)),
        size: u64::from(u32_at(header, 24)),
        offset: u64::from(u32_at(header, 42)),
        external_attrs: u32_at(header, 38),
        data_offset: 0,
    };
    let disk = u16_at(header, 34);
    apply_zip64_extra(&mut entry, extra, disk)?;
    Ok((entry, next))
}

fn apply_zip64_extra(entry: &mut Entry, extra: &[u8], disk: u16) -> Result<(), OpenError> {
    let full = u64::from(u32::MAX);
    let wants = entry.size == full || entry.compressed_size == full || entry.offset == full || disk == u16::MAX;
    let mut at = 0usize;
    let mut found = false;
    while at + 4 <= extra.len() {
        let id = u16_at(extra, at);
        let len = usize::from(u16_at(extra, at + 2));
        let body = extra.get(at + 4..at + 4 + len).ok_or(OpenError::Invalid("extra field"))?;
        if id == 0x0001 {
            found = true;
            let mut pos = 0usize;
            let mut take = |field: &mut u64| -> Result<(), OpenError> {
                if *field == full {
                    *field = body.get(pos..pos + 8).map(|_| u64_at(body, pos)).ok_or(OpenError::Invalid("zip64 extra"))?;
                    pos += 8;
                }
                Ok(())
            };
            take(&mut entry.size)?;
            take(&mut entry.compressed_size)?;
            take(&mut entry.offset)?;
        }
        at += 4 + len;
    }
    if disk != 0 && disk != u16::MAX {
        return Err(OpenError::Invalid("multi-disk"));
    }
    if wants && !found {
        return Err(OpenError::Invalid("zip64 extra missing"));
    }
    Ok(())
}

/// Checks every local header against its central record, then that the data spans tile the
/// file up to the central directory.
fn preflight<R: Read + Seek>(source: &mut R, entries: &mut [Entry], dir: &Directory, len: u64) -> Result<(), OpenError> {
    for entry in entries.iter_mut() {
        check_local_header(source, entry, len)?;
    }
    let mut order: Vec<&Entry> = entries.iter().collect();
    order.sort_by_key(|e| e.offset);
    for (i, entry) in order.iter().enumerate() {
        let boundary = order.get(i + 1).map_or(dir.start, |next| next.offset);
        if boundary <= entry.data_offset {
            return Err(OpenError::Invalid("data span"));
        }
        let span = if entry.flags & FLAG_DESCRIPTOR != 0 { descriptor_span(source, entry, boundary)? } else { boundary - entry.data_offset };
        if span != entry.compressed_size {
            return Err(OpenError::Invalid("compressed size"));
        }
    }
    Ok(())
}

fn check_local_header<R: Read + Seek>(source: &mut R, entry: &mut Entry, len: u64) -> Result<(), OpenError> {
    if entry.offset > len.saturating_sub(LOCAL_LEN) || len < LOCAL_LEN {
        return Err(OpenError::Invalid("local offset"));
    }
    let header = read_at(source, entry.offset, 30)?;
    if u32_at(&header, 0) != LOCAL_SIG
        || u16_at(&header, 4) != entry.version_needed
        || u16_at(&header, 6) != entry.flags
        || u16_at(&header, 8) != entry.method
    {
        return Err(OpenError::Invalid("local header"));
    }
    let name_len = u16_at(&header, 26);
    let extra_len = u16_at(&header, 28);
    let name_at = entry.offset + LOCAL_LEN;
    if name_at + u64::from(name_len) + u64::from(extra_len) > len {
        return Err(OpenError::Invalid("local name"));
    }
    let raw = read_at(source, name_at, usize::from(name_len))?;
    if decode_name(&raw, entry.flags)? != entry.name {
        return Err(OpenError::Invalid("local name mismatch"));
    }
    entry.data_offset = name_at + u64::from(name_len) + u64::from(extra_len);
    if entry.flags & FLAG_DESCRIPTOR != 0 {
        return Ok(());
    }
    let matches = |local: u32, central: u64| if central <= u64::from(u32::MAX) { u64::from(local) == central } else { local == u32::MAX };
    if u32_at(&header, 14) != entry.crc || !matches(u32_at(&header, 18), entry.compressed_size) || !matches(u32_at(&header, 22), entry.size) {
        return Err(OpenError::Invalid("local sizes"));
    }
    Ok(())
}

/// The compressed span of an entry whose sizes follow its data in a data descriptor.
fn descriptor_span<R: Read + Seek>(source: &mut R, entry: &Entry, boundary: u64) -> Result<u64, OpenError> {
    let zip64 = entry.needs_zip64();
    let lengths: [u64; 2] = if zip64 { [24, 20] } else { [16, 12] };
    for length in lengths {
        let Some(at) = boundary.checked_sub(length) else { continue };
        let Some(span) = at.checked_sub(entry.data_offset) else { continue };
        let d = read_at(source, at, usize::try_from(length).map_err(|_| OpenError::Invalid("descriptor"))?)?;
        let mut pos = 0usize;
        if length == 16 || length == 24 {
            if u32_at(&d, 0) != DESCRIPTOR_SIG {
                continue;
            }
            pos = 4;
        }
        let crc = u32_at(&d, pos);
        pos += 4;
        let (compressed, size) =
            if zip64 { (u64_at(&d, pos), u64_at(&d, pos + 8)) } else { (u64::from(u32_at(&d, pos)), u64::from(u32_at(&d, pos + 4))) };
        if crc == entry.crc && compressed == entry.compressed_size && size == entry.size && compressed == span {
            return Ok(span);
        }
    }
    Err(OpenError::Invalid("data descriptor"))
}

// ----- entry data -----

/// An uncompressed entry stream that enforces a byte limit and verifies size and CRC at the end.
pub(crate) struct EntryReader<'a> {
    inner: Box<dyn Read + 'a>,
    limit: u64,
    expected_size: u64,
    expected_crc: u32,
    compressed_size: u64,
    read: u64,
    crc: crc32fast::Hasher,
    failed: Option<(&'static str, String)>,
    verified: bool,
}

impl<'a> EntryReader<'a> {
    fn open<R: Read + Seek>(source: &'a mut R, entry: &Entry, limit: u64) -> io::Result<Self> {
        source.seek(SeekFrom::Start(entry.data_offset))?;
        let raw = Read::take(source, entry.compressed_size);
        let inner: Box<dyn Read + 'a> = if entry.method == 8 { Box::new(flate2::read::DeflateDecoder::new(raw)) } else { Box::new(raw) };
        Ok(Self {
            inner,
            limit,
            expected_size: entry.size,
            expected_crc: entry.crc,
            compressed_size: entry.compressed_size,
            read: 0,
            crc: crc32fast::Hasher::new(),
            failed: None,
            verified: false,
        })
    }

    fn verify(&mut self) -> io::Result<()> {
        if self.verified {
            return Ok(());
        }
        self.verified = true;
        let ratio_exceeded =
            if self.compressed_size == 0 { self.read > 0 } else { self.compressed_size.checked_mul(MAX_RATIO).is_some_and(|max| self.read > max) };
        if ratio_exceeded {
            return Err(fault(codes::COMPRESSION_RATIO_EXCEEDED, "The ZIP contains an entry with an excessive compression ratio."));
        }
        if self.read != self.expected_size || self.crc.clone().finalize() != self.expected_crc {
            return Err(fault(codes::INVALID_ARCHIVE, "The ZIP entry data does not match its declared size or CRC."));
        }
        Ok(())
    }
}

impl Read for EntryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some((code, message)) = &self.failed {
            return Err(fault(code, message.clone()));
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let remaining = self.limit.saturating_sub(self.read).saturating_add(1);
        let cap = usize::try_from(remaining).map_or(buf.len(), |r| r.min(buf.len()));
        let target = buf.get_mut(..cap).unwrap_or_default();
        let n = match self.inner.read(target) {
            Ok(n) => n,
            Err(e) if matches!(e.kind(), io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput | io::ErrorKind::UnexpectedEof) => {
                return Err(fault(codes::INVALID_ARCHIVE, "The ZIP entry data is corrupt."));
            }
            Err(e) => return Err(e),
        };
        if n == 0 {
            self.verify()?;
            return Ok(0);
        }
        if self.read > self.limit.saturating_sub(n as u64) {
            self.read = self.limit;
            let message = format!("A ZIP entry exceeded the {}-byte streaming limit.", self.limit);
            self.failed = Some((codes::STREAM_LIMIT_EXCEEDED, message.clone()));
            return Err(fault(codes::STREAM_LIMIT_EXCEEDED, message));
        }
        self.read += n as u64;
        self.crc.update(target.get(..n).unwrap_or_default());
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::is_safe_name;

    #[test]
    fn unsafe_names() {
        for bad in ["", "/a", "\\a", "a\\b", "C:x", "../a", "a/./b", "a/.."] {
            assert!(!is_safe_name(bad), "{bad}");
        }
        for good in ["handoff.json", "a/b", "...", "1:x"] {
            assert!(is_safe_name(good), "{good}");
        }
    }
}
