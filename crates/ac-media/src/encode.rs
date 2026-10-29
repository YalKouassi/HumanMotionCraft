//! A video file from a run of RGBA frames: MP4 (H.264 through ffmpeg, which must be installed)
//! or animated GIF (in process, no tools needed). The encoding is `ac_render::anim`'s, the one
//! encoder MCP's `animation_preview` also uses; this adds the file: the format from its extension,
//! and the GIF written atomically once finished.

use std::path::{Path, PathBuf};

use ac_render::anim::{GifWriter, Mp4Writer, delay_cs};

pub use ac_render::anim::ffmpeg;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Mp4,
    Gif,
}

impl Format {
    /// The format a file name asks for, by its extension.
    pub fn of(path: &Path) -> Result<Format, String> {
        match path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref()
        {
            Some("mp4") => Ok(Format::Mp4),
            Some("gif") => Ok(Format::Gif),
            _ => Err(format!("{}: the file must end in .mp4 or .gif", path.display())),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Mp4 => "mp4",
            Format::Gif => "gif",
        }
    }
}

enum Sink {
    Mp4(Mp4Writer),
    Gif(GifWriter),
}

/// Frames in, a video file out. Sizes are checked here; MP4 sizes must be even (x264).
pub struct Encoder {
    sink: Sink,
    path: PathBuf,
}

impl Encoder {
    /// Start a video of `w` x `h` frames at `fps` into `path` (its extension picks the format).
    pub fn create(path: &Path, w: u32, h: u32, fps: f64) -> Result<Encoder, String> {
        let format = Format::of(path)?;
        if !(fps.is_finite() && fps > 0.0 && fps <= 240.0) {
            return Err(format!("{fps} frames per second: 0 to 240"));
        }
        if w == 0 || h == 0 {
            return Err(format!("a {w}x{h} video has no pixels"));
        }
        let sink = match format {
            Format::Mp4 => Sink::Mp4(Mp4Writer::create(path, w, h, fps)?),
            // GIF delays are in hundredths of a second
            Format::Gif => Sink::Gif(GifWriter::new(w, h, delay_cs(fps))?),
        };
        Ok(Encoder {
            sink,
            path: path.to_path_buf(),
        })
    }

    pub fn format(&self) -> Format {
        match self.sink {
            Sink::Gif(_) => Format::Gif,
            Sink::Mp4(_) => Format::Mp4,
        }
    }

    /// Add a frame: RGBA rows of the encoder's size.
    pub fn push(&mut self, rgba: &[u8]) -> Result<(), String> {
        match &mut self.sink {
            Sink::Mp4(m) => m.push(rgba),
            Sink::Gif(g) => g.push(rgba),
        }
    }

    /// Close the file; returns how many frames it holds.
    pub fn finish(self) -> Result<usize, String> {
        match self.sink {
            Sink::Mp4(m) => m.finish(),
            Sink::Gif(g) => {
                let frames = g.frames();
                let bytes = g.finish()?;
                if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| format!("can't create {}: {e}", dir.display()))?;
                }
                craft_atomic::atomic_write(&self.path, &bytes)
                    .map_err(|e| format!("can't write {}: {e}", self.path.display()))?;
                Ok(frames)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_gif_of_two_frames_starts_with_its_signature() {
        let dir = std::env::temp_dir().join(format!("ac-media-gif-{}", std::process::id()));
        let path = dir.join("two.gif");
        let mut e = Encoder::create(&path, 4, 2, 10.0).unwrap();
        e.push(&[200u8; 32]).unwrap();
        e.push(&[20u8; 32]).unwrap();
        assert!(e.push(&[0u8; 31]).is_err());
        assert_eq!(e.finish().unwrap(), 2);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..6], b"GIF89a");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn formats_come_from_the_extension() {
        assert_eq!(Format::of(Path::new("a/b.MP4")), Ok(Format::Mp4));
        assert_eq!(Format::of(Path::new("b.gif")), Ok(Format::Gif));
        assert!(Format::of(Path::new("b.glb")).is_err());
        assert!(Encoder::create(Path::new("x.mp4"), 3, 2, 30.0).is_err());
    }
}
