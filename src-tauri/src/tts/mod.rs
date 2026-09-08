// SPEC: read-aloud (TTS-01, TTS-02, TTS-03)

//! Reading a page out loud: cutting it into sentences, saying them, and telling
//! the screen when each one plays.
//!
//! It does not live under `reader/` because it is not part of turning a file
//! into pages - it consumes what the reader already produced. `timing` is the
//! pure half and carries the tests; `speaker` and `voices` are process and
//! disk, and are exercised by UAT.

pub mod espeak;
pub mod kokoro;
pub mod speaker;
pub mod voices;
pub mod timing;
