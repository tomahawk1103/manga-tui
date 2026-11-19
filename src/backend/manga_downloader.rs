use std::collections::HashSet;
use std::error::Error;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use manga_tui::{SanitizedFilename, exists};

use super::manga_provider::{ChapterPage, Languages, MangaProviders};
use crate::config::DownloadType;

pub mod cbz_downloader;
pub mod epub_downloader;
pub mod pdf_downloader;
pub mod raw_images;

/// This struct represents a chapter with its data not having characteres that may throw errors
/// when creating files such as `/` or `\`
#[derive(Debug, PartialEq, Clone, Default)]
pub struct ChapterToDownloadSanitized {
    pub chapter_id: String,
    pub manga_id: String,
    pub manga_title: SanitizedFilename,
    pub chapter_title: SanitizedFilename,
    pub chapter_number: String,
    pub volume_number: Option<String>,
    pub language: Languages,
    pub scanlator: SanitizedFilename,
    pub download_type: DownloadType,
    pub pages: Vec<ChapterPage>,
    pub provider: Option<MangaProviders>,
}

pub trait MangaDownloader {
    /// The `base_directory` where the pages will be saved, for `raw_images`
    fn make_manga_base_directory_name(&self, base_directory: &Path, chapter: &ChapterToDownloadSanitized) -> PathBuf {
        let base = base_directory.join(chapter.manga_title.as_path());
        if chapter.provider.unwrap_or_default().is_multi_language() {
            base.join(chapter.language.as_human_readable())
        } else {
            base
        }
    }
    fn create_manga_base_directory(&self, base_directory: &Path) -> Result<(), Box<dyn Error>> {
        if !exists!(base_directory) {
            create_dir_all(base_directory)?
        }
        Ok(())
    }

    fn make_chapter_name(&self, chapter: &ChapterToDownloadSanitized) -> PathBuf {
        PathBuf::from(format!(
            "Ch {} Vol {} {} {} {}",
            chapter.chapter_number,
            chapter.volume_number.as_ref().cloned().unwrap_or("none".to_string()),
            chapter.chapter_title,
            chapter.scanlator,
            chapter.chapter_id
        ))
    }

    fn save_chapter_in_file_system(&self, base_directory: &Path, chapter: ChapterToDownloadSanitized)
    -> Result<(), Box<dyn Error>>;
}

fn strip_download_extension(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for ext in [".cbz", ".epub", ".pdf"] {
        if lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// Returns possible filename suffix IDs used on disk for a stored chapter ID and provider.
pub fn chapter_download_id_candidates(provider: MangaProviders, chapter_id: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if !chapter_id.is_empty() {
        candidates.push(chapter_id.to_string());
    }

    let sanitized = chapter_id.replace(['/', ':', '\\', '?', '*', '"', '<', '>', '|'], "_");
    if !sanitized.is_empty() && !candidates.contains(&sanitized) {
        candidates.push(sanitized);
    }

    if provider == MangaProviders::Mangapill
        && let Some(segment) = chapter_id.split('/').nth(1)
        && !segment.is_empty()
    {
        let segment = segment.to_string();
        if !candidates.contains(&segment) {
            candidates.push(segment);
        }
    }

    candidates
}

/// Lightweight index of downloaded chapter files and raw chapter directories on disk.
#[derive(Debug, Default, Clone)]
pub struct LocalDownloadsIndex {
    suffix_tokens: HashSet<String>,
    stems: Vec<String>,
}

impl LocalDownloadsIndex {
    fn insert_entry_name(&mut self, name: &str) {
        let stem = strip_download_extension(name).trim_end();
        if stem.is_empty() {
            return;
        }
        if let Some(token) = stem.rsplit(' ').next().filter(|t| !t.is_empty()) {
            self.suffix_tokens.insert(token.to_string());
        }
        self.stems.push(stem.to_string());
    }

    /// Scans `base_dir` for downloaded chapters at depth 2 (single-language / raw) and depth 3 (multi-language).
    /// Returns `None` if `base_dir` does not exist, is not a directory, or cannot be read.
    pub fn scan(base_dir: &Path) -> Option<Self> {
        let entries = std::fs::read_dir(base_dir).ok()?;
        let mut index = Self::default();

        for entry1 in entries.flatten() {
            if !entry1.file_type().ok().is_some_and(|ft| ft.is_dir()) {
                continue;
            }
            for entry2 in std::fs::read_dir(entry1.path()).into_iter().flatten().flatten() {
                let name2 = entry2.file_name().to_string_lossy().into_owned();
                index.insert_entry_name(&name2);

                if entry2.file_type().ok().is_some_and(|ft| ft.is_dir()) && !name2.starts_with("Ch ") {
                    for entry3 in std::fs::read_dir(entry2.path()).into_iter().flatten().flatten() {
                        let name3 = entry3.file_name().to_string_lossy().into_owned();
                        index.insert_entry_name(&name3);
                    }
                }
            }
        }

        Some(index)
    }

    pub fn contains_chapter(&self, provider: MangaProviders, chapter_id: &str) -> bool {
        for candidate in chapter_download_id_candidates(provider, chapter_id) {
            if !candidate.contains(' ') {
                if self.suffix_tokens.contains(&candidate) {
                    return true;
                }
            } else {
                let suffix = format!(" {candidate}");
                if self.stems.iter().any(|stem| stem == &candidate || stem.ends_with(&suffix)) {
                    return true;
                }
            }
        }
        false
    }

    pub fn has_any_chapter(&self, provider: MangaProviders, chapter_ids: &[String]) -> bool {
        chapter_ids.iter().any(|id| self.contains_chapter(provider, id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockMangaDownloader {}

    impl MangaDownloader for MockMangaDownloader {
        fn save_chapter_in_file_system(
            &self,
            _base_directory: &Path,
            _chapter: ChapterToDownloadSanitized,
        ) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
    }

    #[test]
    fn default_implementation_makes_manga_directory_name() {
        let downloader = MockMangaDownloader {};

        let test_chapter: ChapterToDownloadSanitized = ChapterToDownloadSanitized {
            chapter_id: "chapter id".to_string(),
            manga_id: "manga id".to_string(),
            manga_title: "some manga title".to_string().into(),
            chapter_title: "some chapter title".to_string().into(),
            chapter_number: "3".to_string(),
            volume_number: None,
            language: Languages::default(),
            scanlator: "".to_string().into(),
            download_type: crate::config::DownloadType::Cbz,
            pages: vec![],
            provider: None,
        };

        let expected = Path::new("./test/some manga title/English");
        let result = downloader.make_manga_base_directory_name(Path::new("./test"), &test_chapter);

        assert_eq!(expected, result);
    }

    #[test]
    fn single_language_provider_omits_language_directory() {
        let downloader = MockMangaDownloader {};

        let test_chapter: ChapterToDownloadSanitized = ChapterToDownloadSanitized {
            chapter_id: "chapter id".to_string(),
            manga_id: "manga id".to_string(),
            manga_title: "some manga title".to_string().into(),
            chapter_title: "some chapter title".to_string().into(),
            chapter_number: "3".to_string(),
            volume_number: None,
            language: Languages::Spanish,
            scanlator: "".to_string().into(),
            download_type: crate::config::DownloadType::Cbz,
            pages: vec![],
            provider: Some(MangaProviders::Inmanga),
        };

        let expected = Path::new("./test/some manga title");
        let result = downloader.make_manga_base_directory_name(Path::new("./test"), &test_chapter);

        assert_eq!(expected, result);
    }

    #[test]
    fn default_implementation_make_chapter_name() {
        let downloader = MockMangaDownloader {};

        let test_chapter: ChapterToDownloadSanitized = ChapterToDownloadSanitized {
            chapter_id: "chapter id".to_string(),
            manga_id: "manga id".to_string(),
            manga_title: "some manga title".to_string().into(),
            chapter_title: "some chapter title".to_string().into(),
            chapter_number: "3".to_string(),
            volume_number: Some(3.to_string()),
            language: Languages::default(),
            scanlator: "some scanlator".to_string().into(),
            download_type: crate::config::DownloadType::Cbz,
            pages: vec![],
            ..Default::default()
        };

        let expected = Path::new("Ch 3 Vol 3 some chapter title some scanlator chapter id");
        let result = downloader.make_chapter_name(&test_chapter);

        assert_eq!(expected, result);
    }

    struct TempTestDir(PathBuf);

    impl TempTestDir {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("create temp test dir");
            Self(path)
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn local_downloads_index_scan_returns_none_for_missing_directory() {
        let missing = std::env::temp_dir().join(format!("manga_tui_missing_{}", uuid::Uuid::new_v4()));
        assert!(LocalDownloadsIndex::scan(&missing).is_none());
    }

    #[test]
    fn local_downloads_index_matches_all_formats_and_providers_without_substring_false_positives() {
        let temp = TempTestDir::new("manga_tui_downloads_idx");
        let root = &temp.0;

        // Depth 2: single-language formats (.cbz, .epub, .pdf, Raw directory)
        let mangaoni_op = root.join("One Piece");
        std::fs::create_dir_all(&mangaoni_op).unwrap();
        std::fs::write(mangaoni_op.join("Ch 1195 Vol none Capítulo 1195  manga_one-piece_811902.cbz"), b"").unwrap();

        let mangaoni_bp = root.join("Blue Period");
        std::fs::create_dir_all(&mangaoni_bp).unwrap();
        std::fs::write(mangaoni_bp.join("Ch 86 Vol none Capítulo 86  manga_blue-period_782549.cbz"), b"").unwrap();

        let leer_dir = root.join("Keikenzumi");
        std::fs::create_dir_all(&leer_dir).unwrap();
        std::fs::write(leer_dir.join("Ch 28.1 Vol none Capitulo 28.1  _leer_abc_title_28.1_.epub"), b"").unwrap();

        let comizy_dir = root.join("Aftergod");
        std::fs::create_dir_all(&comizy_dir).unwrap();
        std::fs::write(comizy_dir.join("Ch 10 Vol none Chapter 10  kNx8eQNE_zj2mzgX5.pdf"), b"").unwrap();

        let raw_dir = root.join("Raw Series").join("Ch 1 Vol none Intro  raw_id_1");
        std::fs::create_dir_all(&raw_dir).unwrap();
        std::fs::write(raw_dir.join("0001.jpg"), b"").unwrap();

        // Depth 3: multi-language (English/ subfolder)
        let weeb_dir = root.join("Weeb Series").join("English");
        std::fs::create_dir_all(&weeb_dir).unwrap();
        std::fs::write(weeb_dir.join("Ch 48 Vol none  Weeb central 01J76XZ7JNB5QTARKZWHCBTF6T.cbz"), b"").unwrap();

        let pill_dir = root.join("Sakamoto Days").join("English");
        std::fs::create_dir_all(&pill_dir).unwrap();
        std::fs::write(pill_dir.join("Ch 234 Vol none   5281-10234000.cbz"), b"").unwrap();

        // Chapter ID containing spaces (fallback stem suffix matching)
        let space_dir = root.join("Space Series");
        std::fs::create_dir_all(&space_dir).unwrap();
        std::fs::write(space_dir.join("Ch 1 Vol none Title  spaced id 42.cbz"), b"").unwrap();

        let index = LocalDownloadsIndex::scan(root).expect("scan existing root");

        // All providers and formats match
        assert!(index.contains_chapter(MangaProviders::Mangaoni, "manga/one-piece|811902"));
        assert!(index.contains_chapter(MangaProviders::Mangaoni, "manga/blue-period|782549"));
        assert!(index.contains_chapter(MangaProviders::Leercapitulo, "/leer/abc/title/28.1/"));
        assert!(index.contains_chapter(MangaProviders::Comizy, "kNx8eQNE|zj2mzgX5"));
        assert!(index.contains_chapter(MangaProviders::Mangadex, "raw_id_1"));
        assert!(index.contains_chapter(MangaProviders::Weebcentral, "01J76XZ7JNB5QTARKZWHCBTF6T"));
        assert!(index.contains_chapter(MangaProviders::Mangapill, "chapters/5281-10234000/sakamoto-days-chapter-234"));
        assert!(index.contains_chapter(MangaProviders::Mangadex, "spaced id 42"));

        // Raw directory image file must NOT be treated as chapter token, while the Raw chapter folder itself matches
        assert!(!index.contains_chapter(MangaProviders::Mangadex, "0001.jpg"));

        // Exact suffix token matching prevents substring false positive for legacy unprefixed MangaOni ID
        assert!(!index.contains_chapter(MangaProviders::Mangaoni, "blue-period|782549"));
        assert!(!index.contains_chapter(MangaProviders::Weebcentral, "01M27CS4DQ1JVQDGARQJVK76EQ"));

        assert!(index.has_any_chapter(MangaProviders::Mangaoni, &[
            "manga/one-piece|missing".to_string(),
            "manga/one-piece|811902".to_string()
        ],));
        assert!(!index.has_any_chapter(MangaProviders::Weebcentral, &["01M27CS4DQ1JVQDGARQJVK76EQ".to_string()]));
    }
}
