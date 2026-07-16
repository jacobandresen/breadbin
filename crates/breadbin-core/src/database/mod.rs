// database - shared domain models for CSDb data (releases, groups, sceners,
// events, search types). Persistence lives in crate::diesel_db.

pub mod models;

pub use models::*;
