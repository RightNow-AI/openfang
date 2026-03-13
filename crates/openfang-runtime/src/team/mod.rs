//! Multi-agent team coordination primitives.
//!
//! Provides a 3-layer model:
//!   Layer 1 — [`Coordinator`]: owns the task registry, contract ledger, and blocker resolution.
//!   Layer 2 — [`WorkerAgent`]: domain-scoped specialists (UI, SDK, backend-contract, QA).
//!   Layer 3 — [`SubagentHandle`]: bounded single-task units spawned by workers, reporting only to their parent.
//!
//! Coordination happens through the [`Blackboard`] — a shared surface holding
//! the task registry, message mailbox, and key-value contract store.

pub mod blackboard;
pub mod coordinator;
pub mod messages;
pub mod registry;
pub mod task_packet;
pub mod worker;

pub use blackboard::Blackboard;
pub use coordinator::Coordinator;
pub use messages::{TeamMessage, TeamMessageKind};
pub use registry::{TaskRecord, TaskRegistry, TaskState};
pub use task_packet::{OutputContract, TaskPacket, TaskPriority};
pub use worker::{SubagentHandle, WorkerAgent};
