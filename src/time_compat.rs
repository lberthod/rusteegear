//! `Instant`/`SystemTime` portables (Sprint 114) : réexporte `std::time` partout
//! sauf sur wasm32, où `Instant::now()`/`SystemTime::now()` paniquent purement et
//! simplement (« time not implemented on this platform » — ce target n'expose
//! aucune horloge système). `web_time` a une API identique, adossée à
//! `performance.now()`/`Date.now()` côté navigateur — un remplacement direct des
//! quelques sites d'appel qui tournent réellement sur le chemin d'exécution web
//! (les autres, réseau/tests, restent derrière `#[cfg(not(target_arch =
//! "wasm32"))]` et n'ont pas besoin de ce module).

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
pub use web_time::{Instant, SystemTime, UNIX_EPOCH};
