// SPEC: book-illustrations (ILLUS-03, ILLUS-04, ILLUS-07, ILLUS-12)

//! The marker that puts a picture back into a stream of paragraphs.
//!
//! The whole feature rests on one idea: an illustration is **a paragraph**.
//! `[[image: 0007.png]]`, alone on its line, travels through pagination
//! (READ-10), through the per-paragraph translation loop (READ-22) and onto
//! disk (READ-31) using machinery that already exists and does not know an
//! image is involved. A page→image table would have to be re-anchored on every
//! repagination; a paragraph does not.
//!
//! The name is also the security boundary: it is the only string of this
//! feature that comes back from the frontend, so `is_image_name` is the single
//! definition of what may be turned into a path, and both the marker parser and
//! the Tauri command go through it.

/// A picture pulled out of a book, ready to be written to `<book>/images/`.
pub struct Illustration {
    /// `NNNN.<ext>`, base 1, in order of appearance — see `image_name`.
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A PDF carries rules, bullets and footer logos as image objects. Without a
/// floor the book comes out speckled with 3x3 px debris.
///
/// 64 is a **chosen** number, not a measured one: T7 (UAT with real books) is
/// what confronts it with actual illustrations, and correcting it is editing
/// this line.
pub const MIN_IMAGE_SIDE_PX: u32 = 64;

/// What one marker costs against `PAGE_BUDGET_CHARS`, so a page does not stack
/// five illustrations believing it spent 110 characters.
///
/// ponytail: a flat number, because the real cost is the rendered height and
/// that depends on the window width — which the backend, where pagination
/// lives, does not know. Measure the screen before making this cleverer.
pub const IMAGE_BUDGET_CHARS: usize = 700;

/// What `starts_with` looks for when pagination charges a marker. Public
/// because it is the cheap half of `marker_name`: pagination scans per
/// character and cannot afford the full parse at every offset.
pub const MARKER_PREFIX: &str = "[[image: ";
const MARKER_SUFFIX: &str = "]]";

/// `0001.png`, base 1. Zero-padded for the same reason page files are: the
/// user opens this folder in the explorer (READ-31), and alphabetical order
/// has to be reading order.
pub fn image_name(index: usize, extension: &str) -> String {
    format!("{index:04}.{extension}")
}

pub fn marker_for(name: &str) -> String {
    format!("{MARKER_PREFIX}{name}{MARKER_SUFFIX}")
}

/// The image name inside a paragraph that is *nothing but* a marker.
///
/// Requiring the whole paragraph to match is what keeps prose that happens to
/// mention `[[image: ...]]` from being swallowed, and the name is validated
/// here so a marker can never name a path.
pub fn marker_name(paragraph: &str) -> Option<&str> {
    let name = paragraph
        .trim()
        .strip_prefix(MARKER_PREFIX)?
        .strip_suffix(MARKER_SUFFIX)?;
    is_image_name(name).then_some(name)
}

/// Exactly `NNNN.<ext>`: four digits, a dot, a short alphanumeric extension.
///
/// This is a guard on a trust boundary. `get_book_image` receives this name
/// from the frontend and joins it onto the book folder, so anything with a
/// separator, a `..`, or a shape this crate never produces is refused before
/// it can become a path.
pub fn is_image_name(name: &str) -> bool {
    let Some((stem, extension)) = name.split_once('.') else {
        return false;
    };
    stem.len() == 4
        && stem.bytes().all(|b| b.is_ascii_digit())
        && (1..=5).contains(&extension.len())
        && extension.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// The extension to store an image under, taken from the source's own name.
/// `None` is "this is not an image file we can name", and the caller drops it —
/// dropping is always allowed here (ILLUS-08).
pub fn extension_of(source: &str) -> Option<String> {
    let extension = source
        .rsplit('/')
        .next()?
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())?;
    is_image_name(&format!("0001.{extension}")).then_some(extension)
}

/// Whether an image is big enough to be a picture rather than a rule or a
/// bullet (ILLUS-02).
pub fn is_large_enough(width: u32, height: u32) -> bool {
    width >= MIN_IMAGE_SIDE_PX && height >= MIN_IMAGE_SIDE_PX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_survives_the_round_trip_and_prose_is_not_mistaken_for_one() {
        let marker = marker_for(&image_name(7, "png"));
        assert_eq!(marker, "[[image: 0007.png]]");
        assert_eq!(marker_name(&marker), Some("0007.png"));
        // `split_paragraphs` trims, but a page read back from disk can still
        // arrive with a stray newline.
        assert_eq!(marker_name("\n[[image: 0007.png]]\n"), Some("0007.png"));

        for prose in [
            "O quadro [[image: 0007.png]] foi pintado em 1890.",
            "[[image: 0007.png]] e o texto continua",
            "[[image: ]]",
            "[[image: 7.png]]",
            "[[image: 0007]]",
            "[[image: 0007.png",
            "Um parágrafo comum.",
        ] {
            assert_eq!(marker_name(prose), None, "confundiu com marcador: {prose:?}");
        }
    }

    #[test]
    fn a_name_that_is_a_path_is_refused_so_it_can_never_be_joined_onto_a_folder() {
        // ILLUS-07. This is the only string of this feature that comes back
        // from the frontend.
        for bad in [
            "../../etc/passwd",
            "..",
            "0001.png/../../x",
            "0001/png",
            "0001.PNG",
            "0001.",
            "0001.averylongext",
            "",
            "0001.pn g",
        ] {
            assert!(!is_image_name(bad), "aceitou {bad:?}");
            assert_eq!(marker_name(&marker_for(bad)), None, "aceitou {bad:?}");
        }
        assert!(is_image_name("0001.png"));
        assert!(is_image_name("9999.jpeg"));
    }

    #[test]
    fn the_extension_comes_from_the_source_name_and_odd_ones_are_dropped() {
        assert_eq!(extension_of("Images/fig01.PNG").as_deref(), Some("png"));
        assert_eq!(extension_of("../img/a.jpeg").as_deref(), Some("jpeg"));
        assert_eq!(extension_of("cover"), None);
        assert_eq!(extension_of("a.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension_of("weird.averylongext"), None);
    }

    #[test]
    fn the_dimension_floor_keeps_rules_and_bullets_out() {
        assert!(is_large_enough(64, 64));
        assert!(is_large_enough(800, 600));
        assert!(!is_large_enough(63, 600));
        assert!(!is_large_enough(600, 3));
    }
}
