//! Processing: decoding, the stages of a plan, and encoding for a target.

use super::*;

impl ImageQueue {
    /// Decoded pixels of an input; with `analysis`, also as the smart crop analysis reads
    /// them ([`codec::decode`]).
    pub(super) fn pixels(
        &self,
        input: &ImageInput,
        analysis: bool,
    ) -> Result<codec::Decoded, ImageError> {
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
    pub(super) fn for_target(
        &self,
        target: &OutputPath,
        id: ImageOpId,
        e: ImageError,
    ) -> ImageError {
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
    pub(super) fn stages(&self, ids: &[ImageOpId]) -> Vec<Vec<ImageOpId>> {
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
    pub(super) fn stage(&self, id: ImageOpId, stage_of: &mut BTreeMap<ImageOpId, usize>) -> usize {
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
