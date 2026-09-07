// SPEC: book-reader (READ-09, READ-10, READ-12, READ-18, READ-20, READ-21, READ-22, READ-23, READ-28, READ-29, READ-31)

//! Reader-side processing of an imported book: turning a file into text.
//!
//! It deliberately does not live under `rag/`: AD-052 marked that module as
//! revoked, and hanging a new feature off code that is on its way out would
//! tie the two together.
//!
pub mod epub;
pub mod html;
pub mod illustrations;
pub mod pagination;
pub mod storage;
pub mod translate;
