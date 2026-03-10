//! FangHub Registry — the public marketplace backend for OpenFang Hand packages.
//!
//! Provides a REST API for:
//! - Publishing Hand packages (with signed manifests)
//! - Searching and browsing available packages
//! - Retrieving package metadata and download URLs
//! - User account management via GitHub OAuth tokens

pub mod auth;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;
pub mod server;
pub mod store;
pub mod ui;

pub use error::{RegistryError, RegistryResult};
pub use models::{
    HandPackage, PackageVersion, PublishRequest, SearchQuery, SearchResponse, SearchResult,
    SortOrder, UserAccount,
};
pub use server::RegistryServer;
