use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;
use image::{DynamicImage, GenericImageView, GrayImage, ImageFormat, RgbImage};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use zip::ZipArchive;

use crate::SanitizedFilename;
use crate::backend::AppDirectories;
use crate::backend::database::{Database, OfflineCatalogMetadata, OfflineDbChapter, OfflineDbManga, normalize_chapter_number};
use crate::backend::manga_downloader::chapter_download_id_candidates;
use crate::backend::manga_provider::{
    Chapter, ChapterReader, ChapterToRead, Languages, ListOfChapters, Manga, MangaPanel, MangaProviders, MangaStatus,
    SortedChapters, SortedVolumes, Volumes,
};
use crate::i18n::tr;

/// Compares strings with natural numeric ordering (e.g. `2.jpg` < `10.jpg`, `0001.jpg` < `0002.jpg`).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ia = 0usize;
    let mut ib = 0usize;
    let ba = a.as_bytes();
    let bb = b.as_bytes();

    while ia < ba.len() && ib < bb.len() {
        let ca = ba[ia];
        let cb = bb[ib];
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let start_a = ia;
            while ia < ba.len() && ba[ia].is_ascii_digit() {
                ia += 1;
            }
            let start_b = ib;
            while ib < bb.len() && bb[ib].is_ascii_digit() {
                ib += 1;
            }

            let num_a = &ba[start_a..ia];
            let num_b = &bb[start_b..ib];

            let trim_a = match num_a.iter().position(|&c| c != b'0') {
                Some(idx) => &num_a[idx..],
                None => &num_a[num_a.len().saturating_sub(1)..],
            };
            let trim_b = match num_b.iter().position(|&c| c != b'0') {
                Some(idx) => &num_b[idx..],
                None => &num_b[num_b.len().saturating_sub(1)..],
            };

            let ord = trim_a.len().cmp(&trim_b.len()).then_with(|| trim_a.cmp(trim_b));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            let sa = &a[ia..];
            let sb = &b[ib..];
            let cha = sa.chars().next().unwrap();
            let chb = sb.chars().next().unwrap();
            let ord = cha.to_ascii_lowercase().cmp(&chb.to_ascii_lowercase());
            if ord != Ordering::Equal {
                return ord;
            }
            ia += cha.len_utf8();
            ib += chb.len_utf8();
        }
    }

    ba.len().cmp(&bb.len()).then_with(|| a.cmp(b))
}

#[inline]
pub fn is_supported_image_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".webp")
        || lower.ends_with(".avif")
}

/// Formats a byte count into a human-readable size label (`B`, `KB`, `MB`, `GB`).
pub fn format_size_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if b < MB {
        format!("{:.0} KB", b / KB)
    } else if b < GB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{:.2} GB", b / GB)
    }
}

fn dir_size_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

fn collect_zip_image_names(path: &Path) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut image_names = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if entry.is_file() && is_supported_image_name(entry.name()) {
            image_names.push(entry.name().to_string());
        }
    }
    image_names.sort_by(|a, b| natural_cmp(a, b));
    Ok(image_names)
}

fn collect_dir_image_paths(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error + Send + Sync>> {
    let entries = std::fs::read_dir(dir)?;
    let mut image_paths = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            if is_supported_image_name(&name) {
                image_paths.push(p);
            }
        }
    }
    image_paths.sort_by(|a, b| {
        let na = a.file_name().unwrap_or_default().to_string_lossy();
        let nb = b.file_name().unwrap_or_default().to_string_lossy();
        natural_cmp(&na, &nb)
    });
    Ok(image_paths)
}

fn obj_as_bytes(obj: &Object) -> Option<&[u8]> {
    match obj {
        Object::Name(b) => Some(b.as_slice()),
        Object::String(b, _) => Some(b.as_slice()),
        _ => None,
    }
}

fn deref_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    match doc.dereference(obj) {
        Ok((_, Object::Dictionary(d))) => Some(d),
        _ => None,
    }
}

fn find_page_resources(doc: &Document, page_id: ObjectId) -> Option<&Dictionary> {
    let mut current_id = Some(page_id);
    for _ in 0..8 {
        let id = current_id?;
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(res_obj) = dict.get(b"Resources")
            && let Some(res_dict) = deref_dict(doc, res_obj)
        {
            return Some(res_dict);
        }
        current_id = dict.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    None
}

fn extract_page_image_stream(doc: &Document, page_id: ObjectId) -> Option<&Stream> {
    let res_dict = find_page_resources(doc, page_id)?;
    let xobj_obj = res_dict.get(b"XObject").ok()?;
    let xobj_dict = deref_dict(doc, xobj_obj)?;

    for (_, val) in xobj_dict.iter() {
        if let Ok((_, Object::Stream(stream))) = doc.dereference(val)
            && let Ok(subtype_obj) = stream.dict.get(b"Subtype")
            && let Ok((_, deref_sub)) = doc.dereference(subtype_obj)
            && obj_as_bytes(deref_sub) == Some(b"Image")
        {
            return Some(stream);
        }
    }
    None
}

fn extract_filter_name(doc: &Document, dict: &Dictionary) -> Option<Vec<u8>> {
    let filter_obj = dict.get(b"Filter").ok()?;
    let (_, deref) = doc.dereference(filter_obj).ok()?;
    if let Some(bytes) = obj_as_bytes(deref) {
        return Some(bytes.to_vec());
    }
    if let Object::Array(arr) = deref
        && let Some(first) = arr.first()
        && let Ok((_, first_deref)) = doc.dereference(first)
    {
        return obj_as_bytes(first_deref).map(|b| b.to_vec());
    }
    None
}

fn extract_dict_u32(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<u32> {
    let obj = dict.get(key).ok()?;
    let (_, deref) = doc.dereference(obj).ok()?;
    let val = deref.as_i64().ok()?;
    u32::try_from(val).ok().filter(|&v| v > 0)
}

fn extract_colorspace_bytes(doc: &Document, dict: &Dictionary) -> Option<Vec<u8>> {
    let cs_obj = dict.get(b"ColorSpace").ok()?;
    let (_, deref) = doc.dereference(cs_obj).ok()?;
    if let Some(bytes) = obj_as_bytes(deref) {
        return Some(bytes.to_vec());
    }
    if let Object::Array(arr) = deref
        && let Some(first) = arr.first()
        && let Ok((_, first_deref)) = doc.dereference(first)
    {
        return obj_as_bytes(first_deref).map(|b| b.to_vec());
    }
    None
}

enum DecodedPdfImage {
    EncodedBytes(Vec<u8>),
    Dynamic(DynamicImage),
}

fn decode_pdf_image_stream(doc: &Document, stream: &Stream) -> Result<DecodedPdfImage, Box<dyn Error + Send + Sync>> {
    let filter = extract_filter_name(doc, &stream.dict);
    match filter.as_deref() {
        Some(b"DCTDecode") | Some(b"JPXDecode") => Ok(DecodedPdfImage::EncodedBytes(stream.content.clone())),
        Some(b"FlateDecode") | None => {
            let raw_pixels = if filter.as_deref() == Some(b"FlateDecode") {
                let mut decoder = ZlibDecoder::new(stream.content.as_slice());
                let mut out = Vec::new();
                decoder.read_to_end(&mut out)?;
                out
            } else {
                stream.content.clone()
            };

            let width = extract_dict_u32(doc, &stream.dict, b"Width").ok_or("PDF image missing valid Width")?;
            let height = extract_dict_u32(doc, &stream.dict, b"Height").ok_or("PDF image missing valid Height")?;
            let pixel_count = (width as usize).checked_mul(height as usize).ok_or("PDF image dimensions overflow")?;

            let cs = extract_colorspace_bytes(doc, &stream.dict);
            let is_gray = cs.as_deref() == Some(b"DeviceGray") || (cs.is_none() && raw_pixels.len() == pixel_count);

            if is_gray {
                if raw_pixels.len() < pixel_count {
                    return Err("PDF grayscale image buffer is truncated".into());
                }
                let mut buf = raw_pixels;
                buf.truncate(pixel_count);
                let img = GrayImage::from_raw(width, height, buf).ok_or("Failed to build GrayImage from PDF stream")?;
                Ok(DecodedPdfImage::Dynamic(DynamicImage::ImageLuma8(img)))
            } else {
                let expected_rgb = pixel_count.checked_mul(3).ok_or("PDF RGB buffer size overflow")?;
                if raw_pixels.len() < expected_rgb {
                    return Err("PDF RGB image buffer is truncated".into());
                }
                let mut buf = raw_pixels;
                buf.truncate(expected_rgb);
                let img = RgbImage::from_raw(width, height, buf).ok_or("Failed to build RgbImage from PDF stream")?;
                Ok(DecodedPdfImage::Dynamic(DynamicImage::ImageRgb8(img)))
            }
        },
        Some(other) => Err(format!("Unsupported PDF image filter: {}", String::from_utf8_lossy(other)).into()),
    }
}

fn load_pdf_page_image(path: &Path, index: usize) -> Result<DecodedPdfImage, Box<dyn Error + Send + Sync>> {
    let doc = Document::load(path)?;
    let pages = doc.get_pages();
    let stream = pages
        .into_values()
        .filter_map(|page_id| extract_page_image_stream(&doc, page_id))
        .nth(index)
        .ok_or_else(|| format!("Page index {index} out of bounds in PDF"))?;
    decode_pdf_image_stream(&doc, stream)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalChapterSource {
    Cbz(PathBuf),
    Epub(PathBuf),
    Pdf(PathBuf),
    Directory(PathBuf),
}

impl LocalChapterSource {
    /// Classifies a filesystem path as a readable local chapter source across CBZ, EPUB, PDF, or RAW image folder.
    pub fn from_path(path: &Path) -> Option<Self> {
        if path.is_file() {
            let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
            if name.ends_with(".cbz") {
                return Some(Self::Cbz(path.to_path_buf()));
            }
            if name.ends_with(".epub") {
                return Some(Self::Epub(path.to_path_buf()));
            }
            if name.ends_with(".pdf") {
                return Some(Self::Pdf(path.to_path_buf()));
            }
            return None;
        }
        if path.is_dir() {
            let has_images = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .flatten()
                .any(|entry| entry.path().is_file() && is_supported_image_name(&entry.file_name().to_string_lossy()));
            if has_images {
                return Some(Self::Directory(path.to_path_buf()));
            }
        }
        None
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Cbz(p) | Self::Epub(p) | Self::Pdf(p) | Self::Directory(p) => p.as_path(),
        }
    }

    pub fn format_label(&self) -> &'static str {
        match self {
            Self::Cbz(_) => "CBZ",
            Self::Epub(_) => "EPUB",
            Self::Pdf(_) => "PDF",
            Self::Directory(_) => "RAW",
        }
    }

    pub fn size_bytes(&self) -> u64 {
        match self {
            Self::Cbz(p) | Self::Epub(p) | Self::Pdf(p) => std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            Self::Directory(d) => dir_size_bytes(d),
        }
    }

    pub fn find(manga_title: &str, chapter_id: &str) -> Option<Self> {
        let base_dir = AppDirectories::MangaDownloads.get_full_path();
        Self::find_in_dir(&base_dir, manga_title, chapter_id)
    }

    pub fn find_in_dir(base_dir: &Path, manga_title: &str, chapter_id: &str) -> Option<Self> {
        if !base_dir.exists() || chapter_id.is_empty() {
            return None;
        }

        let mut candidates = Vec::new();
        candidates.push(chapter_id.to_string());
        let id_clean = chapter_id.replace(['/', ':', '\\', '?', '*', '"', '<', '>', '|'], "_");
        if !candidates.contains(&id_clean) {
            candidates.push(id_clean);
        }
        if chapter_id.starts_with("chapters/")
            && let Some(segment) = chapter_id.split('/').nth(1)
            && !segment.is_empty()
            && !candidates.iter().any(|c| c == segment)
        {
            candidates.push(segment.to_string());
        }
        if let Some((_, entry_name)) = chapter_id.rsplit_once("::")
            && !entry_name.is_empty()
            && !candidates.iter().any(|c| c == entry_name)
        {
            candidates.push(entry_name.to_string());
        }

        fn check_path(path: &Path, candidates: &[String]) -> Option<LocalChapterSource> {
            let name = path.file_name()?.to_string_lossy();
            if candidates.iter().any(|c| name.contains(c.as_str())) {
                return LocalChapterSource::from_path(path);
            }
            None
        }

        fn search_dir(dir: &Path, candidates: &[String], depth: usize) -> Option<LocalChapterSource> {
            if depth == 0 {
                return None;
            }
            let entries = std::fs::read_dir(dir).ok()?;
            let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();

            let mut subdirs = Vec::new();
            for p in paths {
                if let Some(src) = check_path(&p, candidates) {
                    return Some(src);
                }
                if p.is_dir() {
                    subdirs.push(p);
                }
            }
            for subdir in subdirs {
                if let Some(src) = search_dir(&subdir, candidates, depth - 1) {
                    return Some(src);
                }
            }
            None
        }

        if !manga_title.trim().is_empty() {
            let sanitized_manga_dir = base_dir.join(SanitizedFilename::new(manga_title).as_path());
            if sanitized_manga_dir.is_dir()
                && let Some(found) = search_dir(&sanitized_manga_dir, &candidates, 2)
            {
                return Some(found);
            }
        }

        search_dir(base_dir, &candidates, 3)
    }

    /// Finds a downloaded local archive/directory only if it covers the chapter completely when online page count is known.
    pub fn find_for_chapter(manga_title: &str, chapter_id: &str, expected_remote_pages: usize) -> Option<Self> {
        let src = Self::find(manga_title, chapter_id)?;
        let local_count = src.page_count();
        if local_count == 0 {
            return None;
        }
        if expected_remote_pages > 0 && local_count != expected_remote_pages {
            return None;
        }
        Some(src)
    }

    pub fn page_count(&self) -> usize {
        match self {
            Self::Cbz(path) | Self::Epub(path) => collect_zip_image_names(path).map(|v| v.len()).unwrap_or(0),
            Self::Directory(dir) => collect_dir_image_paths(dir).map(|v| v.len()).unwrap_or(0),
            Self::Pdf(path) => {
                let Ok(doc) = Document::load(path) else {
                    return 0;
                };
                doc.get_pages()
                    .into_values()
                    .filter(|&page_id| extract_page_image_stream(&doc, page_id).is_some())
                    .count()
            },
        }
    }

    pub fn load_page_bytes(&self, index: usize) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        match self {
            Self::Cbz(path) | Self::Epub(path) => {
                let image_names = collect_zip_image_names(path)?;
                let entry_name = image_names.get(index).ok_or_else(|| format!("Page index {index} out of bounds"))?;
                let file = File::open(path)?;
                let mut archive = ZipArchive::new(file)?;
                let mut entry_file = archive.by_name(entry_name)?;
                let mut bytes = Vec::new();
                entry_file.read_to_end(&mut bytes)?;
                Ok(bytes)
            },
            Self::Directory(dir) => {
                let image_paths = collect_dir_image_paths(dir)?;
                let page_path = image_paths.get(index).ok_or_else(|| format!("Page index {index} out of bounds"))?;
                Ok(std::fs::read(page_path)?)
            },
            Self::Pdf(path) => match load_pdf_page_image(path, index)? {
                DecodedPdfImage::EncodedBytes(bytes) => Ok(bytes),
                DecodedPdfImage::Dynamic(img) => {
                    let mut out = Vec::new();
                    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;
                    Ok(out)
                },
            },
        }
    }

    pub fn load_page(&self, index: usize) -> Result<MangaPanel, Box<dyn Error + Send + Sync>> {
        match self {
            Self::Pdf(path) => {
                let image_decoded = match load_pdf_page_image(path, index)? {
                    DecodedPdfImage::EncodedBytes(bytes) => image::load_from_memory(&bytes)?,
                    DecodedPdfImage::Dynamic(img) => img,
                };
                let dimensions = image_decoded.dimensions();
                Ok(MangaPanel {
                    image_decoded,
                    dimensions,
                })
            },
            _ => {
                let bytes = self.load_page_bytes(index)?;
                let image_decoded = image::load_from_memory(&bytes)?;
                let dimensions = image_decoded.dimensions();
                Ok(MangaPanel {
                    image_decoded,
                    dimensions,
                })
            },
        }
    }

    pub fn load_first_page_image(&self) -> Option<DynamicImage> {
        self.load_page(0).ok().map(|panel| panel.image_decoded)
    }
}

enum LocalChapterHandleKind {
    Cbz { path: PathBuf, entries: Vec<String> },
    Epub { path: PathBuf, entries: Vec<String> },
    Directory { dir: PathBuf, paths: Vec<PathBuf> },
    Pdf { path: PathBuf, doc: std::sync::Mutex<Document>, page_ids: Vec<ObjectId> },
}

/// Cached handle for fast random-access local page loading across CBZ, EPUB, PDF, and RAW.
pub struct LocalChapterHandle {
    kind: LocalChapterHandleKind,
}

impl std::fmt::Debug for LocalChapterHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalChapterHandle")
            .field("format", &self.format_label())
            .field("path", &self.path())
            .field("page_count", &self.page_count())
            .finish()
    }
}

impl LocalChapterHandle {
    pub fn open(source: &LocalChapterSource) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let kind = match source {
            LocalChapterSource::Cbz(path) => LocalChapterHandleKind::Cbz {
                path: path.clone(),
                entries: collect_zip_image_names(path)?,
            },
            LocalChapterSource::Epub(path) => LocalChapterHandleKind::Epub {
                path: path.clone(),
                entries: collect_zip_image_names(path)?,
            },
            LocalChapterSource::Directory(dir) => LocalChapterHandleKind::Directory {
                dir: dir.clone(),
                paths: collect_dir_image_paths(dir)?,
            },
            LocalChapterSource::Pdf(path) => {
                let doc = Document::load(path)?;
                let page_ids = doc
                    .get_pages()
                    .into_values()
                    .filter(|&page_id| extract_page_image_stream(&doc, page_id).is_some())
                    .collect();
                LocalChapterHandleKind::Pdf {
                    path: path.clone(),
                    doc: std::sync::Mutex::new(doc),
                    page_ids,
                }
            },
        };
        Ok(Self { kind })
    }

    pub fn page_count(&self) -> usize {
        match &self.kind {
            LocalChapterHandleKind::Cbz { entries, .. } | LocalChapterHandleKind::Epub { entries, .. } => entries.len(),
            LocalChapterHandleKind::Directory { paths, .. } => paths.len(),
            LocalChapterHandleKind::Pdf { page_ids, .. } => page_ids.len(),
        }
    }

    pub fn format_label(&self) -> &'static str {
        match &self.kind {
            LocalChapterHandleKind::Cbz { .. } => "CBZ",
            LocalChapterHandleKind::Epub { .. } => "EPUB",
            LocalChapterHandleKind::Directory { .. } => "RAW",
            LocalChapterHandleKind::Pdf { .. } => "PDF",
        }
    }

    pub fn path(&self) -> &Path {
        match &self.kind {
            LocalChapterHandleKind::Cbz { path, .. }
            | LocalChapterHandleKind::Epub { path, .. }
            | LocalChapterHandleKind::Pdf { path, .. } => path.as_path(),
            LocalChapterHandleKind::Directory { dir, .. } => dir.as_path(),
        }
    }

    pub fn load_page_bytes(&self, index: usize) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        match &self.kind {
            LocalChapterHandleKind::Cbz { path, entries } | LocalChapterHandleKind::Epub { path, entries } => {
                let entry_name = entries.get(index).ok_or_else(|| format!("Page index {index} out of bounds"))?;
                let file = File::open(path)?;
                let mut archive = ZipArchive::new(file)?;
                let mut entry_file = archive.by_name(entry_name)?;
                let mut bytes = Vec::new();
                entry_file.read_to_end(&mut bytes)?;
                Ok(bytes)
            },
            LocalChapterHandleKind::Directory { paths, .. } => {
                let page_path = paths.get(index).ok_or_else(|| format!("Page index {index} out of bounds"))?;
                Ok(std::fs::read(page_path)?)
            },
            LocalChapterHandleKind::Pdf { doc, page_ids, .. } => {
                let &page_id = page_ids
                    .get(index)
                    .ok_or_else(|| format!("Page index {index} out of bounds in PDF"))?;
                let guard = doc.lock().map_err(|_| "PDF document lock poisoned")?;
                let stream = extract_page_image_stream(&guard, page_id)
                    .ok_or_else(|| format!("Page index {index} image stream missing in PDF"))?;
                match decode_pdf_image_stream(&guard, stream)? {
                    DecodedPdfImage::EncodedBytes(bytes) => Ok(bytes),
                    DecodedPdfImage::Dynamic(img) => {
                        let mut out = Vec::new();
                        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;
                        Ok(out)
                    },
                }
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDiskChapterName {
    pub chapter_number: String,
    pub volume_number: Option<String>,
    pub title: String,
    pub disk_id_token: String,
}

fn extract_first_number_token(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut end = i + 1;
            let mut has_dot = false;
            while end < chars.len() {
                if chars[end].is_ascii_digit() {
                    end += 1;
                } else if chars[end] == '.' && !has_dot && end + 1 < chars.len() && chars[end + 1].is_ascii_digit() {
                    has_dot = true;
                    end += 1;
                } else {
                    break;
                }
            }
            let num: String = chars[i..end].iter().collect();
            return Some(normalize_chapter_number(&num));
        }
        i += 1;
    }
    None
}

/// Parses chapter metadata from either `manga-tui`'s `Ch <num> Vol <vol> ... <id>` convention or arbitrary filenames.
pub fn parse_disk_chapter_name(stem: &str) -> ParsedDiskChapterName {
    let trimmed = stem.trim();
    if let Some(after_ch) = trimmed.strip_prefix("Ch ")
        && let Some((num_raw, after_vol_marker)) = after_ch.split_once(" Vol ")
    {
        let chapter_number = {
            let raw = num_raw.trim();
            if raw.is_empty() { "0".to_string() } else { raw.to_string() }
        };

        let after_vol = after_vol_marker.trim_start();
        let (vol_token, rest) = match after_vol.split_once(char::is_whitespace) {
            Some((v, r)) => (v.trim(), r.trim()),
            None => (after_vol.trim(), ""),
        };

        let volume_number =
            if vol_token.is_empty() || vol_token.eq_ignore_ascii_case("none") { None } else { Some(vol_token.to_string()) };

        let (title, disk_id_token) = if rest.is_empty() {
            (String::new(), String::new())
        } else {
            let mut parts: Vec<&str> = rest.split_whitespace().collect();
            let id_tok = parts.pop().unwrap_or_default().to_string();
            if parts.last() == Some(&"_") {
                parts.pop();
            }
            if parts == ["_"] {
                parts.clear();
            }
            let title_str = parts.join(" ");
            (title_str, id_tok)
        };

        return ParsedDiskChapterName {
            chapter_number,
            volume_number,
            title,
            disk_id_token,
        };
    }

    let chapter_number = extract_first_number_token(trimmed).unwrap_or_else(|| "0".to_string());
    ParsedDiskChapterName {
        chapter_number,
        volume_number: None,
        title: trimmed.to_string(),
        disk_id_token: trimmed.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OfflineChapter {
    pub chapter: Chapter,
    pub source: LocalChapterSource,
    pub format_label: &'static str,
    pub size_bytes: u64,
    pub is_read: bool,
    pub is_bookmarked: bool,
    pub bookmarked_page: Option<u32>,
}

impl OfflineChapter {
    pub fn to_chapter_to_read(&self, normalized_number: f64) -> Option<ChapterToRead> {
        let count = self.source.page_count();
        if count == 0 {
            return None;
        }
        let pages_url = (0..count)
            .filter_map(|i| reqwest::Url::parse(&format!("http://localhost/local/{i}")).ok())
            .collect();
        let volume = self
            .chapter
            .volume_number
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("none")
            .to_string();

        Some(ChapterToRead {
            id: self.chapter.id.clone(),
            title: self.chapter.title.clone(),
            number: normalized_number,
            volume_number: Some(volume),
            num_page_bookmarked: self.bookmarked_page,
            language: self.chapter.language,
            pages_url,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OfflineManga {
    pub manga_id: String,
    pub title: String,
    pub folder_name: String,
    pub provider: Option<MangaProviders>,
    pub is_favorite: bool,
    pub chapters: Vec<OfflineChapter>,
    pub total_size_bytes: u64,
    pub languages: Vec<Languages>,
}

impl OfflineManga {
    pub fn read_count(&self) -> usize {
        self.chapters.iter().filter(|c| c.is_read).count()
    }

    pub fn format_labels(&self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        for ch in &self.chapters {
            if !labels.contains(&ch.format_label) {
                labels.push(ch.format_label);
            }
        }
        labels
    }

    pub fn to_manga(&self) -> Manga {
        let languages = if self.languages.is_empty() { vec![Languages::default()] } else { self.languages.clone() };
        Manga {
            id: self.manga_id.clone(),
            id_safe_for_download: SanitizedFilename::new(&self.title).to_string(),
            title: self.title.clone(),
            genres: vec![],
            description: tr(
                "Modo sin conexión — leyendo capítulos descargados localmente en tu disco.",
                "Offline mode — reading chapters downloaded locally on disk.",
            )
            .to_string(),
            status: MangaStatus::default(),
            cover_img_url: String::new(),
            languages,
            rating: String::new(),
            artist: None,
            author: None,
            provider: self.provider,
        }
    }

    pub fn sources_map(&self) -> HashMap<String, LocalChapterSource> {
        self.chapters.iter().map(|c| (c.chapter.id.clone(), c.source.clone())).collect()
    }

    /// Assigns unique `f64` chapter numbers matching `ListOfChapters` (`get_next_chapter` / `get_previous_chapter`).
    pub fn normalized_chapter_numbers(&self) -> HashMap<String, f64> {
        let mut ordered: Vec<&OfflineChapter> = self.chapters.iter().collect();
        ordered.sort_by(|a, b| compare_chapters_ascending(a, b));

        let mut used_by_vol: HashMap<String, HashSet<String>> = HashMap::new();
        let mut result = HashMap::with_capacity(ordered.len());

        for ch in ordered {
            let vol = ch
                .chapter
                .volume_number
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or("none")
                .to_string();
            let used = used_by_vol.entry(vol).or_default();

            let base = ch.chapter.chapter_number.trim().parse::<f64>().ok().filter(|n| n.is_finite());
            let mut candidate = base.unwrap_or(1.0);
            if used.contains(&candidate.to_string()) {
                if base.is_some() {
                    let mut step = 1u32;
                    let base_hundredths = (candidate * 100.0).round();
                    loop {
                        let next = (base_hundredths + f64::from(step)) / 100.0;
                        if !used.contains(&next.to_string()) {
                            candidate = next;
                            break;
                        }
                        step += 1;
                    }
                } else {
                    while used.contains(&candidate.to_string()) {
                        candidate += 1.0;
                    }
                }
            }
            used.insert(candidate.to_string());
            result.insert(ch.chapter.id.clone(), candidate);
        }

        result
    }

    pub fn build_list_of_chapters(&self) -> ListOfChapters {
        let numbers = self.normalized_chapter_numbers();
        let mut by_volume: BTreeMap<String, Vec<ChapterReader>> = BTreeMap::new();

        for ch in &self.chapters {
            let volume = ch
                .chapter
                .volume_number
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or("none")
                .to_string();
            let num = numbers.get(&ch.chapter.id).copied().unwrap_or(0.0);
            by_volume.entry(volume.clone()).or_default().push(ChapterReader {
                id: ch.chapter.id.clone(),
                number: num.to_string(),
                volume,
            });
        }

        let volumes = by_volume
            .into_iter()
            .map(|(volume, chapters)| Volumes {
                volume,
                chapters: SortedChapters::new(chapters),
            })
            .collect();

        ListOfChapters {
            volumes: SortedVolumes::new(volumes),
        }
    }

    pub fn chapter_to_read(&self, chapter_id: &str) -> Option<ChapterToRead> {
        let numbers = self.normalized_chapter_numbers();
        let ch = self.chapters.iter().find(|c| c.chapter.id == chapter_id)?;
        let num = numbers.get(chapter_id).copied().unwrap_or(0.0);
        ch.to_chapter_to_read(num)
    }

    pub fn first_source(&self) -> Option<&LocalChapterSource> {
        self.chapters.first().map(|c| &c.source)
    }
}

fn strip_archive_extension(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for ext in [".cbz", ".epub", ".pdf"] {
        if lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

struct DiskEntry {
    entry_name: String,
    stem: String,
    language: Option<Languages>,
    source: LocalChapterSource,
    parsed: ParsedDiskChapterName,
    size_bytes: u64,
}

/// Recursively collects chapter archives (`.cbz`, `.pdf`, `.epub`) and RAW image chapter folders
/// inside a manga folder up to `max_depth` levels deep (supports nested volume/arc subdirectories).
fn collect_disk_entries_in_manga_dir(
    dir: &Path,
    inherited_lang: Option<Languages>,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<DiskEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(src) = LocalChapterSource::from_path(&path) {
            let stem = strip_archive_extension(&name).to_string();
            let size_bytes = src.size_bytes();
            let parsed = parse_disk_chapter_name(&stem);
            out.push(DiskEntry {
                entry_name: name,
                stem,
                language: inherited_lang,
                source: src,
                parsed,
                size_bytes,
            });
        } else if path.is_dir() && depth < max_depth {
            let sub_lang = Languages::from_str_lenient(&name).filter(|&l| l != Languages::Unkown).or(inherited_lang);
            collect_disk_entries_in_manga_dir(&path, sub_lang, depth + 1, max_depth, out);
        }
    }
}

/// Infers a series title and `ParsedDiskChapterName` from a loose external archive filename stem
/// placed directly in the root downloads folder (e.g. `"Berserk Vol 01"`, `"One Piece - Cap 1100"`, `"Uzumaki"`).
pub fn infer_manga_and_chapter_from_external_filename(stem: &str) -> (String, ParsedDiskChapterName) {
    let cleaned = stem.replace('_', " ");
    let cleaned = cleaned.trim();
    let lower = cleaned.to_lowercase();

    let markers = [
        (" - cap.", false),
        (" - cap ", false),
        (" - capítulo ", false),
        (" - capitulo ", false),
        (" - ch.", false),
        (" - ch ", false),
        (" - chapter ", false),
        (" cap.", false),
        (" cap ", false),
        (" capítulo ", false),
        (" capitulo ", false),
        (" ch.", false),
        (" ch ", false),
        (" chapter ", false),
        (" episodio ", false),
        (" - vol.", true),
        (" - vol ", true),
        (" - volumen ", true),
        (" - volume ", true),
        (" - tomo ", true),
        (" vol.", true),
        (" vol ", true),
        (" volumen ", true),
        (" volume ", true),
        (" tomo ", true),
        (" v0", true),
        (" v1", true),
        (" v2", true),
        (" v3", true),
        (" #", false),
    ];

    for (marker, is_vol) in markers {
        if let Some(idx) = lower.find(marker)
            && idx > 0
        {
            let series = cleaned[..idx]
                .trim_matches(|c: char| c.is_whitespace() || c == '-' || c == ':' || c == '_')
                .trim();
            let rest = cleaned[idx..].trim_start_matches(|c: char| c.is_whitespace() || c == '-').trim();
            if !series.is_empty() {
                let mut parsed = parse_disk_chapter_name(rest);
                if is_vol && parsed.volume_number.is_none() {
                    let digits: String = rest
                        .chars()
                        .skip_while(|c| !c.is_ascii_digit())
                        .take_while(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    let digits = digits.trim_end_matches('.');
                    if !digits.is_empty() {
                        parsed.volume_number = Some(digits.to_string());
                        if parsed.chapter_number == "0" {
                            parsed.chapter_number = digits.to_string();
                        }
                    }
                }
                if parsed.title.is_empty() {
                    parsed.title = rest.to_string();
                }
                return (series.to_string(), parsed);
            }
        }
    }

    if let Some((left, right)) = cleaned.split_once(" - ") {
        let series = left.trim();
        let right = right.trim();
        if !series.is_empty() && !right.is_empty() {
            let mut parsed = parse_disk_chapter_name(right);
            if parsed.title.is_empty() {
                parsed.title = right.to_string();
            }
            return (series.to_string(), parsed);
        }
    }

    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.len() >= 2 {
        let last = tokens[tokens.len() - 1].trim_start_matches('#');
        if last.parse::<f64>().is_ok() {
            let series = tokens[..tokens.len() - 1]
                .join(" ")
                .trim_matches(|c: char| c.is_whitespace() || c == '-')
                .to_string();
            if !series.is_empty() {
                return (series, ParsedDiskChapterName {
                    chapter_number: last.to_string(),
                    volume_number: None,
                    title: format!("Cap. {last}"),
                    disk_id_token: String::new(),
                });
            }
        }
    }

    let series = if cleaned.is_empty() { "Manga Local".to_string() } else { cleaned.to_string() };
    (series.clone(), ParsedDiskChapterName {
        chapter_number: "1".to_string(),
        volume_number: None,
        title: series,
        disk_id_token: String::new(),
    })
}

/// Imports an external `.cbz`, `.pdf`, or `.epub` archive file (or a directory containing external archives)
/// into `downloads_dir/<InferredMangaTitle>/`, returning the number of imported chapter archives.
pub fn import_external_archives_into_downloads(source_path: &Path, downloads_dir: &Path) -> Result<usize, String> {
    if !source_path.exists() {
        return Err(format!("Path does not exist: {}", source_path.display()));
    }

    let mut files_to_import: Vec<PathBuf> = Vec::new();
    if source_path.is_file() {
        if LocalChapterSource::from_path(source_path).is_some() {
            files_to_import.push(source_path.to_path_buf());
        }
    } else if source_path.is_dir() {
        for entry in std::fs::read_dir(source_path).map_err(|e| e.to_string())?.flatten() {
            let p = entry.path();
            if p.is_file() && LocalChapterSource::from_path(&p).is_some() {
                files_to_import.push(p);
            }
        }
    }

    let mut imported_count = 0usize;
    for file_path in files_to_import {
        let Some(file_name) = file_path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let stem = strip_archive_extension(&file_name);
        let (series_title, _) = infer_manga_and_chapter_from_external_filename(stem);
        let safe_folder = SanitizedFilename::new(&series_title).to_string();
        let target_dir = downloads_dir.join(if safe_folder.trim().is_empty() { "Manga Local" } else { &safe_folder });
        std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        let target_file = target_dir.join(&file_name);
        if target_file != file_path {
            std::fs::copy(&file_path, &target_file).map_err(|e| e.to_string())?;
        }
        imported_count += 1;
    }

    Ok(imported_count)
}

fn db_chapter_matches_disk(
    db_ch: &OfflineDbChapter,
    provider: MangaProviders,
    disk: &DiskEntry,
    offline_fallback_id: &str,
) -> bool {
    if db_ch.id == offline_fallback_id {
        return true;
    }
    for candidate in chapter_download_id_candidates(provider, &db_ch.id) {
        if !disk.parsed.disk_id_token.is_empty() && disk.parsed.disk_id_token == candidate {
            return true;
        }
        if disk.stem == candidate || disk.stem.ends_with(&format!(" {candidate}")) {
            return true;
        }
    }
    false
}

fn default_provider_language(provider: Option<MangaProviders>) -> Option<Languages> {
    match provider? {
        MangaProviders::Mangaoni | MangaProviders::Leercapitulo | MangaProviders::Inmanga => Some(Languages::Spanish),
        MangaProviders::Weebcentral | MangaProviders::Mangapill | MangaProviders::Comizy => Some(Languages::English),
        MangaProviders::Mangadex => None,
    }
}

fn compare_chapters_ascending(a: &OfflineChapter, b: &OfflineChapter) -> Ordering {
    let na = a.chapter.chapter_number.trim().parse::<f64>().ok().filter(|n| n.is_finite());
    let nb = b.chapter.chapter_number.trim().parse::<f64>().ok().filter(|n| n.is_finite());
    match (na, nb) {
        (Some(va), Some(vb)) => va
            .total_cmp(&vb)
            .then_with(|| natural_cmp(&a.chapter.title, &b.chapter.title))
            .then_with(|| natural_cmp(&a.chapter.id, &b.chapter.id)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => natural_cmp(&a.chapter.title, &b.chapter.title).then_with(|| natural_cmp(&a.chapter.id, &b.chapter.id)),
    }
}

/// Scans `AppDirectories::MangaDownloads` and enriches downloaded entries with SQLite metadata when available.
pub fn scan_offline_mangas() -> Vec<OfflineManga> {
    let base_dir = AppDirectories::MangaDownloads.get_full_path();
    let conn = Database::get_connection().ok();
    let db = conn.as_ref().map(Database::new);
    scan_offline_mangas_in_dir(&base_dir, db.as_ref())
}

/// Scans `base_dir` for downloaded chapters in CBZ, EPUB, PDF, and RAW formats, reconciling with `db`.
pub fn scan_offline_mangas_in_dir(base_dir: &Path, db: Option<&Database<'_>>) -> Vec<OfflineManga> {
    let Ok(entries1) = std::fs::read_dir(base_dir) else {
        return Vec::new();
    };

    let metadata: OfflineCatalogMetadata = db.and_then(|d| d.get_offline_catalog_metadata().ok()).unwrap_or_default();

    let mut db_manga_by_id: HashMap<&str, &OfflineDbManga> = HashMap::new();
    let mut db_mangas_by_folder: HashMap<String, Vec<&OfflineDbManga>> = HashMap::new();
    for m in &metadata.mangas {
        db_manga_by_id.insert(m.id.as_str(), m);
        let sanitized = SanitizedFilename::new(&m.title).to_string().to_lowercase();
        db_mangas_by_folder.entry(sanitized).or_default().push(m);
        let raw_lower = m.title.trim().to_lowercase();
        if !raw_lower.is_empty() {
            let list = db_mangas_by_folder.entry(raw_lower).or_default();
            if !list.iter().any(|existing| existing.id == m.id) {
                list.push(m);
            }
        }
    }

    let mut db_chapters_by_manga: HashMap<&str, Vec<&OfflineDbChapter>> = HashMap::new();
    for ch in &metadata.chapters {
        db_chapters_by_manga.entry(ch.manga_id.as_str()).or_default().push(ch);
    }

    let mut mangas = Vec::new();
    let mut loose_root_entries_by_series: HashMap<String, (String, Vec<DiskEntry>)> = HashMap::new();

    for entry1 in entries1.flatten() {
        let manga_path = entry1.path();
        if !manga_path.is_dir() {
            let file_name = entry1.file_name().to_string_lossy().into_owned();
            if let Some(src) = LocalChapterSource::from_path(&manga_path) {
                let stem = strip_archive_extension(&file_name).to_string();
                let size_bytes = src.size_bytes();
                let (series_title, parsed) = infer_manga_and_chapter_from_external_filename(&stem);
                let key = series_title.to_lowercase();
                let group = loose_root_entries_by_series.entry(key).or_insert_with(|| (series_title, Vec::new()));
                group.1.push(DiskEntry {
                    entry_name: file_name,
                    stem,
                    language: None,
                    source: src,
                    parsed,
                    size_bytes,
                });
            }
            continue;
        }
        let folder_name = entry1.file_name().to_string_lossy().into_owned();
        let mut disk_entries = Vec::new();
        collect_disk_entries_in_manga_dir(&manga_path, None, 0, 3, &mut disk_entries);

        if disk_entries.is_empty() {
            continue;
        }

        let folder_lower = folder_name.to_lowercase();
        let mut candidates: Vec<&OfflineDbManga> = db_mangas_by_folder.get(&folder_lower).cloned().unwrap_or_default();

        // Also locate DB manga via chapter ID match if folder was renamed or only offline IDs exist
        let offline_manga_id = format!("offline::{folder_name}");
        if let Some(direct) = db_manga_by_id.get(offline_manga_id.as_str())
            && !candidates.iter().any(|c| c.id == direct.id)
        {
            candidates.push(direct);
        }

        if candidates.is_empty() {
            for db_ch in &metadata.chapters {
                if let Some(parent_manga) = db_manga_by_id.get(db_ch.manga_id.as_str())
                    && disk_entries.iter().any(|d| {
                        let fallback_id = format!("offline::{folder_name}::{}", d.entry_name);
                        db_chapter_matches_disk(db_ch, parent_manga.provider, d, &fallback_id)
                    })
                    && !candidates.iter().any(|c| c.id == parent_manga.id)
                {
                    candidates.push(parent_manga);
                }
            }
        }

        let matched_db_manga = candidates.into_iter().max_by_key(|m| {
            let db_chs = db_chapters_by_manga.get(m.id.as_str()).map(|v| v.as_slice()).unwrap_or(&[]);
            let match_score = disk_entries
                .iter()
                .filter(|d| {
                    let fallback_id = format!("offline::{folder_name}::{}", d.entry_name);
                    db_chs.iter().any(|ch| db_chapter_matches_disk(ch, m.provider, d, &fallback_id))
                })
                .count();
            (match_score, usize::from(m.is_favorite))
        });

        let manga_id = matched_db_manga.map(|m| m.id.clone()).unwrap_or_else(|| offline_manga_id.clone());
        let title = matched_db_manga.map(|m| m.title.clone()).unwrap_or_else(|| folder_name.clone());
        let provider = matched_db_manga.map(|m| m.provider);
        let is_favorite = matched_db_manga.is_some_and(|m| m.is_favorite);

        // Gather all DB chapters associated with this manga (or sharing the same title & provider)
        let mut candidate_db_chapters: Vec<&OfflineDbChapter> = Vec::new();
        if let Some(chs) = db_chapters_by_manga.get(manga_id.as_str()) {
            candidate_db_chapters.extend(chs.iter().copied());
        }
        if let Some(same_title_mangas) = db_mangas_by_folder.get(&folder_lower) {
            for m in same_title_mangas {
                if m.id != manga_id
                    && Some(m.provider) == provider
                    && let Some(chs) = db_chapters_by_manga.get(m.id.as_str())
                {
                    candidate_db_chapters.extend(chs.iter().copied());
                }
            }
        }

        let folder_fallback_language = disk_entries
            .iter()
            .find_map(|d| d.language.filter(|&l| l != Languages::Unkown))
            .or_else(|| {
                candidate_db_chapters.iter().find_map(|c| {
                    let s = c.translated_language.trim();
                    (!s.is_empty())
                        .then(|| Languages::from_str_lenient(s))
                        .flatten()
                        .filter(|&l| l != Languages::Unkown)
                })
            })
            .or_else(|| default_provider_language(provider))
            .unwrap_or_else(|| *Languages::get_preferred_lang());

        let mut chapters = Vec::with_capacity(disk_entries.len());
        let mut total_size_bytes = 0u64;
        let mut languages = Vec::new();

        for d in disk_entries {
            total_size_bytes = total_size_bytes.saturating_add(d.size_bytes);
            let format_label = d.source.format_label();
            let fallback_id = format!("offline::{folder_name}::{}", d.entry_name);

            let matched_ch = candidate_db_chapters
                .iter()
                .copied()
                .find(|ch| db_chapter_matches_disk(ch, provider.unwrap_or_default(), &d, &fallback_id));

            let chapter_id = matched_ch.map(|c| c.id.clone()).unwrap_or(fallback_id);
            let is_read = matched_ch.is_some_and(|c| c.is_read);
            let is_bookmarked = matched_ch.is_some_and(|c| c.is_bookmarked);
            let bookmarked_page = matched_ch.and_then(|c| c.number_page_bookmarked);

            let language = d
                .language
                .filter(|&l| l != Languages::Unkown)
                .or_else(|| {
                    matched_ch
                        .map(|c| c.translated_language.trim())
                        .filter(|s| !s.is_empty())
                        .and_then(Languages::from_str_lenient)
                        .filter(|&l| l != Languages::Unkown)
                })
                .unwrap_or(folder_fallback_language);

            if language != Languages::Unkown && !languages.contains(&language) {
                languages.push(language);
            }

            let chapter_title =
                if !d.parsed.title.is_empty() { d.parsed.title } else { matched_ch.map(|c| c.title.clone()).unwrap_or_default() };

            let chapter = Chapter {
                id: chapter_id,
                id_safe_for_download: SanitizedFilename::new(&d.entry_name).to_string(),
                manga_id: manga_id.clone(),
                title: chapter_title,
                language,
                chapter_number: d.parsed.chapter_number,
                volume_number: d.parsed.volume_number,
                scanlator: Some(format!("💾 {format_label} · {}", format_size_bytes(d.size_bytes))),
                publication_date: None,
            };

            chapters.push(OfflineChapter {
                chapter,
                source: d.source,
                format_label,
                size_bytes: d.size_bytes,
                is_read,
                is_bookmarked,
                bookmarked_page,
            });
        }

        if languages.is_empty() {
            languages.push(Languages::default());
        }

        chapters.sort_by(|a, b| compare_chapters_ascending(b, a));

        mangas.push(OfflineManga {
            manga_id,
            title,
            folder_name,
            provider,
            is_favorite,
            chapters,
            total_size_bytes,
            languages,
        });
    }

    // Merge or add loose root-level external archives (.cbz, .pdf, .epub placed directly in base_dir)
    for (series_lower, (series_title, root_entries)) in loose_root_entries_by_series {
        let mut new_chapters = Vec::with_capacity(root_entries.len());
        let mut added_bytes = 0u64;
        let default_lang = *Languages::get_preferred_lang();
        let folder_name = SanitizedFilename::new(&series_title).to_string();

        for d in root_entries {
            added_bytes = added_bytes.saturating_add(d.size_bytes);
            let format_label = d.source.format_label();
            let chapter_id = format!("offline::root::{folder_name}::{}", d.entry_name);
            let matched_ch = metadata.chapters.iter().find(|ch| ch.id == chapter_id);
            let is_read = matched_ch.is_some_and(|c| c.is_read);
            let is_bookmarked = matched_ch.is_some_and(|c| c.is_bookmarked);
            let bookmarked_page = matched_ch.and_then(|c| c.number_page_bookmarked);
            let chapter = Chapter {
                id: chapter_id,
                id_safe_for_download: SanitizedFilename::new(&d.entry_name).to_string(),
                manga_id: format!("offline::{folder_name}"),
                title: d.parsed.title,
                language: default_lang,
                chapter_number: d.parsed.chapter_number,
                volume_number: d.parsed.volume_number,
                scanlator: Some(format!("📦 {format_label} · {}", format_size_bytes(d.size_bytes))),
                publication_date: None,
            };
            new_chapters.push(OfflineChapter {
                chapter,
                source: d.source,
                format_label,
                size_bytes: d.size_bytes,
                is_read,
                is_bookmarked,
                bookmarked_page,
            });
        }

        if let Some(existing) = mangas
            .iter_mut()
            .find(|m| m.title.to_lowercase() == series_lower || m.folder_name.to_lowercase() == folder_name.to_lowercase())
        {
            for mut ch in new_chapters {
                ch.chapter.manga_id = existing.manga_id.clone();
                existing.chapters.push(ch);
            }
            existing.total_size_bytes = existing.total_size_bytes.saturating_add(added_bytes);
            existing.chapters.sort_by(|a, b| compare_chapters_ascending(b, a));
        } else {
            new_chapters.sort_by(|a, b| compare_chapters_ascending(b, a));
            mangas.push(OfflineManga {
                manga_id: format!("offline::{folder_name}"),
                title: series_title,
                folder_name,
                provider: None,
                is_favorite: false,
                chapters: new_chapters,
                total_size_bytes: added_bytes,
                languages: vec![default_lang],
            });
        }
    }

    mangas.sort_by(|a, b| {
        b.is_favorite
            .cmp(&a.is_favorite)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    mangas
}

/// Finds a specific downloaded manga on disk by `manga_id` or `manga_title` (used for offline fallback).
pub fn find_offline_manga(manga_id: &str, manga_title: &str, provider: Option<MangaProviders>) -> Option<OfflineManga> {
    let base_dir = AppDirectories::MangaDownloads.get_full_path();
    let conn = Database::get_connection().ok();
    let db = conn.as_ref().map(Database::new);
    find_offline_manga_in_dir(&base_dir, manga_id, manga_title, provider, db.as_ref())
}

pub fn find_offline_manga_in_dir(
    base_dir: &Path,
    manga_id: &str,
    manga_title: &str,
    provider: Option<MangaProviders>,
    db: Option<&Database<'_>>,
) -> Option<OfflineManga> {
    let all = scan_offline_mangas_in_dir(base_dir, db);
    if all.is_empty() {
        return None;
    }

    if !manga_id.is_empty()
        && let Some(exact) = all.iter().find(|m| m.manga_id == manga_id)
    {
        return Some(exact.clone());
    }

    let trimmed_title = manga_title.trim();
    if trimmed_title.is_empty() {
        return None;
    }

    let sanitized_target = SanitizedFilename::new(trimmed_title).to_string().to_lowercase();
    let raw_lower = trimmed_title.to_lowercase();

    let mut matches: Vec<OfflineManga> = all
        .into_iter()
        .filter(|m| {
            m.title.trim().to_lowercase() == raw_lower
                || m.folder_name.to_lowercase() == sanitized_target
                || m.folder_name.to_lowercase() == raw_lower
        })
        .collect();

    if let Some(target_prov) = provider
        && let Some(idx) = matches.iter().position(|m| m.provider == Some(target_prov))
    {
        return Some(matches.swap_remove(idx));
    }

    matches.into_iter().next()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use bytes::Bytes;
    use image::{DynamicImage, GrayImage, RgbImage};
    use rusqlite::Connection;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;
    use crate::backend::database::{ChapterToBookmark, ChapterToSaveHistory, MangaReadingHistorySave, SetChapterDownloaded};
    use crate::backend::manga_downloader::cbz_downloader::CbzDownloader;
    use crate::backend::manga_downloader::epub_downloader::EpubDownloader;
    use crate::backend::manga_downloader::pdf_downloader::PdfDownloader;
    use crate::backend::manga_downloader::raw_images::RawImagesDownloader;
    use crate::backend::manga_downloader::{ChapterToDownloadSanitized, MangaDownloader};
    use crate::backend::manga_provider::ChapterPage;
    use crate::config::DownloadType;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample_jpeg_bytes() -> Vec<u8> {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data_test/images/1.jpg")).to_vec()
    }

    fn sample_png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 15) as u8, (y * 20) as u8, 128]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn sample_gray_png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = GrayImage::from_fn(w, h, |x, y| image::Luma([((x + y) * 10) as u8]));
        let mut out = Vec::new();
        DynamicImage::ImageLuma8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn sample_chapter_sanitized(
        chapter_id: &str,
        ch_num: &str,
        pages: Vec<ChapterPage>,
        download_type: DownloadType,
    ) -> ChapterToDownloadSanitized {
        ChapterToDownloadSanitized {
            chapter_id: chapter_id.to_string(),
            manga_id: "manga-1".to_string(),
            manga_title: "Test Offline Manga".to_string().into(),
            chapter_title: format!("Capítulo {ch_num}").into(),
            chapter_number: ch_num.to_string(),
            volume_number: None,
            language: Languages::Spanish,
            scanlator: "TestScan".to_string().into(),
            download_type,
            pages,
            provider: Some(MangaProviders::Inmanga),
        }
    }

    #[test]
    fn natural_cmp_orders_numeric_filenames_correctly() {
        let mut names = vec!["10.jpg", "2.jpg", "1.jpg", "20.jpg", "3.jpg"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["1.jpg", "2.jpg", "3.jpg", "10.jpg", "20.jpg"]);
    }

    #[test]
    fn local_chapter_source_reads_all_four_formats() {
        let temp = TempDir::new("offline_all_formats");
        let root = &temp.0;

        let jpg_page = ChapterPage {
            bytes: Bytes::from(sample_jpeg_bytes()),
            extension: "jpg".to_string(),
        };
        let png_page = ChapterPage {
            bytes: Bytes::from(sample_png_bytes(8, 12)),
            extension: "png".to_string(),
        };
        let gray_png_page = ChapterPage {
            bytes: Bytes::from(sample_gray_png_bytes(6, 10)),
            extension: "png".to_string(),
        };

        // 1. CBZ
        CbzDownloader::new()
            .save_chapter_in_file_system(
                root,
                sample_chapter_sanitized("id_cbz", "1", vec![jpg_page.clone(), png_page.clone()], DownloadType::Cbz),
            )
            .unwrap();
        let cbz_src = LocalChapterSource::find_in_dir(root, "Test Offline Manga", "id_cbz").expect("CBZ found");
        assert_eq!(cbz_src.format_label(), "CBZ");
        assert_eq!(cbz_src.page_count(), 2);
        assert!(cbz_src.load_page(0).is_ok());
        assert_eq!(cbz_src.load_page(1).unwrap().dimensions, (8, 12));

        // 2. EPUB
        EpubDownloader::new()
            .save_chapter_in_file_system(
                root,
                sample_chapter_sanitized("id_epub", "2", vec![jpg_page.clone(), png_page.clone()], DownloadType::Epub),
            )
            .unwrap();
        let epub_src = LocalChapterSource::find_in_dir(root, "Test Offline Manga", "id_epub").expect("EPUB found");
        assert_eq!(epub_src.format_label(), "EPUB");
        assert_eq!(epub_src.page_count(), 2);
        assert!(epub_src.load_page(0).is_ok());
        assert_eq!(epub_src.load_page(1).unwrap().dimensions, (8, 12));

        // 3. PDF (JPEG DCTDecode + RGB PNG FlateDecode + Grayscale PNG FlateDecode)
        PdfDownloader::new()
            .save_chapter_in_file_system(
                root,
                sample_chapter_sanitized("id_pdf", "3", vec![jpg_page.clone(), png_page.clone(), gray_png_page], DownloadType::Pdf),
            )
            .unwrap();
        let pdf_src = LocalChapterSource::find_in_dir(root, "Test Offline Manga", "id_pdf").expect("PDF found");
        assert_eq!(pdf_src.format_label(), "PDF");
        assert_eq!(pdf_src.page_count(), 3);
        let p0_bytes = pdf_src.load_page_bytes(0).unwrap();
        assert!(p0_bytes.starts_with(&[0xFF, 0xD8, 0xFF]), "JPEG page should extract as raw JPEG");
        assert!(pdf_src.load_page(0).is_ok());
        assert_eq!(pdf_src.load_page(1).unwrap().dimensions, (8, 12));
        assert_eq!(pdf_src.load_page(2).unwrap().dimensions, (6, 10));
        let p1_png_bytes = pdf_src.load_page_bytes(1).unwrap();
        assert!(
            p1_png_bytes.starts_with(&[0x89, b'P', b'N', b'G']),
            "FlateDecode page should encode valid PNG bytes for web viewer"
        );

        // 4. RAW directory
        RawImagesDownloader::new()
            .save_chapter_in_file_system(root, sample_chapter_sanitized("id_raw", "4", vec![jpg_page, png_page], DownloadType::Raw))
            .unwrap();
        let raw_src = LocalChapterSource::find_in_dir(root, "Test Offline Manga", "id_raw").expect("RAW found");
        assert_eq!(raw_src.format_label(), "RAW");
        assert_eq!(raw_src.page_count(), 2);
        assert_eq!(raw_src.load_page(1).unwrap().dimensions, (8, 12));
    }

    #[test]
    fn cbz_natural_sort_avoids_lexicographic_order_bug() {
        let temp = TempDir::new("offline_cbz_natural");
        let cbz_path = temp.0.join("natural.cbz");
        let file = File::create(&cbz_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();

        for (name, w) in [("1.png", 4u32), ("2.png", 6u32), ("10.png", 10u32)] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(&sample_png_bytes(w, 5)).unwrap();
        }
        zip.finish().unwrap();

        let src = LocalChapterSource::Cbz(cbz_path);
        assert_eq!(src.page_count(), 3);
        assert_eq!(src.load_page(0).unwrap().dimensions.0, 4);
        assert_eq!(src.load_page(1).unwrap().dimensions.0, 6);
        assert_eq!(src.load_page(2).unwrap().dimensions.0, 10);
    }

    #[test]
    fn parse_disk_chapter_name_handles_all_provider_formats_and_manual_files() {
        let p1 = parse_disk_chapter_name("Ch 1195 Vol none Capítulo 1195  manga_one-piece_811902");
        assert_eq!(p1.chapter_number, "1195");
        assert_eq!(p1.volume_number, None);
        assert_eq!(p1.title, "Capítulo 1195");
        assert_eq!(p1.disk_id_token, "manga_one-piece_811902");

        let p2 = parse_disk_chapter_name("Ch 28.5 Vol 4 El Bosque ScanX abc-123");
        assert_eq!(p2.chapter_number, "28.5");
        assert_eq!(p2.volume_number.as_deref(), Some("4"));
        assert_eq!(p2.title, "El Bosque ScanX");
        assert_eq!(p2.disk_id_token, "abc-123");

        let p3 = parse_disk_chapter_name("Ch 234 Vol none   5281-10234000");
        assert_eq!(p3.chapter_number, "234");
        assert_eq!(p3.volume_number, None);
        assert_eq!(p3.title, "");
        assert_eq!(p3.disk_id_token, "5281-10234000");

        let p4 = parse_disk_chapter_name("One Piece 015 - Special.cbz");
        assert_eq!(p4.chapter_number, "15");
        assert_eq!(p4.volume_number, None);
        assert_eq!(p4.title, "One Piece 015 - Special.cbz");

        let p5 = parse_disk_chapter_name("Ch 31.1 Vol none Capítulo 31.1 _ manga_dame-ningen-no-aishikata_826027");
        assert_eq!(p5.chapter_number, "31.1");
        assert_eq!(p5.volume_number, None);
        assert_eq!(p5.title, "Capítulo 31.1");
        assert_eq!(p5.disk_id_token, "manga_dame-ningen-no-aishikata_826027");
    }

    #[test]
    fn scan_offline_mangas_unifies_single_folder_language_when_downloaded_chapters_have_empty_db_lang() {
        let temp = TempDir::new("offline_single_lang");
        let root = &temp.0;

        let oni_dir = root.join("Dame Ningen no Aishikata");
        std::fs::create_dir_all(&oni_dir).unwrap();
        std::fs::write(oni_dir.join("Ch 31.1 Vol none Capítulo 31.1 _ manga_dame-ningen-no-aishikata_826027.cbz"), b"cbz31")
            .unwrap();
        std::fs::write(oni_dir.join("Ch 30.2 Vol none Capítulo 30.2 _ manga_dame-ningen-no-aishikata_824811.cbz"), b"cbz30")
            .unwrap();
        std::fs::write(oni_dir.join("Ch 1 Vol none Capítulo 1 _ manga_dame-ningen-no-aishikata_780954.cbz"), b"cbz1").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        let db = Database::new(&conn);
        db.setup().unwrap();

        // All 3 chapters marked downloaded (which inserts translated_language = "")
        for (id, title) in [
            ("manga/dame-ningen-no-aishikata|826027", "Capítulo 31.1"),
            ("manga/dame-ningen-no-aishikata|824811", "Capítulo 30.2"),
            ("manga/dame-ningen-no-aishikata|780954", "Capítulo 1"),
        ] {
            db.set_chapter_downloaded(SetChapterDownloaded {
                id,
                title,
                manga_id: "manga/dame-ningen-no-aishikata",
                manga_title: "Dame Ningen no Aishikata",
                img_url: None,
                provider: MangaProviders::Mangaoni,
            })
            .unwrap();
        }

        // Only chapter 1 was opened in reader (sets translated_language = "es" on just 1 chapter)
        db.save_history(MangaReadingHistorySave {
            id: "manga/dame-ningen-no-aishikata",
            title: "Dame Ningen no Aishikata",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "manga/dame-ningen-no-aishikata|780954",
                title: "Capítulo 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangaoni,
        })
        .unwrap();

        let scanned = scan_offline_mangas_in_dir(root, Some(&db));
        assert_eq!(scanned.len(), 1);
        let manga = &scanned[0];
        assert_eq!(manga.provider, Some(MangaProviders::Mangaoni));
        // Must NOT split into [Spanish, English]; all 3 chapters are Spanish
        assert_eq!(manga.languages, vec![Languages::Spanish]);
        assert_eq!(manga.chapters.len(), 3);
        assert!(manga.chapters.iter().all(|c| c.chapter.language == Languages::Spanish));
        assert_eq!(manga.chapters[0].chapter.chapter_number, "31.1");
        assert_eq!(manga.chapters[0].chapter.title, "Capítulo 31.1");
        assert_eq!(manga.chapters[1].chapter.chapter_number, "30.2");
        assert_eq!(manga.chapters[2].chapter.chapter_number, "1");
    }

    #[test]
    fn scan_offline_mangas_reconciles_sqlite_multi_lang_and_manual_files() {
        let temp = TempDir::new("offline_scan");
        let root = &temp.0;

        // Multi-language manga (MangaDex) with DB metadata (read, bookmark, favorite)
        let md_dir = root.join("Dandadan").join("Spanish");
        std::fs::create_dir_all(&md_dir).unwrap();
        std::fs::write(md_dir.join("Ch 01 Vol 1 Primer contacto  ch_dd_1.cbz"), b"cbz1").unwrap();
        std::fs::write(md_dir.join("Ch 2 Vol 1 Segundo  ch_dd_2.epub"), b"epub2").unwrap();

        // Manually dropped manga with no DB records
        let manual_dir = root.join("Manual Comic");
        std::fs::create_dir_all(&manual_dir).unwrap();
        std::fs::write(manual_dir.join("Part A.pdf"), b"pdf1").unwrap();
        std::fs::write(manual_dir.join("Part B.pdf"), b"pdf2").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        let mut db = Database::new(&conn);
        db.setup().unwrap();
        db.set_chapter_downloaded(SetChapterDownloaded {
            id: "ch_dd_1",
            title: "Primer contacto",
            manga_id: "md_dandadan",
            manga_title: "Dandadan",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })
        .unwrap();
        db.save_history(MangaReadingHistorySave {
            id: "md_dandadan",
            title: "Dandadan",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "ch_dd_1",
                title: "Primer contacto",
                translated_language: "es",
            },
            provider: MangaProviders::Mangadex,
        })
        .unwrap();
        crate::backend::database::Bookmark::bookmark(&mut db, ChapterToBookmark {
            chapter_id: "ch_dd_2",
            manga_id: "md_dandadan",
            chapter_title: "Segundo",
            manga_title: "Dandadan",
            manga_cover_url: None,
            translated_language: Languages::Spanish,
            page_number: Some(7),
            provider: MangaProviders::Mangadex,
        })
        .unwrap();
        db.toggle_favorite("md_dandadan", MangaProviders::Mangadex, "Dandadan").unwrap();

        let scanned = scan_offline_mangas_in_dir(root, Some(&db));
        assert_eq!(scanned.len(), 2);

        // Favorite comes first
        let dandadan = &scanned[0];
        assert_eq!(dandadan.title, "Dandadan");
        assert_eq!(dandadan.manga_id, "md_dandadan");
        assert_eq!(dandadan.provider, Some(MangaProviders::Mangadex));
        assert!(dandadan.is_favorite);
        assert_eq!(dandadan.languages, vec![Languages::Spanish]);
        assert_eq!(dandadan.chapters.len(), 2);
        // Descending order: Ch 2 first, Ch 01 second
        assert_eq!(dandadan.chapters[0].chapter.id, "ch_dd_2");
        assert!(dandadan.chapters[0].is_bookmarked);
        assert_eq!(dandadan.chapters[0].bookmarked_page, Some(7));
        assert_eq!(dandadan.chapters[1].chapter.id, "ch_dd_1");
        assert!(dandadan.chapters[1].is_read);

        // ListOfChapters roundtrip works even with leading-zero "01"
        let list = dandadan.build_list_of_chapters();
        let numbers = dandadan.normalized_chapter_numbers();
        let ch1_num = numbers["ch_dd_1"];
        let ch2_num = numbers["ch_dd_2"];
        assert_eq!(ch1_num, 1.0);
        assert_eq!(ch2_num, 2.0);
        let next = list.get_next_chapter(Some("1"), ch1_num).expect("ch 2 should be next");
        assert_eq!(next.id, "ch_dd_2");
        let prev = list.get_previous_chapter(Some("1"), ch2_num).expect("ch 1 should be prev");
        assert_eq!(prev.id, "ch_dd_1");

        // Manual manga gets deterministic IDs & non-colliding sequential numbers for ListOfChapters
        let manual = &scanned[1];
        assert_eq!(manual.manga_id, "offline::Manual Comic");
        assert_eq!(manual.provider, None);
        let manual_list = manual.build_list_of_chapters();
        let manual_nums = manual.normalized_chapter_numbers();
        assert_ne!(manual_nums["offline::Manual Comic::Part A.pdf"], manual_nums["offline::Manual Comic::Part B.pdf"]);
        let part_a_num = manual_nums["offline::Manual Comic::Part A.pdf"];
        let next_manual = manual_list.get_next_chapter(None, part_a_num).expect("Part B follows Part A");
        assert_eq!(next_manual.id, "offline::Manual Comic::Part B.pdf");
    }

    #[test]
    fn test_scan_loose_root_archives_nested_volumes_and_importer() {
        let temp = TempDir::new("offline_external_scan");
        let root = &temp.0;

        // 1. Loose root-level CBZ & PDF files directly inside downloads root
        let write_dummy_cbz = |path: &Path| {
            let file = std::fs::File::create(path).unwrap();
            let mut zip = ZipWriter::new(file);
            zip.start_file("001.jpg", SimpleFileOptions::default()).unwrap();
            zip.write_all(&sample_jpeg_bytes()).unwrap();
            zip.finish().unwrap();
        };

        write_dummy_cbz(&root.join("Berserk Vol 01.cbz"));
        write_dummy_cbz(&root.join("Berserk Vol 02.cbz"));

        // 2. Deeply nested volume subdirectory inside a manga folder
        let nested_vol_dir = root.join("Vagabond").join("Arco Yoshioka").join("Volumen 25");
        std::fs::create_dir_all(&nested_vol_dir).unwrap();
        write_dummy_cbz(&nested_vol_dir.join("Cap 220.cbz"));

        let scanned = scan_offline_mangas_in_dir(root, None);
        assert_eq!(scanned.len(), 2);

        let berserk = scanned.iter().find(|m| m.title == "Berserk").expect("Berserk inferred from root CBZs");
        assert_eq!(berserk.chapters.len(), 2);
        assert_eq!(berserk.chapters[0].chapter.chapter_number, "2");
        assert_eq!(berserk.chapters[1].chapter.chapter_number, "1");

        let vagabond = scanned.iter().find(|m| m.title == "Vagabond").expect("Vagabond found from nested dir");
        assert_eq!(vagabond.chapters.len(), 1);

        // 3. Test import_external_archives_into_downloads
        let ext_src = TempDir::new("external_import_src");
        let ext_cbz = ext_src.0.join("Chainsaw Man - Cap 150.cbz");
        write_dummy_cbz(&ext_cbz);

        let imported = import_external_archives_into_downloads(&ext_cbz, root).unwrap();
        assert_eq!(imported, 1);
        assert!(root.join("Chainsaw Man").join("Chainsaw Man - Cap 150.cbz").exists());
    }

    #[test]
    fn local_chapter_handle_reads_all_four_formats_cbz_epub_pdf_raw() {
        let temp = TempDir::new("offline_handle_all_four");
        let root = &temp.0;

        let p0 = ChapterPage {
            bytes: Bytes::from(sample_png_bytes(8, 10)),
            extension: "png".to_string(),
        };
        let p1 = ChapterPage {
            bytes: Bytes::from(sample_png_bytes(12, 16)),
            extension: "png".to_string(),
        };

        CbzDownloader::new()
            .save_chapter_in_file_system(
                root,
                sample_chapter_sanitized("h_cbz", "1", vec![p0.clone(), p1.clone()], DownloadType::Cbz),
            )
            .unwrap();
        EpubDownloader::new()
            .save_chapter_in_file_system(
                root,
                sample_chapter_sanitized("h_epub", "2", vec![p0.clone(), p1.clone()], DownloadType::Epub),
            )
            .unwrap();
        PdfDownloader::new()
            .save_chapter_in_file_system(
                root,
                sample_chapter_sanitized("h_pdf", "3", vec![p0.clone(), p1.clone()], DownloadType::Pdf),
            )
            .unwrap();
        RawImagesDownloader::new()
            .save_chapter_in_file_system(
                root,
                sample_chapter_sanitized("h_raw", "4", vec![p0, p1], DownloadType::Raw),
            )
            .unwrap();

        for (id, expected_label, expected_dims) in [
            ("h_cbz", "CBZ", (8, 10)),
            ("h_epub", "EPUB", (8, 10)),
            ("h_pdf", "PDF", (8, 10)),
            ("h_raw", "RAW", (8, 10)),
        ] {
            let src = LocalChapterSource::find_in_dir(root, "Test Offline Manga", id).expect("local source exists");
            let handle = LocalChapterHandle::open(&src).expect("handle opens");
            assert_eq!(handle.format_label(), expected_label);
            assert_eq!(handle.page_count(), 2);
            assert!(handle.path().exists());

            let b0 = handle.load_page_bytes(0).expect("load page 0");
            let img0 = image::load_from_memory(&b0).expect("decode page 0");
            assert_eq!(img0.dimensions(), expected_dims);

            let b1 = handle.load_page_bytes(1).expect("load page 1");
            let img1 = image::load_from_memory(&b1).expect("decode page 1");
            assert_eq!(img1.dimensions(), (12, 16));
        }
    }
}
