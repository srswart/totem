//! The dedupe job: near-duplicate Knowledge memories become one record that
//! cites the ones it replaces.
//!
//! Two records are candidates for merging only if they agree on *both* signals
//! the Solution Intent names — vector similarity and graph context (§5). The
//! graph half matters more than it looks: two records that read almost
//! identically but concern different components are two facts about two things,
//! and a job that merged them on cosine distance alone would delete one of them
//! in all but name.
//!
//! Scope is not a signal here, it is a partition. Records at different scopes
//! are never in the same group however identical they read, because collapsing
//! a private note into a project record would publish it — sharing is
//! promotion's job, and promotion is a decision with an author
//! (`totem_core::promotion`).

use surrealdb::Connection;
use totem_core::{
    AccessOperation, Content, CurationEvent, Economics, MemoryCategory, MemoryRecord, Provenance,
    ScopeChain,
};
use totem_store::StoreResult;

use crate::{Curator, MERGE_ENDPOINT, SCAN_ENDPOINT};

/// How alike two records must be before the job treats them as one.
///
/// The default is deliberately strict. A false negative leaves a duplicate for
/// the next run — or for a human — to deal with; a false positive retires
/// somebody's memory, and while the rollback exists, nobody reviews what they
/// never noticed. There is no measured value behind the number yet: the
/// evaluation corpus that could produce one is ADV-STORE-005/ADV-CORE-005, so
/// this is a defensible starting point and a knob, not a finding.
const DEFAULT_SIMILARITY_THRESHOLD: f32 = 0.95;

/// How the dedupe job decides what counts as a duplicate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DedupePolicy {
    threshold: f32,
}

impl Default for DedupePolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl DedupePolicy {
    /// The standing policy.
    pub fn new() -> Self {
        Self {
            threshold: DEFAULT_SIMILARITY_THRESHOLD,
        }
    }

    /// Require a different cosine similarity before merging.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// The similarity two records must reach to be merged.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }
}

/// What one dedupe run did.
#[derive(Debug, Clone, PartialEq)]
pub struct DedupeReport {
    /// How many active records the scan considered.
    pub examined: usize,
    /// How many of those carried no embedding and were therefore left alone.
    ///
    /// Reported rather than silently skipped: an unembedded corpus makes this
    /// job a no-op, and a report of "0 merges" would otherwise look like a
    /// clean bill of health.
    pub skipped_without_embedding: usize,
    /// The merges this run recorded, in the order it made them.
    pub merges: Vec<CurationEvent>,
}

impl<'a, C: Connection> Curator<'a, C> {
    /// Merge near-duplicate Knowledge memories within each scope and subject.
    ///
    /// Idempotent: a second run sees the survivor of the first (the originals
    /// are retired and no longer candidates), finds nothing else like it, and
    /// does nothing.
    ///
    /// A merge that the store refuses — because a live write changed one of the
    /// records between the scan and the write — ends the run rather than being
    /// swallowed. Merges already recorded stand, and because the job is
    /// idempotent the next run simply re-scans.
    pub async fn dedupe(
        &self,
        chain: &ScopeChain,
        policy: &DedupePolicy,
    ) -> StoreResult<DedupeReport> {
        let candidates = self
            .store
            .curation()
            .candidates(chain, MemoryCategory::Knowledge)
            .await?;
        self.log(AccessOperation::Recall, SCAN_ENDPOINT, |entry| {
            entry.with_result_count(candidates.len() as u64)
        })
        .await?;

        let embedded: Vec<&MemoryRecord> = candidates
            .iter()
            .filter(|record| record.content.embedding.is_some())
            .collect();
        let mut report = DedupeReport {
            examined: candidates.len(),
            skipped_without_embedding: candidates.len() - embedded.len(),
            merges: Vec::new(),
        };

        for group in group_by_context(&embedded) {
            for cluster in cluster(&group, policy.threshold()) {
                let originals: Vec<MemoryRecord> =
                    cluster.iter().map(|record| (*record).clone()).collect();
                // One provenance for both: the record the curator wrote and the
                // event recording that it wrote it are the same act, by the
                // same author, citing the same sources.
                let provenance =
                    self.provenance(originals.iter().map(|record| record.id).collect());
                let survivor = survivor_of(&cluster, provenance.clone());
                let event = self
                    .store
                    .curation()
                    .merge(chain, &survivor, &originals, provenance)
                    .await?;
                self.log(AccessOperation::Save, MERGE_ENDPOINT, |entry| {
                    entry.for_memory(survivor.id)
                })
                .await?;
                report.merges.push(event);
            }
        }
        Ok(report)
    }
}

/// Partition candidates into the sets a merge could ever be drawn from: same
/// scope, same subject.
///
/// Order is preserved within each group, so the whole job is deterministic —
/// which is what makes "run it again and nothing happens" a property rather
/// than a hope.
fn group_by_context<'r>(records: &[&'r MemoryRecord]) -> Vec<Vec<&'r MemoryRecord>> {
    let mut groups: Vec<Vec<&MemoryRecord>> = Vec::new();
    for record in records {
        match groups
            .iter_mut()
            .find(|group| group[0].scope == record.scope && group[0].subject == record.subject)
        {
            Some(group) => group.push(record),
            None => groups.push(vec![record]),
        }
    }
    groups
}

/// Greedily collect the records in one group that are near-duplicates of each
/// other, returning only the clusters worth merging.
///
/// The oldest unclustered record anchors each cluster, so the survivor of a
/// cluster is always compared against the record a reader would have written
/// first — and a run that sees the same corpus twice builds the same clusters.
fn cluster<'r>(group: &[&'r MemoryRecord], threshold: f32) -> Vec<Vec<&'r MemoryRecord>> {
    let mut taken = vec![false; group.len()];
    let mut clusters = Vec::new();

    for anchor in 0..group.len() {
        if taken[anchor] {
            continue;
        }
        let mut members = vec![group[anchor]];
        let mut members_at = vec![anchor];
        for candidate in (anchor + 1)..group.len() {
            if taken[candidate] {
                continue;
            }
            if similarity(group[anchor], group[candidate]) >= threshold {
                members.push(group[candidate]);
                members_at.push(candidate);
            }
        }
        if members.len() < 2 {
            continue;
        }
        for position in members_at {
            taken[position] = true;
        }
        clusters.push(members);
    }
    clusters
}

/// Cosine similarity between two records' embeddings, or `-1.0` (maximally
/// dissimilar) if either is missing or degenerate.
///
/// Callers have already filtered out unembedded records; returning the
/// least-similar value rather than panicking means a record that somehow
/// arrives without a usable vector is left alone, which is the safe direction.
fn similarity(left: &MemoryRecord, right: &MemoryRecord) -> f32 {
    let (Some(left), Some(right)) = (&left.content.embedding, &right.content.embedding) else {
        return -1.0;
    };
    if left.len() != right.len() {
        return -1.0;
    }
    let dot: f32 = left.iter().zip(right.iter()).map(|(a, b)| a * b).sum();
    let norm = |vector: &[f32]| vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    let magnitude = norm(left) * norm(right);
    if magnitude == 0.0 {
        return -1.0;
    }
    dot / magnitude
}

/// Build the record that will replace a cluster, authored by the curator and
/// citing every member in `provenance.derived_from` — the lineage a reader
/// follows from the survivor back to what it replaced.
///
/// The newest member's wording wins: a merge that preferred the oldest text
/// would quietly undo the most recent correction. Everything else is carried
/// forward rather than reset — the union of tags, and the accumulated
/// economics of the whole cluster, because a dedupe that started the survivor
/// at zero would make every consolidation a silent write-off of what the value
/// loop had learned (docs/project-brief.md G4).
fn survivor_of(cluster: &[&MemoryRecord], provenance: Provenance) -> MemoryRecord {
    let canonical = cluster.last().expect("a cluster is never empty");

    let mut tags: Vec<String> = cluster
        .iter()
        .flat_map(|record| record.content.tags.iter().cloned())
        .collect();
    tags.sort();
    tags.dedup();

    let mut content = Content::new(canonical.content.body.clone()).with_tags(tags);
    content.embedding = canonical.content.embedding.clone();

    let economics = Economics {
        use_count: cluster
            .iter()
            .map(|record| record.economics.use_count)
            .sum(),
        last_used_at: cluster
            .iter()
            .filter_map(|record| record.economics.last_used_at)
            .max(),
        value_score: cluster
            .iter()
            .map(|record| record.economics.value_score)
            .fold(f32::MIN, f32::max),
        currency: cluster
            .iter()
            .map(|record| record.economics.currency)
            .fold(f32::MIN, f32::max),
    };

    let mut survivor = MemoryRecord::new(
        canonical.category,
        canonical.scope.clone(),
        content,
        provenance,
    );
    survivor.subject = canonical.subject.clone();
    survivor.economics = economics;
    survivor
}
