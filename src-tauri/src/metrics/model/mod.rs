//! The PULSE metrics data model.
//!
//! Every type here is part of the contract shared with the frontend and is
//! mirrored in `src/types/metrics.ts`. Serialisation tests in each module pin
//! the wire format so the two cannot drift apart silently.
//!
//! The model separates four things that are easy to conflate:
//!
//! | Concept | Type | Answers |
//! |---|---|---|
//! | What is measured | [`MetricKey`] | "GPU core temperature" |
//! | What it is measured on | [`SourceId`] | "the discrete GPU at PCI 01:00.0" |
//! | What PULSE knows about it | [`MetricDefinition`] | unit, category, kind, availability |
//! | What it read just now | [`MetricSample`] | value plus timestamp, or why not |

pub mod availability;
pub mod category;
pub mod definition;
pub mod error;
pub mod ident;
pub mod key;
pub mod kind;
pub mod provider_id;
pub mod reference;
pub mod sample;
pub mod source;
pub mod unit;
pub mod value;

pub use availability::Availability;
pub use category::MetricCategory;
pub use definition::{MetricDefinition, MetricDefinitionBuilder};
pub use error::{MetricError, MetricErrorCode};
pub use ident::IdentError;
pub use key::MetricKey;
pub use kind::MetricKind;
pub use provider_id::ProviderId;
pub use reference::MetricRef;
pub use sample::{now_ms, MetricSample, TimestampMs};
pub use source::{SourceId, CANONICAL_SOURCE_KINDS};
pub use unit::MetricUnit;
pub use value::{MetricValue, MetricValueType};
