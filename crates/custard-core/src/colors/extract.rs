//! Turning a wallpaper into a seed color.
//!
//! ffmpeg decodes everything — stills, GIFs, videos — straight to a small
//! block of raw pixels, so there is one path for every format and no image
//! library to keep in step with it. A video is sampled at several points
//! across its length, so one dark scene does not decide the palette for the
//! whole clip.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use material_colors::color::Rgb;
use material_colors::hct::Hct;
use material_colors::quantize::{Quantizer, QuantizerCelebi};
use material_colors::score::Score;

use super::scheme::{Cast, Hue};
use crate::error::{Error, Result};

/// The side of the square each frame is shrunk to. Colors, not detail, are
/// what is wanted, and the quantizer's time grows with the pixel count.
const SIDE: usize = 96;

/// How many frames a video is sampled at.
pub const VIDEO_FRAMES: usize = 6;

/// How many files a folder is sampled from, at most.
pub const FOLDER_FILES: usize = 8;

/// Extensions treated as video. Anything else ffmpeg can open is read as a
/// still, its first frame — which is right for a GIF too.
const VIDEO: &[&str] =
    &["mp4", "mkv", "webm", "mov", "avi", "m4v", "wmv", "flv", "ogv", "mpg", "mpeg", "ts"];

/// Extensions a wallpaper folder's files may have.
const MEDIA: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "avif", "jxl", "mp4", "mkv", "webm",
    "mov", "avi", "m4v", "wmv", "flv", "ogv", "mpg", "mpeg", "ts",
];

/// Whether a path is a video, going by its extension.
pub fn is_video(path: &Path) -> bool {
    has_extension(path, VIDEO)
}

/// Whether a path looks like something a wallpaper program shows.
pub fn is_media(path: &Path) -> bool {
    has_extension(path, MEDIA)
}

fn has_extension(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| list.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Whether ffmpeg is installed.
pub fn have_ffmpeg() -> bool {
    crate::paths::which("ffmpeg").is_some()
}

/// The pixels of a still, or of several frames across a video.
pub fn sample(path: &Path) -> Result<Vec<Rgb>> {
    if !have_ffmpeg() {
        return Err(Error::other(
            "ffmpeg is needed to read the wallpaper, and it is not installed",
        ));
    }
    if !path.is_file() {
        return Err(Error::other(format!("{} is not a file", crate::paths::contract(path))));
    }
    if !is_video(path) {
        return frame(path, None);
    }
    let length = duration(path).unwrap_or(0.0);
    if length <= 0.0 {
        return frame(path, None);
    }
    // The frames are independent, so they are grabbed side by side: the
    // whole sample then takes about as long as one frame.
    let frames: Vec<Result<Vec<Rgb>>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..VIDEO_FRAMES)
            .map(|i| {
                let at = length * (i as f64 + 0.5) / VIDEO_FRAMES as f64;
                scope.spawn(move || frame(path, Some(at)))
            })
            .collect();
        jobs.into_iter()
            .map(|j| j.join().unwrap_or_else(|_| Err(Error::other("ffmpeg thread failed"))))
            .collect()
    });
    // A frame that will not decode is skipped rather than failing the lot;
    // only none at all is an error.
    let pixels: Vec<Rgb> = frames.into_iter().flatten().flatten().collect();
    if pixels.is_empty() {
        return frame(path, None);
    }
    Ok(pixels)
}

/// The media files in a folder, sorted, at most `limit` of them spread evenly
/// through the list.
pub fn folder_files(dir: &Path, limit: usize) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file() && is_media(p))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    if files.len() <= limit || limit == 0 {
        return files;
    }
    (0..limit).map(|i| files[i * files.len() / limit].clone()).collect()
}

/// Pixels from across a whole folder: one frame from the middle of each of
/// up to [`FOLDER_FILES`] files.
pub fn sample_folder(dir: &Path) -> Result<Vec<Rgb>> {
    if !have_ffmpeg() {
        return Err(Error::other(
            "ffmpeg is needed to read the wallpaper, and it is not installed",
        ));
    }
    let files = folder_files(dir, FOLDER_FILES);
    if files.is_empty() {
        return Err(Error::other(format!(
            "{} has no images or videos in it",
            crate::paths::contract(dir)
        )));
    }
    let frames: Vec<Result<Vec<Rgb>>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = files
            .iter()
            .map(|f| {
                scope.spawn(move || {
                    let at = if is_video(f) { duration(f).map(|d| d / 2.0) } else { None };
                    frame(f, at)
                })
            })
            .collect();
        jobs.into_iter()
            .map(|j| j.join().unwrap_or_else(|_| Err(Error::other("ffmpeg thread failed"))))
            .collect()
    });
    let pixels: Vec<Rgb> = frames.into_iter().flatten().flatten().collect();
    if pixels.is_empty() {
        return Err(Error::other(format!(
            "none of the files in {} could be read",
            crate::paths::contract(dir)
        )));
    }
    Ok(pixels)
}

/// One frame as raw pixels, shrunk to [`SIDE`] square.
///
/// A video is seeked to `at`, decoding keyframes only: ffmpeg then does not
/// decode its way forward to the exact time, which on a 4K clip is the
/// difference between ~0.3s and ~0.9s a frame, and which frame it lands on
/// does not matter here. A clip with few keyframes can come back empty that
/// way, so an ordinary seek is the fallback.
fn frame(path: &Path, at: Option<f64>) -> Result<Vec<Rgb>> {
    match at {
        None => grab(path, &[]),
        Some(t) => {
            let t = format!("{t:.2}");
            grab(path, &["-skip_frame", "nokey", "-ss", &t])
                .or_else(|_| grab(path, &["-noaccurate_seek", "-ss", &t]))
        }
    }
}

fn grab(path: &Path, seek: &[&str]) -> Result<Vec<Rgb>> {
    let out = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error"])
        .args(seek)
        .arg("-i")
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={SIDE}:{SIDE}"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error::other(format!("could not run ffmpeg: {e}")))?;
    if !out.status.success() || out.stdout.len() < SIDE * SIDE * 3 {
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why.lines().last().unwrap_or("it produced no picture").trim();
        return Err(Error::other(format!(
            "ffmpeg could not read {}: {why}",
            crate::paths::contract(path)
        )));
    }
    Ok(out.stdout.chunks_exact(3).map(|p| Rgb::new(p[0], p[1], p[2])).collect())
}

/// A video's length in seconds, from ffprobe.
fn duration(path: &Path) -> Option<f64> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// The colors worth building a scheme on, best first — the same ranking
/// Android's wallpaper picker and matugen use. Never empty: an image with
/// nothing usable gets Material's fallback blue.
pub fn candidates(pixels: &[Rgb]) -> Vec<Rgb> {
    if pixels.is_empty() {
        return vec![Rgb::from_u32(0x4285f4)];
    }
    let q = QuantizerCelebi::quantize(pixels, 128);
    Score::score(&q.color_to_count, Some(5), None, None)
}

/// The cast of the half of the wallpaper a scheme's surfaces stand for: its
/// darker half for a dark scheme, its lighter half for a light one.
///
/// The pixels are averaged in RGB before the hue is read, so scattered
/// colors cancel out and what is left is the picture's overall tint there —
/// the cool of a night street, the warm of a sunlit wall.
pub fn cast(pixels: &[Rgb], dark: bool) -> Cast {
    if pixels.is_empty() {
        return Cast { hue: 0.0, chroma: 0.0 };
    }
    let mut toned: Vec<(f64, Rgb)> = pixels.iter().map(|p| (Hct::new(*p).get_tone(), *p)).collect();
    toned.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = toned.len().div_ceil(2);
    let side = if dark { &toned[..half] } else { &toned[toned.len() - half..] };
    let n = side.len() as f64;
    let mean = |f: fn(&Rgb) -> u8| {
        (side.iter().map(|(_, p)| f64::from(f(p))).sum::<f64>() / n).round() as u8
    };
    let h = Hct::new(Rgb::new(mean(|p| p.red), mean(|p| p.green), mean(|p| p.blue)));
    Cast { hue: h.get_hue(), chroma: h.get_chroma() }
}

/// Below this HCT chroma a pixel counts as gray and has no hue to give.
const HUE_MIN_CHROMA: f64 = 12.0;

/// How far either side of a peak, in degrees, a hue family reaches.
const HUE_REACH: usize = 25;

/// The least share of the colored pixels a family must have to count.
const HUE_MIN_SHARE: f64 = 0.04;

/// The most families read.
const HUE_MAX: usize = 6;

/// The families of hue a wallpaper has, biggest first: what its accents can
/// be built from, so a hue the picture does not have stays out of them.
///
/// Gray pixels are left out, and each colored one counts by its chroma as
/// well as by being there, so a small bright subject still registers beside
/// a large muted one. Empty for a picture with no color to speak of.
pub fn hues(pixels: &[Rgb]) -> Vec<Hue> {
    let mut weight = [0.0f64; 360];
    let mut chroma = [0.0f64; 360];
    for p in pixels {
        let h = Hct::new(*p);
        let (c, t) = (h.get_chroma(), h.get_tone());
        if c < HUE_MIN_CHROMA || !(8.0..=97.0).contains(&t) {
            continue;
        }
        let bin = (h.get_hue().round() as usize) % 360;
        weight[bin] += c;
        chroma[bin] += c * c;
    }
    let total: f64 = weight.iter().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let at = |i: isize| (i.rem_euclid(360)) as usize;
    let reach = HUE_REACH as isize;
    let mut out = Vec::new();
    while out.len() < HUE_MAX {
        // The window holding the most weight, its middle the peak.
        let window = |w: &[f64; 360], i: isize| (-reach..=reach).map(|d| w[at(i + d)]).sum::<f64>();
        let Some((peak, held)) =
            (0..360isize).map(|i| (i, window(&weight, i))).max_by(|a, b| a.1.total_cmp(&b.1))
        else {
            break;
        };
        if held / total < HUE_MIN_SHARE {
            break;
        }
        // The weighted mean hue, taken around the circle.
        let (mut x, mut y, mut c) = (0.0, 0.0, 0.0);
        for d in -reach..=reach {
            let i = at(peak + d);
            let r = (i as f64).to_radians();
            x += weight[i] * r.cos();
            y += weight[i] * r.sin();
            c += chroma[i];
            weight[i] = 0.0;
            chroma[i] = 0.0;
        }
        out.push(Hue {
            hue: y.atan2(x).to_degrees().rem_euclid(360.0),
            chroma: c / held,
            share: held / total,
        });
    }
    out
}

/// Which candidate to build on.
///
/// `prominent` takes the best-ranked one; `colorful`, the most saturated of
/// them, which suits wallpapers where a small bright subject matters more
/// than a large muted background.
pub fn pick(candidates: &[Rgb], rule: &str) -> Rgb {
    let first = candidates.first().copied().unwrap_or(Rgb::from_u32(0x4285f4));
    match rule {
        "colorful" => candidates
            .iter()
            .copied()
            .max_by(|a, b| super::scheme::chroma(*a).total_cmp(&super::scheme::chroma(*b)))
            .unwrap_or(first),
        _ => first,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_and_media_by_extension() {
        assert!(is_video(Path::new("/a/b.MP4")));
        assert!(!is_video(Path::new("/a/b.gif")));
        assert!(is_media(Path::new("/a/b.webp")));
        assert!(!is_media(Path::new("/a/notes.txt")));
    }

    #[test]
    fn candidates_find_the_dominant_hue() {
        let mut pixels = vec![Rgb::from_u32(0xd03030); 6000];
        pixels.extend(vec![Rgb::from_u32(0x101010); 3000]);
        let c = candidates(&pixels);
        let hue = material_colors::hct::Hct::new(c[0]).get_hue();
        assert!(!(40.0..=350.0).contains(&hue), "a red, got hue {hue}");
    }

    #[test]
    fn a_gray_image_falls_back_to_blue_rather_than_failing() {
        let c = candidates(&vec![Rgb::from_u32(0x808080); 1000]);
        assert_eq!(c.len(), 1);
        assert_eq!(pick(&c, "prominent"), Rgb::from_u32(0x4285f4));
    }

    #[test]
    fn the_cast_comes_from_the_matching_half() {
        // Cool teal shadows under a warm, bright top half.
        let mut pixels = vec![Rgb::new(0x10, 0x18, 0x18); 60];
        pixels.extend(vec![Rgb::new(0xf0, 0xc0, 0x80); 40]);
        let dark = cast(&pixels, true);
        assert!((160.0..230.0).contains(&dark.hue), "dark hue {}", dark.hue);
        let light = cast(&pixels, false);
        assert!((40.0..100.0).contains(&light.hue), "light hue {}", light.hue);
        assert!(cast(&[Rgb::new(0x20, 0x20, 0x20); 10], true).chroma < 1.0);
        assert_eq!(cast(&[], true).chroma, 0.0);
    }

    #[test]
    fn hues_are_the_pictures_own_biggest_first() {
        // Mostly blue, some orange, a lot of gray, and a speck of green too
        // small to count.
        let mut pixels = vec![Rgb::from_u32(0x2a5db0); 500];
        pixels.extend(vec![Rgb::from_u32(0xd9822b); 150]);
        pixels.extend(vec![Rgb::from_u32(0x808080); 2000]);
        pixels.extend(vec![Rgb::from_u32(0x30a040); 5]);
        let h = hues(&pixels);
        assert_eq!(h.len(), 2, "{h:?}");
        let blue = Hct::new(Rgb::from_u32(0x2a5db0)).get_hue();
        assert!((h[0].hue - blue).abs() < 2.0, "{h:?}");
        assert!((40.0..80.0).contains(&h[1].hue), "{h:?}");
        assert!(h[0].share > h[1].share && (h[0].share + h[1].share) > 0.97);
        assert!(h[1].chroma > 30.0);
        assert!(hues(&[Rgb::from_u32(0x808080); 100]).is_empty());
    }

    #[test]
    fn colorful_takes_the_most_saturated() {
        let list = [Rgb::from_u32(0x8a8f99), Rgb::from_u32(0xff2a6d), Rgb::from_u32(0x506070)];
        assert_eq!(pick(&list, "colorful"), Rgb::from_u32(0xff2a6d));
        assert_eq!(pick(&list, "prominent"), Rgb::from_u32(0x8a8f99));
    }

    #[test]
    fn folder_files_spread_through_the_list() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20 {
            std::fs::write(dir.path().join(format!("{i:02}.png")), b"").unwrap();
        }
        std::fs::write(dir.path().join("readme.txt"), b"").unwrap();
        let f = folder_files(dir.path(), 4);
        let names: Vec<String> =
            f.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["00.png", "05.png", "10.png", "15.png"]);
        assert_eq!(folder_files(dir.path(), 0).len(), 20);
    }

    /// ffmpeg's own test pattern, decoded both as a still and as a video.
    #[test]
    fn ffmpeg_samples_stills_and_videos() {
        if !have_ffmpeg() {
            eprintln!("ffmpeg is not installed; skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let still = dir.path().join("red.png");
        let video = dir.path().join("green.mp4");
        let make = |args: &[&str], out: &Path| {
            let ok = Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-y"])
                .args(args)
                .arg(out)
                .status()
                .unwrap()
                .success();
            assert!(ok, "could not make {}", out.display());
        };
        make(&["-f", "lavfi", "-i", "color=c=0xc02020:s=320x200", "-frames:v", "1"], &still);
        make(
            &["-f", "lavfi", "-i", "color=c=0x20a040:s=320x200:d=3", "-pix_fmt", "yuv420p"],
            &video,
        );

        let p = sample(&still).unwrap();
        assert_eq!(p.len(), SIDE * SIDE);
        assert!(p[0].red > 150 && p[0].green < 60, "{:?}", p[0]);

        let v = sample(&video).unwrap();
        assert_eq!(v.len(), SIDE * SIDE * VIDEO_FRAMES);
        assert!(v[0].green > 120 && v[0].red < 80, "{:?}", v[0]);

        let both = sample_folder(dir.path()).unwrap();
        assert_eq!(both.len(), SIDE * SIDE * 2);

        let err = sample(&dir.path().join("missing.png")).unwrap_err();
        assert!(err.to_string().contains("not a file"));
    }
}
