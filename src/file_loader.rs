use egui::{
    ahash::HashMap,
    load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError},
    mutex::Mutex,
};
use std::{sync::Arc, task::Poll};

#[derive(Clone)]
struct File {
    bytes: Arc<[u8]>,
    mime: Option<String>,
}

type Entry = Poll<Result<File, String>>;

#[derive(Default)]
pub struct BlockingFileLoader {
    /// Cache for loaded files
    cache: Arc<Mutex<HashMap<String, Entry>>>,
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

        let mut cache = self.cache.lock();
        if let Some(entry) = cache.get(path).cloned() {
            // `path` has either begun loading, is loaded, or has failed to load.
            match entry {
                Poll::Ready(Ok(file)) => Ok(BytesPoll::Ready {
                    size: None,
                    bytes: Bytes::Shared(file.bytes),
                    mime: file.mime,
                }),
                Poll::Ready(Err(err)) => Err(LoadError::Loading(err)),
                Poll::Pending => Ok(BytesPoll::Pending { size: None }),
            }
        } else {
            log::trace!("started loading {uri:?}");
            // We need to load the file at `path`.

            // Set the file to `pending` until we finish loading it.
            let path = path.to_owned();
            cache.insert(path.clone(), Poll::Pending);
            drop(cache);

            let ctx = ctx.clone();
            let _uri = uri.to_owned();
            let result = match std::fs::read(&path) {
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
    }

    fn forget(&self, uri: &str) {
        let _ = self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .values()
            .map(|entry| match entry {
                Poll::Ready(Ok(file)) => {
                    file.bytes.len() + file.mime.as_ref().map_or(0, |m| m.len())
                }
                Poll::Ready(Err(err)) => err.len(),
                _ => 0,
            })
            .sum()
    }
}
