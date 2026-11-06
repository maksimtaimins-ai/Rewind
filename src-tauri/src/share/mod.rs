//! Sending clips to other places: Discord webhooks and other Rewinds on the LAN.

pub mod discord;
#[cfg(windows)]
pub mod http;
pub mod lan;
pub mod presence;
#[cfg(windows)]
pub mod zeroconf;
