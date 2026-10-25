//! Latest-only, read-only draft rendering. Never authorizes Save or Use.
use super::*;
use std::sync::{Arc, Condvar, Mutex};

type ResultSlot = (u64, Result<crate::tui::sample::Sample, String>);
struct Request {
    generation: u64,
    contrast_infeasible: bool,
    manifest: EnvironmentManifest,
    image: Option<Vec<u8>>,
    effects: crate::domain::WallpaperEffects,
    area: Rect,
}
#[derive(Default)]
struct Mailbox {
    pending: Option<Request>,
    completed: Option<ResultSlot>,
    closed: bool,
}

pub(super) struct Preview {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    key: Option<(String, Rect)>,
    generation: u64,
    pub sample: Option<crate::tui::sample::Sample>,
    pub status: String,
}
impl Preview {
    pub fn new() -> io::Result<Self> {
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let worker = Arc::clone(&mailbox);
        std::thread::Builder::new()
            .name("editor-preview".into())
            .spawn(move || {
                loop {
                    let request = {
                        let mut state = worker.0.lock().unwrap();
                        while state.pending.is_none() && !state.closed {
                            state = worker.1.wait(state).unwrap();
                        }
                        if state.closed {
                            return;
                        }
                        state.pending.take().unwrap()
                    };
                    let result = prepare(&request);
                    let mut state = worker.0.lock().unwrap();
                    if state.closed {
                        return;
                    }
                    state.completed = Some((request.generation, result));
                }
            })?;
        Ok(Self {
            mailbox,
            key: None,
            generation: 0,
            sample: None,
            status: String::new(),
        })
    }

    pub fn request(
        &mut self,
        editor: &ProfileEditor,
        manifest: &EnvironmentManifest,
        area: Rect,
    ) -> Result<bool, WorkflowError> {
        let key = (editor.draft().document().to_owned(), area);
        if self.key.as_ref() == Some(&key) {
            return Ok(false);
        }
        let effects = match editor.intent()?.wallpaper {
            Some(WallpaperIntent::Source { effects, .. }) => effects,
            _ => None,
        }
        .unwrap_or(crate::domain::WallpaperEffects { blur: 0, darken: 0 });
        self.generation += 1;
        self.key = Some(key);
        self.sample = None;
        self.status = "Preparing preview · cell fallback".into();
        let mut state = self.mailbox.0.lock().unwrap();
        state.completed = None;
        // One running render plus one replaceable pending snapshot; sources are <=384x256.
        state.pending = Some(Request {
            generation: self.generation,
            contrast_infeasible: editor.palette_contrast_infeasible()?,
            manifest: manifest.clone(),
            image: editor.preview_image_source().map(<[u8]>::to_vec),
            effects,
            area,
        });
        self.mailbox.1.notify_one();
        Ok(true)
    }

    pub fn poll(&mut self) -> bool {
        let completed = self.mailbox.0.lock().unwrap().completed.take();
        let Some((generation, result)) = completed else {
            return false;
        };
        if generation != self.generation {
            return false;
        }
        match result {
            Ok(sample) => {
                self.status = if sample.thumbnail.is_none() {
                    "No image · illustrative sample"
                } else if TerminalGraphics::from_environment() == TerminalGraphics::Ghostty {
                    "Image preview · reduced source · v details"
                } else {
                    "Cell fallback · graphics unavailable"
                }
                .into();
                self.sample = Some(sample);
            }
            Err(error) => {
                self.sample = None;
                self.status = format!("Preview failed; cell fallback: {error}");
            }
        }
        true
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        let mut state = self.mailbox.0.lock().unwrap();
        state.closed = true;
        state.pending = None;
        self.mailbox.1.notify_one();
    }
}
fn prepare(request: &Request) -> Result<crate::tui::sample::Sample, String> {
    let image = request
        .image
        .as_deref()
        .map(|bytes| {
            crate::wallpaper_effects::render(bytes, request.effects, true)
                .map_err(|_| "cannot render reduced wallpaper".to_owned())
        })
        .transpose()?;
    let mut sample =
        crate::tui::sample::Sample::for_preview(request.manifest.clone(), image.as_deref())
            .map_err(|e| e.to_string())?;
    sample.contrast_infeasible = request.contrast_infeasible;
    sample
        .prepare_graphics(request.area)
        .map_err(|e| e.to_string())?;
    Ok(sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_matches_browser_pipeline_and_rejects_obsolete_results() {
        let image =
            image::RgbImage::from_fn(96, 64, |x, y| image::Rgb([x as u8 * 2, y as u8 * 3, 120]));
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let request = Request {
            generation: 2,
            contrast_infeasible: false,
            manifest: EnvironmentManifest::new(
                Some(crate::domain::WallpaperManifest::Image(
                    crate::domain::ImageWallpaper::new(
                        crate::domain::Sha256Digest::from_bytes([0; 32]),
                        crate::domain::MediaType::Png,
                    )
                    .with_opacity(crate::domain::OpacityMillionths::new(650_000).unwrap()),
                )),
                Some(crate::domain::ColorsManifest::new(
                    "101820".parse().unwrap(),
                    "ffffff".parse().unwrap(),
                    ["aaaaaa".parse().unwrap(); 16],
                )),
                None,
            ),
            image: Some(png.into_inner()),
            effects: crate::domain::WallpaperEffects {
                blur: 8,
                darken: 60,
            },
            area: Rect::new(44, 2, 56, 25),
        };
        let sample = prepare(&request).unwrap();
        let transformed = crate::wallpaper_effects::render(
            request.image.as_deref().unwrap(),
            request.effects,
            true,
        )
        .unwrap();
        let mut browser =
            crate::tui::sample::Sample::for_preview(request.manifest.clone(), Some(&transformed))
                .unwrap();
        browser.prepare_graphics(request.area).unwrap();
        let (mut actual, mut expected) = (Vec::new(), Vec::new());
        assert!(sample.render_graphics(&mut actual, request.area).unwrap());
        browser
            .render_graphics(&mut expected, request.area)
            .unwrap();
        assert_eq!(actual, expected);

        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let mut preview = Preview {
            mailbox,
            key: None,
            generation: 2,
            sample: None,
            status: String::new(),
        };
        preview.mailbox.0.lock().unwrap().completed = Some((1, Ok(sample.clone())));
        assert!(!preview.poll());
        assert!(preview.sample.is_none());
        preview.mailbox.0.lock().unwrap().completed = Some((2, Ok(sample)));
        assert!(preview.poll());
        assert!(preview.sample.is_some());
        preview.mailbox.0.lock().unwrap().completed = Some((2, Err("injected failure".into())));
        assert!(preview.poll());
        assert!(preview.sample.is_none());
        assert!(preview.status.contains("cell fallback: injected failure"));
        assert!(
            prepare(&Request {
                image: Some(vec![0; 10]),
                ..request
            })
            .is_err()
        );
    }
}
