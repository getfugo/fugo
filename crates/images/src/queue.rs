//! The deferred image queue and its file cache (`[caches.images]`).
//!
//! [`ImageQueue::enqueue`] plans an operation from metadata only and returns its final name
//! and size at once, so templates can print `.Width` and `.RelPermalink` without decoding
//! pixels (a smart crop whose size depends on its region is the exception: it analyses the
//! source while planning). The pixels are produced later, in parallel and outside any render, by
//! [`ImageQueue::process`] (build phase E6), or on demand by [`ImageQueue::encoded`].

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, SystemTime};

use rayon::prelude::*;
use ssg_base::paths::OutputPath;
use ssg_base::{ImageOpId, Sink};
use ssg_config::global::{FileCache, MaxAge};
use xxhash_rust::xxh3::xxh3_64;

use crate::codec;
use crate::error::ImageError;
use crate::exif;
use crate::filter::{ImageFilter, ImageInput, MemoryImage};
use crate::font::{FontData, FontId};
use crate::format::ImageFormat;
use crate::pixels;
use crate::plan::{InputInfo, InputRef, Plan, Step};
use crate::settings::Imaging;
use crate::smartcrop;
use crate::spec::ImageSpec;
use crate::text::FontInput;

mod process;
mod sources;

/// The result of [`ImageQueue::enqueue`]: known before any pixel is processed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Enqueued {
    /// Identifies the operation *and* its name: the same content and operation under two
    /// source names (identical bundle images) are two operations that share their pixels.
    pub id: ImageOpId,
    /// `<source stem>_hu_<16 hex digits>.<ext>`, as in Go: the stem and extension are the
    /// source's own and the digits hash the source content and the planned operation.
    pub file_name: String,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
}

/// Where processed images are kept between builds.
#[derive(Clone, Debug)]
pub struct ImageCache {
    pub dir: PathBuf,
    pub max_age: MaxAge,
}

impl ImageCache {
    /// The `images` cache of `[caches]`.
    #[must_use]
    pub fn from_config(cache: &FileCache) -> Self {
        Self {
            dir: cache.path.clone(),
            max_age: cache.max_age,
        }
    }

    fn path(&self, file_name: &str) -> PathBuf {
        self.dir.join(file_name)
    }

    /// Whether the cache has `file_name`, not older than the maximum age.
    fn has(&self, file_name: &str) -> bool {
        let Ok(meta) = fs::metadata(self.path(file_name)) else {
            return false;
        };
        match self.max_age {
            MaxAge::Forever => meta.is_file(),
            MaxAge::For(age) if age == Duration::ZERO => false,
            MaxAge::For(age) => meta.modified().is_ok_and(|modified| {
                SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default()
                    <= age
            }),
        }
    }

    /// The cached bytes, when present and not older than the maximum age.
    fn read(&self, file_name: &str) -> Option<Vec<u8>> {
        if !self.has(file_name) {
            return None;
        }
        fs::read(self.path(file_name)).ok()
    }

    fn write(&self, file_name: &str, bytes: &[u8]) -> Result<(), ImageError> {
        /// Makes the temporary names of one process unique.
        static TMP: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(&self.dir).map_err(|e| ImageError::io(&self.dir, e))?;
        let path = self.path(file_name);
        // Write then rename, so a concurrent reader never sees a partial file. The temporary
        // name is the writer's own: two writers of one name (two builds sharing the cache)
        // must not write into one temporary file and rename it from under each other.
        let tmp = self.dir.join(format!(
            ".{file_name}.{}-{}.tmp",
            std::process::id(),
            TMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&tmp, bytes).map_err(|e| ImageError::io(&tmp, e))?;
        fs::rename(&tmp, &path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            ImageError::io(&path, e)
        })
    }
}

/// What the queue knows about a source file.
struct SourceMeta {
    /// xxh3 of the file's bytes.
    hash: u64,
    info: InputInfo,
    stem: String,
    /// The extension as spelled, with its dot (`.JPG`), or empty.
    ext: String,
}

/// A processed result, shared by the operations that differ only in their name.
type SharedResult = Arc<OnceLock<Arc<[u8]>>>;

struct Op {
    input: ImageInput,
    plan: Plan,
    out: Enqueued,
    /// The extension of the result as spelled in `out.file_name`.
    ext: String,
    stem: String,
    /// The hash of the source content and the operation (the digits of the name).
    digest: u64,
    /// Shared by every operation with the same `digest`: identical bytes are processed once.
    result: SharedResult,
    /// The EXIF orientation of the original source: like Go, a processed image keeps its
    /// source's EXIF data for `auto_orient` (the encoded result carries none).
    orientation: Option<u8>,
}

impl Op {
    /// The operations whose results this one reads: its input, the images of its overlays and
    /// masks.
    fn reads(&self) -> impl Iterator<Item = ImageOpId> + '_ {
        let steps = self.plan.steps.iter().filter_map(|s| match s {
            Step::Overlay { image, .. } | Step::Mask { image } => Some(&image.input),
            _ => None,
        });
        std::iter::once(&self.input)
            .chain(steps)
            .filter_map(|i| match i {
                ImageInput::Op(id) => Some(*id),
                ImageInput::File(_) | ImageInput::Memory(_) => None,
            })
    }
}

/// Queued image operations, shared by every render of a build.
pub struct ImageQueue {
    imaging: Imaging,
    cache: Option<ImageCache>,
    ops: Mutex<BTreeMap<ImageOpId, Arc<Op>>>,
    /// Sources (files and images in memory) by input.
    sources: Mutex<BTreeMap<ImageInput, Arc<SourceMeta>>>,
    /// The bytes of the images in memory, by their xxh3.
    memory: Mutex<BTreeMap<u64, Arc<[u8]>>>,
    results: Mutex<BTreeMap<u64, SharedResult>>,
    /// Fonts of text filters, by content.
    fonts: Mutex<BTreeMap<FontId, FontData>>,
    font_files: Mutex<BTreeMap<PathBuf, FontId>>,
}

fn stem_of(name: &str) -> &str {
    // A processed image keeps the name of its source: `a_hu_1234.jpg` → `a`.
    name.rsplit_once("_hu_").map_or(name, |(s, _)| s)
}

impl ImageQueue {
    /// A queue with the site's `[imaging]` defaults and, unless `None`, the `[caches.images]`
    /// file cache.
    #[must_use]
    pub fn new(imaging: Imaging, cache: Option<ImageCache>) -> Self {
        Self {
            imaging,
            cache,
            ops: Mutex::new(BTreeMap::new()),
            sources: Mutex::new(BTreeMap::new()),
            memory: Mutex::new(BTreeMap::new()),
            results: Mutex::new(BTreeMap::new()),
            fonts: Mutex::new(BTreeMap::new()),
            font_files: Mutex::new(BTreeMap::new()),
        }
    }

    /// The `[imaging]` settings.
    #[must_use]
    pub fn imaging(&self) -> &Imaging {
        &self.imaging
    }

    fn op(&self, id: ImageOpId) -> Result<Arc<Op>, ImageError> {
        self.ops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&id)
            .cloned()
            .ok_or(ImageError::UnknownOp(id))
    }

    /// Plans `spec` (if any) followed by `filters` on `input` and queues it. Returns the
    /// result's name and size without processing pixels. Enqueueing the same operation twice
    /// returns the same id.
    ///
    /// # Errors
    /// An unreadable source or one whose format is unknown, an unknown input operation or
    /// font, a font that cannot be used, or an operation whose result would be empty.
    pub fn enqueue(
        &self,
        input: &ImageInput,
        spec: Option<&ImageSpec>,
        filters: &[ImageFilter],
    ) -> Result<Enqueued, ImageError> {
        let (info, identity, stem, ext) = match input {
            ImageInput::File(_) | ImageInput::Memory(_) => {
                let m = self.source(input)?;
                let info = InputInfo {
                    size: m.info.size,
                    format: m.info.format,
                    orientation: m.info.orientation,
                };
                (info, m.hash, m.stem.clone(), m.ext.clone())
            }
            ImageInput::Op(id) => {
                let op = self.op(*id)?;
                let info = InputInfo {
                    size: (op.out.width, op.out.height),
                    format: op.out.format,
                    orientation: op.orientation,
                };
                (info, op.digest, op.stem.clone(), op.ext.clone())
            }
        };
        let orientation = info.orientation;
        let plan = Plan::new(
            &info,
            spec,
            filters,
            &self.imaging,
            &mut |i| self.input_ref(i),
            &mut |f| self.font(f),
            &mut |target, filter| {
                let src = self.pixels(input, true)?;
                Ok(smartcrop::find(&src.source(), target.0, target.1, filter))
            },
        )?;
        let hash = xxh3_64(format!("{identity:016x}|{}", plan.key()).as_bytes());
        let format = plan.encode.format;
        // Keep the source's spelling (`.JPEG`) when it names the result's format.
        let ext = if ImageFormat::from_extension(&ext) == Some(format) {
            ext
        } else {
            format.extension().to_owned()
        };
        let file_name = format!("{stem}_hu_{hash:016x}{ext}");
        // The name is part of the identity: keyed by the digits alone, identical images in
        // two bundles would share whichever name was queued first (non-deterministic).
        let id = xxh3_64(format!("{hash:016x}|{file_name}").as_bytes());
        let out = Enqueued {
            id: ImageOpId::from_raw(id),
            file_name,
            width: plan.size.0,
            height: plan.size.1,
            format,
        };
        let mut ops = self.ops.lock().unwrap_or_else(PoisonError::into_inner);
        let op = ops.entry(out.id).or_insert_with(|| {
            let result = Arc::clone(
                self.results
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .entry(hash)
                    .or_default(),
            );
            Arc::new(Op {
                input: input.clone(),
                plan,
                out,
                ext,
                stem,
                digest: hash,
                result,
                orientation,
            })
        });
        Ok(op.out.clone())
    }

    /// The description of a queued operation.
    #[must_use]
    pub fn get(&self, id: ImageOpId) -> Option<Enqueued> {
        self.op(id).ok().map(|op| op.out.clone())
    }

    /// The number of queued operations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether nothing is queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests;
