// SPEC: self-contained-runtime (SELF-12)

use crate::runtime::bundled;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// fastembed is built against a dynamically loaded ONNX Runtime, so the
/// library has to exist before the first embedding call and `ORT_DYLIB_PATH`
/// has to point at it. Static linking was ruled out because the prebuilt
/// static lib requires the MSVC 2022 STL.
///
/// The library used to be downloaded on first use, which meant importing a
/// document offline could not finish. It now ships inside the installer; the
/// pinned version lives in `scripts/vendor.json`, not in this file.
///
/// Still `async` because the whole document pipeline calls it that way, and
/// changing every caller to save one `await` would be noise.
pub async fn ensure_dylib(app: &AppHandle) -> Result<PathBuf, String> {
    ensure_dylib_blocking(app)
}

/// The same work without the `async`, for callers that are not part of the
/// document pipeline - the voice synthesis loads its own ONNX graph and has no
/// async context to borrow.
///
/// This is not an optimisation: `speak_sentence` never called anything here, so
/// which `onnxruntime.dll` got loaded depended on the OS search path, and this
/// tree ships two of them (1.28.0 under `resources/onnxruntime`, and piper's
/// own from 2023). Pointing at the resolved one is what makes it a choice.
pub fn ensure_dylib_blocking(app: &AppHandle) -> Result<PathBuf, String> {
    let dylib = bundled::onnxruntime_dylib(app)?;
    set_dylib_path(&dylib);
    Ok(dylib)
}

fn set_dylib_path(path: &Path) {
    // `ort` reads this the first time a session is created, so setting it
    // before any embedding call is enough.
    std::env::set_var("ORT_DYLIB_PATH", path);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure mode this replaced was a download that could not happen.
    /// What matters now is that the variable `ort` reads really is set from
    /// the resolved path, since nothing else points it at the library.
    #[test]
    fn the_resolved_library_is_what_ort_will_load() {
        let path = std::env::temp_dir().join("readme-ort-probe.dll");
        set_dylib_path(&path);
        assert_eq!(
            std::env::var("ORT_DYLIB_PATH").unwrap(),
            path.to_string_lossy()
        );
    }
}
