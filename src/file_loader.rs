use egui::{
    load::{
        Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError, SizedTexture, TextureLoader,
        TexturePoll,
    },
    Context,
};
use std::{sync::Arc, task::Poll};

#[derive(Clone)]
struct File {
    bytes: Arc<[u8]>,
    mime: Option<String>,
}

#[derive(Default)]
pub struct BlockingFileLoader {
    // / Cache for loaded files
    // cache: Arc<Mutex<HashMap<String, Entry>>>,
}

impl BlockingFileLoader {
    pub const ID: &'static str = egui::generate_loader_id!(FileLoader);
}

const PROTOCOL: &str = "file://";

impl BytesLoader for BlockingFileLoader {
    fn id(&self) -> &str {
        Self::ID
    }

    fn load(&self, ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        // File loader only supports the `file` protocol.
        let Some(path) = uri.strip_prefix(PROTOCOL) else {
            return Err(LoadError::NotSupported);
        };

        log::trace!("started loading {uri:?}");
        // We need to load the file at `path`.

        // Set the file to `pending` until we finish loading it.
        let path = path.to_owned();

        let ctx = ctx.clone();
        let _uri = uri.to_owned();
        let result = match std::fs::read(path) {
            Ok(bytes) => {
                // #[cfg(feature = "mime_guess")]
                // let mime = mime_guess2::from_path(&path)
                //     .first_raw()
                //     .map(|v| v.to_owned());

                // #[cfg(not(feature = "mime_guess"))]
                // let mime = None;

                Ok(File {
                    bytes: bytes.into(),
                    mime: None,
                })
            }
            Err(err) => Err(err.to_string()),
        };
        // let prev = cache.lock().insert(path, Poll::Ready(result));
        // assert!(matches!(prev, Some(Poll::Pending)));
        ctx.request_repaint();
        log::trace!("finished loading {_uri:?}");
        // })
        // .expect("failed to spawn thread");
        let poll = Poll::Ready(result);
        match poll {
            Poll::Ready(Ok(file)) => Ok(BytesPoll::Ready {
                size: None,
                bytes: Bytes::Shared(file.bytes),
                mime: file.mime,
            }),
            Poll::Ready(Err(err)) => Err(LoadError::Loading(err)),
            _ => unreachable!(),
        }
    }

    fn forget(&self, _uri: &str) {
        // let _ = self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        // self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        0
        // self.cache
        //     .lock()
        //     .values()
        //     .map(|entry| match entry {
        //         Poll::Ready(Ok(file)) => {
        //             file.bytes.len() + file.mime.as_ref().map_or(0, |m| m.len())
        //         }
        //         Poll::Ready(Err(err)) => err.len(),
        //         _ => 0,
        //     })
        //     .sum()
    }
}

use egui::load::{ImageLoadResult, ImageLoader, ImagePoll, SizeHint};
use std::path::Path;

// type Entry = Result<Arc<ColorImage>, String>;

#[derive(Default)]
pub struct ImageCrateLoader {
    // cache: Mutex<HashMap<String, Entry>>,
}

impl ImageCrateLoader {
    pub const ID: &'static str = egui::generate_loader_id!(ImageCrateLoader);
}

fn is_supported_uri(uri: &str) -> bool {
    let Some(ext) = Path::new(uri).extension().and_then(|ext| ext.to_str()) else {
        // `true` because if there's no extension, assume that we support it
        return true;
    };

    ext != "svg"
}

fn is_unsupported_mime(mime: &str) -> bool {
    mime.contains("svg")
}

impl ImageLoader for ImageCrateLoader {
    fn id(&self) -> &str {
        Self::ID
    }

    fn load(&self, ctx: &egui::Context, uri: &str, _: SizeHint) -> ImageLoadResult {
        // three stages of guessing if we support loading the image:
        // 1. URI extension
        // 2. Mime from `BytesPoll::Ready`
        // 3. image::guess_format

        // (1)
        if !is_supported_uri(uri) {
            return Err(LoadError::NotSupported);
        }

        // let mut cache = self.cache.lock();
        // if let Some(entry) = cache.get(uri).cloned() {
        //     match entry {
        //         Ok(image) => Ok(ImagePoll::Ready { image }),
        //         Err(err) => Err(LoadError::Loading(err)),
        //     }
        // } else {
        match ctx.try_load_bytes(uri) {
            Ok(BytesPoll::Ready { bytes, mime, .. }) => {
                // (2 and 3)
                if mime.as_deref().is_some_and(is_unsupported_mime)
                    || image::guess_format(&bytes).is_err()
                {
                    return Err(LoadError::NotSupported);
                }

                log::trace!("started loading {uri:?}");
                let result = egui_extras::image::load_image_bytes(&bytes).map(Arc::new);
                log::trace!("finished loading {uri:?}");
                // cache.insert(uri.into(), result.clone());
                match result {
                    Ok(image) => Ok(ImagePoll::Ready { image }),
                    Err(err) => Err(LoadError::Loading(err)),
                }
            }
            Ok(BytesPoll::Pending { size }) => Ok(ImagePoll::Pending { size }),
            Err(err) => Err(err),
        }
        // }
    }

    fn forget(&self, _uri: &str) {
        // let _ = self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        // self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        0
        // self.cache
        //     .lock()
        //     .values()
        //     .map(|result| match result {
        //         Ok(image) => image.pixels.len() * size_of::<egui::Color32>(),
        //         Err(err) => err.len(),
        //     })
        //     .sum()
    }
}

#[derive(Default)]
pub struct BoundrsTextureLoader {
    // cache: Mutex<HashMap<(String, TextureOptions), TextureHandle>>,
}

impl TextureLoader for BoundrsTextureLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(DefaultTextureLoader)
    }

    fn load(
        &self,
        ctx: &Context,
        uri: &str,
        texture_options: egui::TextureOptions,
        size_hint: SizeHint,
    ) -> egui::load::TextureLoadResult {
        // let mut cache = self.cache.lock();
        match ctx.try_load_image(uri, size_hint)? {
            ImagePoll::Pending { size } => Ok(TexturePoll::Pending { size }),
            ImagePoll::Ready { image } => {
                let handle = ctx.load_texture(uri, image, texture_options);
                let texture = SizedTexture::from_handle(&handle);
                // cache.insert((uri.into(), texture_options), handle);
                Ok(TexturePoll::Ready { texture })
            }
        }
    }

    fn forget(&self, _uri: &str) {
        // self.cache.lock().retain(|(u, _), _| u != uri);
    }

    fn forget_all(&self) {
        // #[cfg(feature = "log")]
        // log::trace!("forget all");

        // self.cache.lock().clear();
    }

    fn end_frame(&self, _: usize) {}

    fn byte_size(&self) -> usize {
        0
        // self.cache
        //     .lock()
        //     .values()
        //     .map(|texture| texture.byte_size())
        //     .sum()
    }
}
