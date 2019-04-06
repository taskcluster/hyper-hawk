//! Library for authenticating HTTP requests and responses with Hawk.
//!
//! Most functionality comes directly from the `hawk` crate; this merely adds support for the
//! [HawkScheme] [Authorization](hyper::header::Authorization) scheme and a new (nonstandard)
//! [ServerAuthorization] header.

mod serverauth;
pub use crate::serverauth::ServerAuthorization;

mod authscheme;
pub use crate::authscheme::HawkScheme;
