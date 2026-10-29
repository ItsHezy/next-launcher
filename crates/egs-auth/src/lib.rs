//! OAuth login, token refresh, exchange codes and secure token storage.
//!
//! Spec: `docs/protocol/auth.md`. No Tauri dependency: the app owns the login
//! window and passes the callback URL in.

pub mod login;
pub mod pkce;
pub mod session;
pub mod store;

pub use login::{login_url, parse_callback, Callback, CALLBACK_PREFIX, LOGIN_INIT_SCRIPT};
pub use pkce::Pkce;
pub use session::{AccountSummary, AuthError, Session};
pub use store::{KeyringStore, MemoryStore, StoredSession, TokenStore};
