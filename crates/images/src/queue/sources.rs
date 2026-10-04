//! Sources and fonts: loading them once, and the inputs of steps.

use super::*;

impl ImageQueue {
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
    pub(super) fn source_bytes(
        &self,
        input: &ImageInput,
    ) -> Result<(Arc<[u8]>, String, String), ImageError> {
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
    pub(super) fn source(&self, input: &ImageInput) -> Result<Arc<SourceMeta>, ImageError> {
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
    pub(super) fn font(&self, input: Option<&FontInput>) -> Result<FontData, ImageError> {
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

    pub(super) fn input_ref(&self, input: &ImageInput) -> Result<InputRef, ImageError> {
        let identity = match input {
            ImageInput::File(_) | ImageInput::Memory(_) => self.source(input)?.hash,
            ImageInput::Op(id) => self.op(*id)?.digest,
        };
        Ok(InputRef {
            input: input.clone(),
            identity,
        })
    }
}
