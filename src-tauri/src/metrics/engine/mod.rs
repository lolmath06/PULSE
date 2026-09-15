//! The metrics engine: provider registry, catalog and sampling entry point.
//!
//! The engine is deliberately **platform-agnostic**. It knows nothing about
//! `/proc`, WMI, NVML or any other data source — only about providers. That is
//! what makes the following true:
//!
//! > All future PULSE metrics can be added behind a single contract without the
//! > interface needing to know whether they come from Fedora, Windows, NVIDIA,
//! > `/proc`, WMI or SMART.

pub mod status;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricErrorCode, MetricRef, MetricSample,
    ProviderId,
};
use crate::metrics::providers::MetricProvider;

pub use status::{EngineState, EngineStatus, ProviderSummary};

/// Why a provider could not be registered.
#[derive(Debug, Clone, PartialEq)]
pub enum RegistrationError {
    /// A provider with this identifier is already registered.
    DuplicateProvider { provider: ProviderId },
    /// Two providers claim the same metric reference.
    MetricCollision {
        metric: MetricRef,
        existing_provider: ProviderId,
        new_provider: ProviderId,
    },
    /// One provider declared the same metric reference twice.
    DuplicateWithinProvider {
        metric: MetricRef,
        provider: ProviderId,
    },
    /// The provider failed to enumerate its metrics.
    Describe {
        provider: ProviderId,
        error: MetricError,
    },
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistrationError::DuplicateProvider { provider } => {
                write!(f, "provider '{provider}' is already registered")
            }
            RegistrationError::MetricCollision {
                metric,
                existing_provider,
                new_provider,
            } => write!(
                f,
                "metric '{metric}' is claimed by both '{existing_provider}' and '{new_provider}'"
            ),
            RegistrationError::DuplicateWithinProvider { metric, provider } => write!(
                f,
                "provider '{provider}' declared metric '{metric}' more than once"
            ),
            RegistrationError::Describe { provider, error } => {
                write!(
                    f,
                    "provider '{provider}' failed to describe metrics: {error}"
                )
            }
        }
    }
}

impl std::error::Error for RegistrationError {}

impl From<RegistrationError> for MetricError {
    fn from(error: RegistrationError) -> Self {
        let code = match error {
            RegistrationError::MetricCollision { .. }
            | RegistrationError::DuplicateWithinProvider { .. }
            | RegistrationError::DuplicateProvider { .. } => MetricErrorCode::DuplicateMetric,
            RegistrationError::Describe { .. } => MetricErrorCode::ProviderUnavailable,
        };

        MetricError::new(code, error.to_string()).with_recoverable(false)
    }
}

/// Registry of providers and the metrics they own.
///
/// Built once at startup, then shared immutably (`Arc<MetricsEngine>`) across
/// Tauri commands and, later, across windows and background tasks. Sampling
/// takes `&self`, so no locking is involved on the read path.
pub struct MetricsEngine {
    providers: Vec<Arc<dyn MetricProvider>>,
    /// Catalog sorted by `(key, sourceId)` so the frontend sees a stable order
    /// across runs.
    catalog: Vec<MetricDefinition>,
    /// Metric reference to provider index. Resolution is O(1); the engine never
    /// scans providers looking for an owner.
    owners: HashMap<MetricRef, usize>,
}

impl MetricsEngine {
    /// Creates an engine with no providers.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            catalog: Vec::new(),
            owners: HashMap::new(),
        }
    }

    /// Registers a provider and merges its metrics into the catalog.
    ///
    /// Registration is **atomic**: if any metric collides with an existing one,
    /// or the provider declares a duplicate itself, nothing is added. A partly
    /// registered provider would be far harder to reason about than a rejected
    /// one.
    pub fn register(&mut self, provider: Arc<dyn MetricProvider>) -> Result<(), RegistrationError> {
        let provider_id = provider.id().clone();

        if self.providers.iter().any(|p| p.id() == &provider_id) {
            return Err(RegistrationError::DuplicateProvider {
                provider: provider_id,
            });
        }

        let definitions = provider
            .describe()
            .map_err(|error| RegistrationError::Describe {
                provider: provider_id.clone(),
                error,
            })?;

        // Validate everything before mutating any state.
        let mut seen = HashMap::with_capacity(definitions.len());
        for definition in &definitions {
            if let Some(existing) = self.owners.get(&definition.metric) {
                return Err(RegistrationError::MetricCollision {
                    metric: definition.metric.clone(),
                    existing_provider: self.providers[*existing].id().clone(),
                    new_provider: provider_id,
                });
            }

            if seen.insert(definition.metric.clone(), ()).is_some() {
                return Err(RegistrationError::DuplicateWithinProvider {
                    metric: definition.metric.clone(),
                    provider: provider_id,
                });
            }
        }

        // Commit.
        let index = self.providers.len();
        self.providers.push(provider);

        self.owners.reserve(definitions.len());
        for definition in &definitions {
            self.owners.insert(definition.metric.clone(), index);
        }

        self.catalog.extend(definitions);
        self.catalog
            .sort_by(|left, right| left.metric.cmp(&right.metric));

        Ok(())
    }

    /// The aggregated catalog, ordered deterministically by `(key, sourceId)`.
    pub fn catalog(&self) -> &[MetricDefinition] {
        &self.catalog
    }

    /// Looks up one metric's definition.
    ///
    /// The catalog is kept sorted, so this is a binary search rather than the
    /// linear scan that would cost `O(catalog)` per widget once PULSE exposes
    /// hundreds of metrics.
    pub fn definition(&self, metric: &MetricRef) -> Option<&MetricDefinition> {
        let position = self
            .catalog
            .binary_search_by(|candidate| candidate.metric.cmp(metric))
            .ok()?;

        Some(&self.catalog[position])
    }

    /// Whether a reference is known to this engine.
    pub fn knows(&self, metric: &MetricRef) -> bool {
        self.owners.contains_key(metric)
    }

    /// A snapshot of the engine's condition.
    pub fn status(&self) -> EngineStatus {
        let summaries = self
            .providers
            .iter()
            .enumerate()
            .map(|(index, provider)| {
                let owned = self
                    .catalog
                    .iter()
                    .filter(|definition| self.owners.get(&definition.metric) == Some(&index));

                let (metric_count, available_metric_count) =
                    owned.fold((0, 0), |(total, available), definition| {
                        (
                            total + 1,
                            available + usize::from(definition.availability.is_available()),
                        )
                    });

                ProviderSummary {
                    id: provider.id().clone(),
                    metric_count,
                    available_metric_count,
                }
            })
            .collect();

        EngineStatus::new(summaries)
    }

    /// Samples the requested metrics.
    ///
    /// Guarantees, all of which are tested:
    ///
    /// - **One sample per requested reference, in request order.** Callers can
    ///   zip the response against their request.
    /// - **Each provider is called at most once**, with its own references
    ///   deduplicated — asking for five GPU metrics does not query the driver
    ///   five times.
    /// - **Failures are isolated.** A provider returning `Err` only affects its
    ///   own metrics; every other provider's samples are still returned.
    /// - **Unknown references are answered, not rejected.** They come back as
    ///   [`Availability::NotRegistered`], so one stale dashboard entry cannot
    ///   fail the whole request.
    /// - **Missing samples are accounted for.** A provider that omits a metric
    ///   it declared yields a `MissingSample` error rather than a silent gap.
    /// - **An empty request returns an empty response**, without touching any
    ///   provider.
    pub fn sample(&self, requested: &[MetricRef]) -> Vec<MetricSample> {
        if requested.is_empty() {
            return Vec::new();
        }

        // Group the unique known references by owning provider, preserving
        // first-seen order for reproducible provider call order.
        let mut per_provider: Vec<(usize, Vec<MetricRef>)> = Vec::new();
        let mut provider_slot: HashMap<usize, usize> = HashMap::new();
        let mut queued: HashSet<&MetricRef> = HashSet::new();

        for metric in requested {
            let Some(&provider_index) = self.owners.get(metric) else {
                continue;
            };
            if !queued.insert(metric) {
                continue;
            }

            let slot = *provider_slot.entry(provider_index).or_insert_with(|| {
                per_provider.push((provider_index, Vec::new()));
                per_provider.len() - 1
            });
            per_provider[slot].1.push(metric.clone());
        }

        // Collect results per provider, isolating failures.
        let mut results: HashMap<MetricRef, MetricSample> = HashMap::with_capacity(requested.len());

        for (provider_index, refs) in &per_provider {
            let provider = &self.providers[*provider_index];

            match provider.sample(refs) {
                Ok(samples) => {
                    for sample in samples {
                        // Ignore anything the provider was not asked for; a
                        // provider must not smuggle extra metrics into a
                        // response.
                        if queued.contains(&sample.metric) {
                            results.insert(sample.metric.clone(), sample);
                        }
                    }

                    for metric in refs {
                        if !results.contains_key(metric) {
                            results.insert(
                                metric.clone(),
                                MetricSample::unavailable(
                                    metric.clone(),
                                    Availability::provider_error(MetricError::new(
                                        MetricErrorCode::MissingSample,
                                        format!(
                                            "provider '{}' returned no sample for '{metric}'",
                                            provider.id()
                                        ),
                                    )),
                                ),
                            );
                        }
                    }
                }
                Err(error) => {
                    // This provider is down. Mark only its metrics; the loop
                    // continues with the others.
                    for metric in refs {
                        results.insert(
                            metric.clone(),
                            MetricSample::unavailable(
                                metric.clone(),
                                Availability::provider_error(error.clone()),
                            ),
                        );
                    }
                }
            }
        }

        // Rebuild the response in request order.
        requested
            .iter()
            .map(|metric| {
                results.get(metric).cloned().unwrap_or_else(|| {
                    MetricSample::unavailable(
                        metric.clone(),
                        Availability::not_registered(format!(
                            "'{metric}' is not registered in this catalog"
                        )),
                    )
                })
            })
            .collect()
    }
}

impl Default for MetricsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MetricsEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetricsEngine")
            .field("providers", &self.providers.len())
            .field("metrics", &self.catalog.len())
            .finish()
    }
}

#[cfg(test)]
mod tests;
