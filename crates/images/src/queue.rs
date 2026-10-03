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

    /// Holds an image that is not a file (a remote resource, a QR code, …) for processing: the
    /// input that reads it. `name` is its file name, whose stem the processed images keep.
    /// Adding the same bytes again keeps one copy.
    #[must_use]
    pub fn add_memory(&self, name: &str, bytes: Arc<[u8]>) -> ImageInput {
        let memory = xxh3_64(&bytes);
        self.memory
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(memory)
            .or_insert(bytes);
        ImageInput::Memory(MemoryImage {
            memory,
            name: name.to_owned(),
        })
    }

    /// The bytes of a source (a file or an image in memory), what errors call it, and its file
    /// name.
    fn source_bytes(&self, input: &ImageInput) -> Result<(Arc<[u8]>, String, String), ImageError> {
        match input {
            ImageInput::File(path) => {
                let bytes = fs::read(path).map_err(|e| ImageError::io(path, e))?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Ok((bytes.into(), path.display().to_string(), name))
            }
            ImageInput::Memory(m) => {
                let bytes = self
                    .memory
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(&m.memory)
                    .cloned()
                    .ok_or_else(|| ImageError::UnknownMemory(m.name.clone()))?;
                let name = m.name.rsplit('/').next().unwrap_or_default().to_owned();
                Ok((bytes, m.name.clone(), name))
            }
            ImageInput::Op(id) => Err(ImageError::UnknownOp(*id)),
        }
    }

    /// The metadata of a source, a file or an image in memory (read once per input).
    fn source(&self, input: &ImageInput) -> Result<Arc<SourceMeta>, ImageError> {
        if let Some(m) = self
            .sources
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(input)
        {
            return Ok(Arc::clone(m));
        }
        let (bytes, what, name) = self.source_bytes(input)?;
        let (size, format) = codec::probe(&bytes, &what)?;
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (&*name, ""),
        };
        let meta = Arc::new(SourceMeta {
            hash: xxh3_64(&bytes),
            info: InputInfo {
                size,
                format,
                orientation: exif::orientation(&bytes),
            },
            stem: stem_of(stem).to_owned(),
            ext: ext.to_owned(),
        });
        self.sources
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(input.clone(), Arc::clone(&meta));
        Ok(meta)
    }

    /// Registers the bytes of a TrueType or OpenType font for text filters
    /// ([`FontInput::Registered`]). The id is the bytes' identity: registering the same bytes
    /// again returns the same id.
    ///
    /// # Errors
    /// Bytes that are not a usable font.
    pub fn add_font(&self, bytes: impl Into<Arc<[u8]>>) -> Result<FontId, ImageError> {
        let font = FontData::new(bytes.into());
        let id = font.id();
        let known = self
            .fonts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(&id);
        if !known {
            font.validate(&format!("font {id}"))?;
            self.fonts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id, font);
        }
        Ok(id)
    }

    /// The font of a text filter: the default one (Go Regular, as in Go), a registered one, or
    /// a font file (read once per path).
    fn font(&self, input: Option<&FontInput>) -> Result<FontData, ImageError> {
        let registered = |id: FontId| {
            self.fonts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&id)
                .cloned()
                .ok_or(ImageError::UnknownFont(id))
        };
        match input {
            None => Ok(FontData::go_regular()),
            Some(FontInput::Registered(id)) => registered(*id),
            Some(FontInput::File(path)) => {
                let known = self
                    .font_files
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(path)
                    .copied();
                if let Some(id) = known {
                    return registered(id);
                }
                let bytes = fs::read(path).map_err(|e| ImageError::io(path, e))?;
                let font = FontData::new(bytes.into());
                font.validate(&path.display().to_string())?;
                self.fonts
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .entry(font.id())
                    .or_insert_with(|| font.clone());
                self.font_files
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(path.clone(), font.id());
                Ok(font)
            }
        }
    }

    fn input_ref(&self, input: &ImageInput) -> Result<InputRef, ImageError> {
        let identity = match input {
            ImageInput::File(_) | ImageInput::Memory(_) => self.source(input)?.hash,
            ImageInput::Op(id) => self.op(*id)?.digest,
        };
        Ok(InputRef {
            input: input.clone(),
            identity,
        })
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

    /// Decoded pixels of an input; with `analysis`, also as the smart crop analysis reads
    /// them ([`codec::decode`]).
    fn pixels(&self, input: &ImageInput, analysis: bool) -> Result<codec::Decoded, ImageError> {
        match input {
            ImageInput::File(_) | ImageInput::Memory(_) => {
                let (bytes, what, _) = self.source_bytes(input)?;
                codec::decode(&bytes, &what, analysis)
            }
            ImageInput::Op(id) => {
                let op = self.op(*id)?;
                let bytes = self.encoded(*id)?;
                codec::decode(&bytes, &op.out.file_name, analysis)
            }
        }
    }

    /// The encoded result of an operation, processing it (and the operations it reads) if
    /// neither this build nor the file cache has it.
    ///
    /// # Errors
    /// An unknown id, an unreadable input, or a failing filter or encoder.
    pub fn encoded(&self, id: ImageOpId) -> Result<Arc<[u8]>, ImageError> {
        let op = self.op(id)?;
        if let Some(bytes) = op.result.get() {
            return Ok(Arc::clone(bytes));
        }
        let cached = self.cache.as_ref().and_then(|c| c.read(&op.out.file_name));
        let bytes: Arc<[u8]> = if let Some(bytes) = cached {
            bytes.into()
        } else {
            let smart = op
                .plan
                .steps
                .iter()
                .any(|s| matches!(s, Step::SmartCrop { .. }));
            let src = self.pixels(&op.input, smart)?;
            let load = |r: &InputRef| self.pixels(&r.input, false).map(|d| d.image);
            let regions = pixels::smart_regions(&src, &op.plan.steps);
            let img = pixels::run(src.image, &op.plan.steps, &load, &regions)?;
            let bytes = codec::encode(img, src.gray, &op.plan.encode)?;
            if let Some(cache) = &self.cache {
                cache.write(&op.out.file_name, &bytes)?;
            }
            bytes.into()
        };
        Ok(Arc::clone(op.result.get_or_init(|| bytes)))
    }

    /// Error `e` of operation `id` with the file it was processed for and what it read.
    fn for_target(&self, target: &OutputPath, id: ImageOpId, e: ImageError) -> ImageError {
        let Ok(op) = self.op(id) else {
            return e;
        };
        let input = match &op.input {
            ImageInput::File(p) => p.display().to_string(),
            ImageInput::Memory(m) => m.name.clone(),
            ImageInput::Op(i) => self.get(*i).map_or_else(|| i.to_string(), |e| e.file_name),
        };
        ImageError::Process {
            target: target.to_string(),
            width: op.out.width,
            height: op.out.height,
            format: op.out.format,
            input,
            source: Box::new(e),
        }
    }

    /// What [`process`](Self::process) computes for `ids`, in stages: each stage in parallel,
    /// after the stages of the operations it reads. Operations that differ only in their name
    /// share their pixels, so one of each digest is processed. The operations the wanted ones
    /// read are processed first (unless the wanted one has a result already): two
    /// operations that read one unprocessed operation would otherwise both process it.
    fn stages(&self, ids: &[ImageOpId]) -> Vec<Vec<ImageOpId>> {
        let mut stage_of: BTreeMap<ImageOpId, usize> = BTreeMap::new();
        for &id in ids {
            self.stage(id, &mut stage_of);
        }
        let mut firsts: BTreeMap<u64, (usize, ImageOpId)> = BTreeMap::new();
        for (&id, &stage) in &stage_of {
            if let Ok(op) = self.op(id) {
                firsts.entry(op.digest).or_insert((stage, id));
            }
        }
        let mut stages: Vec<Vec<ImageOpId>> = Vec::new();
        for (stage, id) in firsts.into_values() {
            if stages.len() <= stage {
                stages.resize_with(stage + 1, Vec::new);
            }
            stages[stage].push(id);
        }
        stages
    }

    /// The stage of operation `id` (recorded in `stage_of` with the operations it reads): 0
    /// when it reads no operation or has a result already (in this build or in the file
    /// cache), else one more than the latest stage of the operations it reads.
    fn stage(&self, id: ImageOpId, stage_of: &mut BTreeMap<ImageOpId, usize>) -> usize {
        if let Some(&s) = stage_of.get(&id) {
            return s;
        }
        let mut s = 0;
        if let Ok(op) = self.op(id) {
            let done = op.result.get().is_some()
                || self
                    .cache
                    .as_ref()
                    .is_some_and(|c| c.has(&op.out.file_name));
            if !done {
                for input in op.reads() {
                    s = s.max(self.stage(input, stage_of) + 1);
                }
            }
        }
        stage_of.insert(id, s);
        s
    }

    /// Processes the wanted operations in parallel and writes each result to its target.
    /// Must be called outside any render (build phase E6).
    ///
    /// # Errors
    /// The first failure in target order: processing (see [`ImageQueue::encoded`]) or
    /// writing to the sink.
    pub fn process(
        &self,
        wanted: &BTreeMap<OutputPath, ImageOpId>,
        sink: &dyn Sink,
    ) -> Result<(), ImageError> {
        let mut ids: Vec<ImageOpId> = wanted.values().copied().collect();
        ids.sort_unstable();
        ids.dedup();
        for stage in self.stages(&ids) {
            stage.into_par_iter().for_each(|id| {
                // Errors are reported below, in target order.
                let _ = self.encoded(id);
            });
        }
        let mut results: BTreeMap<ImageOpId, Result<Arc<[u8]>, ImageError>> = ids
            .into_par_iter()
            .map(|id| (id, self.encoded(id)))
            .collect::<Vec<_>>()
            .into_iter()
            .collect();
        for (target, id) in wanted {
            if let Some(Err(_)) = results.get(id)
                && let Some(Err(e)) = results.remove(id)
            {
                return Err(self.for_target(target, *id, e));
            }
            if let Some(Ok(bytes)) = results.get(id) {
                sink.write(target, bytes)
                    .map_err(|e| ImageError::io(target.as_str(), e))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ssg_testkit::fixture::repo_file;

    use super::*;

    fn spec(s: &str) -> ImageSpec {
        s.parse().expect("spec")
    }

    fn photo() -> ImageInput {
        ImageInput::File(repo_file("resources/testdata/sunset.jpg"))
    }

    fn sorted(mut ids: Vec<ImageOpId>) -> Vec<ImageOpId> {
        ids.sort_unstable();
        ids
    }

    /// Two operations reading one unprocessed operation: it is processed in an earlier stage,
    /// once, as is an overlay's image.
    #[test]
    fn stages_process_what_is_read_first_and_once() {
        let q = ImageQueue::new(Imaging::default(), None);
        let crop = q
            .enqueue(&photo(), Some(&spec("crop 200x200")), &[])
            .expect("crop");
        let small = q
            .enqueue(&ImageInput::Op(crop.id), Some(&spec("resize 50x")), &[])
            .expect("small");
        let large = q
            .enqueue(&ImageInput::Op(crop.id), Some(&spec("resize 100x")), &[])
            .expect("large");
        let mark = q
            .enqueue(&photo(), Some(&spec("resize 20x")), &[])
            .expect("mark");
        let marked = q
            .enqueue(
                &ImageInput::Op(large.id),
                None,
                &[ImageFilter::Overlay {
                    image: ImageInput::Op(mark.id),
                    x: 0,
                    y: 0,
                }],
            )
            .expect("marked");
        let stages = q.stages(&[small.id, marked.id]);
        assert_eq!(stages.len(), 3, "{stages:?}");
        assert_eq!(sorted(stages[0].clone()), sorted(vec![crop.id, mark.id]));
        assert_eq!(sorted(stages[1].clone()), sorted(vec![small.id, large.id]));
        assert_eq!(stages[2], [marked.id]);
    }

    /// A result in the file cache is read from there: what it reads is not processed.
    #[test]
    fn stages_skip_what_a_cached_result_reads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = ImageCache {
            dir: dir.path().to_owned(),
            max_age: MaxAge::Forever,
        };
        let q = ImageQueue::new(Imaging::default(), Some(cache));
        let crop = q
            .enqueue(&photo(), Some(&spec("crop 200x200")), &[])
            .expect("crop");
        let small = q
            .enqueue(&ImageInput::Op(crop.id), Some(&spec("resize 50x")), &[])
            .expect("small");
        assert_eq!(q.stages(&[small.id]), [vec![crop.id], vec![small.id]]);
        fs::write(dir.path().join(&small.file_name), b"cached").expect("write");
        assert_eq!(q.stages(&[small.id]), [vec![small.id]]);
    }
}
