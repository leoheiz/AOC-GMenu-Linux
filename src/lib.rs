//! Biblioteca compartilhada: features, framing DDC/CI, backend e config.

pub mod backend;
pub mod bus_lock;
pub mod config;
pub mod ddc;
pub mod edid;
pub mod features;
pub mod logutil;
pub mod profile;
pub mod worker;

pub use backend::{
    detect_aoc, Backend, BackendError, DetectedMonitor, I2cBackend, MockBackend, SupportFlag,
};
pub use config::{load_config, save_config, AppConfig, CrosshairConfig, CrosshairShape};
pub use features::{find_feature, format_value, parse_value, Access, Feature, FEATURES};
pub use logutil::{short_cli_reason, FRIENDLY_READ_FAIL};
pub use profile::{apply_profile, list_profiles, profile_writes, ProfileApplyError};
pub use worker::{DebounceState, WorkerCmd, WorkerEvent, WorkerHandle};
