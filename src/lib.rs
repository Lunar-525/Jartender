//! Small, synchronous building blocks for Minecraft server launchers.
//!
//! Paths, Java selection, EULA consent, UI, and process ownership belong to the caller.
//! Discovery and launch planning only read files. Installation reports progress through
//! a callback; no library function prints, exits, or replaces the host process.
//!
//! ```no_run
//! use jartender::{LaunchOptions, discover, prepare_launch};
//!
//! # fn main() -> jartender::Result<()> {
//! // The host chooses its paths and Java executable.
//! let root = std::env::current_dir()?.join("Servers");
//! let servers = discover(&root)?;
//! let options = LaunchOptions::new("survival", "java");
//! let plan = prepare_launch(&root, &options)?;
//! // After the host has handled EULA consent, it owns spawning and console policy.
//! jartender::check_eula(&plan.directory)?;
//! let child = plan.to_command().spawn()?;
//! # let _ = (servers, child);
//! # Ok(())
//! # }
//! ```

mod config;
mod error;
pub mod install;
mod run;
mod server;

pub use error::{Error, Result};
pub use run::{LaunchOptions, LaunchPlan, accept_eula, check_eula, prepare_launch};
pub use server::{Server, list as discover};
