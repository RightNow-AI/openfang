//! Core kernel for the OpenFang Agent Operating System.
//!
//! The kernel manages agent lifecycles, memory, permissions, scheduling,
//! and inter-agent communication.

pub mod approval;
pub mod auth;
pub mod auto_reply;
pub mod background;
pub mod capabilities;
pub mod config;
pub mod config_reload;
pub mod cron;
pub mod cron_delivery;
pub mod error;
pub mod event_bus;
pub mod heartbeat;
pub mod kernel;
pub mod metering;
pub mod pairing;
pub mod registry;
pub mod scheduler;
pub mod supervisor;
pub mod triggers;
pub mod whatsapp_gateway;
pub mod wizard;
pub mod workflow;

pub use kernel::DeliveryTracker;
pub use kernel::OpenFangKernel;
