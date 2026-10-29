//! Rank fusion of retrieval lists (PRD §6.6, PLAN Phase 4 step 1).
//!
//! Reciprocal Rank Fusion: an item's fused score is the sum, over every list that
//! contains it, of `1 / (k + rank)` with ranks starting at 1. Only ranks enter the
//! score, so raw similarity scores of unrelated models are never compared. Each list
//! contributes its top `depth` items. Fused results are ordered by descending fused
//! score, with equal scores broken by the ascending item key (the `record_id`).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use serde::{Deserialize, Serialize};

/// The RRF constant of the PRD (§5.3, §6.6).
pub const DEFAULT_RRF_K: u32 = 60;

/// Fusion method; RRF is the only one (PRD §6.6: rank-based fusion first).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FusionMethod {
    /// Reciprocal Rank Fusion.
    #[default]
    Rrf,
}

impl FusionMethod {
    /// Configuration and report name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rrf => "rrf",
        }
    }
}

/// How many candidates of each list enter the fusion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "DepthRepr", into = "DepthRepr")]
pub enum FusionDepth {
    /// The full candidate list of each index (PRD §6.6 default).
    #[default]
    All,
    /// The top `n` candidates of each list.
    Top(NonZeroUsize),
}

impl FusionDepth {
    /// Number of candidates a list contributes (`usize::MAX` for [`FusionDepth::All`]).
    #[must_use]
    pub const fn limit(self) -> usize {
        match self {
            Self::All => usize::MAX,
            Self::Top(n) => n.get(),
        }
    }
}

/// Written form of a depth: `"all"` or a positive count.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum DepthRepr {
    Count(usize),
    Name(String),
}

impl TryFrom<DepthRepr> for FusionDepth {
    type Error = String;

    fn try_from(repr: DepthRepr) -> Result<Self, Self::Error> {
        match repr {
            DepthRepr::Name(name) if name == "all" => Ok(Self::All),
            DepthRepr::Name(name) => Err(format!(
                "fusion depth must be \"all\" or a positive count, not {name:?}"
            )),
            DepthRepr::Count(n) => NonZeroUsize::new(n)
                .map(Self::Top)
                .ok_or_else(|| "fusion depth must be at least 1".to_owned()),
        }
    }
}

impl From<FusionDepth> for DepthRepr {
    fn from(depth: FusionDepth) -> Self {
        match depth {
            FusionDepth::All => Self::Name("all".to_owned()),
            FusionDepth::Top(n) => Self::Count(n.get()),
        }
    }
}

/// Versioned fusion parameters, recorded in every combined-mode report (PRD §6.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionParams {
    /// Method.
    #[serde(default)]
    pub method: FusionMethod,
    /// RRF constant `k`.
    #[serde(default = "default_k")]
    pub k: u32,
    /// Candidates per list.
    #[serde(default)]
    pub depth: FusionDepth,
}

const fn default_k() -> u32 {
    DEFAULT_RRF_K
}

impl Default for FusionParams {
    fn default() -> Self {
        Self {
            method: FusionMethod::Rrf,
            k: DEFAULT_RRF_K,
            depth: FusionDepth::All,
        }
    }
}

/// One fused item.
#[derive(Debug, Clone, PartialEq)]
pub struct Fused<T> {
    /// Item key.
    pub item: T,
    /// Fused score.
    pub score: f64,
    /// 1-based rank of the item in each input list, in list order; `None` where the
    /// list does not contain it within its depth.
    pub ranks: Vec<Option<usize>>,
}

/// Fuses ranked `lists` (best first) with Reciprocal Rank Fusion.
///
/// The position of an item in a list is its rank (from 1); an item repeated within one
/// list counts once, at its best rank. Every contribution is summed in ascending rank
/// order, so two items holding the same ranks in different lists get bit-identical
/// scores and fall to the key tie-break.
#[must_use]
pub fn rrf<T: Ord + Clone>(lists: &[&[T]], params: FusionParams) -> Vec<Fused<T>> {
    let mut ranks: BTreeMap<&T, Vec<Option<usize>>> = BTreeMap::new();
    for (list_index, list) in lists.iter().enumerate() {
        for (position, item) in list.iter().take(params.depth.limit()).enumerate() {
            let slot = ranks.entry(item).or_insert_with(|| vec![None; lists.len()]);
            if let Some(rank) = slot.get_mut(list_index)
                && rank.is_none()
            {
                *rank = Some(position + 1);
            }
        }
    }
    let k = f64::from(params.k);
    let mut fused: Vec<Fused<T>> = ranks
        .into_iter()
        .map(|(item, ranks)| {
            let mut held: Vec<usize> = ranks.iter().flatten().copied().collect();
            held.sort_unstable();
            let score = held.iter().map(|&r| 1.0 / (k + rank_f64(r))).sum();
            Fused {
                item: item.clone(),
                score,
                ranks,
            }
        })
        .collect();
    fused.sort_by(|a, b| match b.score.total_cmp(&a.score) {
        Ordering::Equal => a.item.cmp(&b.item),
        other => other,
    });
    fused
}

#[expect(
    clippy::cast_precision_loss,
    reason = "ranks are list positions, far below 2^52"
)]
const fn rank_f64(rank: usize) -> f64 {
    rank as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn params() -> FusionParams {
        FusionParams::default()
    }

    #[test]
    fn the_prd_example_fuses_to_e_a_c_b_d() {
        let text = ["A", "B", "E"];
        let image = ["C", "E", "D"];
        let fused = rrf(&[&text, &image], params());
        let order: Vec<&str> = fused.iter().map(|f| f.item).collect();
        assert_eq!(order, ["E", "A", "C", "B", "D"]);
        let score = |id| fused.iter().find(|f| f.item == id).map(|f| f.score);
        assert_eq!(score("A"), Some(1.0 / 61.0));
        assert_eq!(score("C"), Some(1.0 / 61.0));
        assert_eq!(score("B"), Some(1.0 / 62.0));
        assert_eq!(score("D"), Some(1.0 / 63.0));
        let e = score("E").expect("E");
        assert!((e - (1.0 / 62.0 + 1.0 / 63.0)).abs() < 1e-15);
        let e_ranks = fused
            .iter()
            .find(|f| f.item == "E")
            .map(|f| f.ranks.clone());
        assert_eq!(e_ranks, Some(vec![Some(3), Some(2)]));
    }

    #[test]
    fn depth_limits_the_candidates_of_each_list() {
        let text = ["A", "B", "E"];
        let image = ["C", "E", "D"];
        let depth = FusionParams {
            depth: FusionDepth::Top(NonZeroUsize::MIN.saturating_add(1)),
            ..params()
        };
        let fused = rrf(&[&text, &image], depth);
        let order: Vec<&str> = fused.iter().map(|f| f.item).collect();
        // E is outside the top 2 of the text list, so only its image rank counts.
        assert_eq!(order, ["A", "C", "B", "E"]);
    }

    #[test]
    fn a_repeated_item_counts_once_at_its_best_rank() {
        let fused = rrf(&[&["A", "B", "A"][..]], params());
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].item, "A");
        assert_eq!(fused[0].score, 1.0 / 61.0);
        assert_eq!(fused[0].ranks, vec![Some(1)]);
    }

    #[test]
    fn empty_lists_fuse_to_nothing() {
        let empty: [&str; 0] = [];
        assert!(rrf(&[&empty, &empty], params()).is_empty());
        assert!(rrf::<&str>(&[], params()).is_empty());
    }

    #[test]
    fn depth_reads_all_or_a_positive_count() {
        let parse = |text: &str| toml::from_str::<FusionParams>(text);
        assert_eq!(parse("depth = \"all\"").expect("all"), params());
        assert_eq!(
            parse("depth = 5\nk = 10").expect("count"),
            FusionParams {
                method: FusionMethod::Rrf,
                k: 10,
                depth: FusionDepth::Top(NonZeroUsize::new(5).expect("5")),
            }
        );
        assert!(parse("depth = 0").is_err());
        assert!(parse("depth = \"most\"").is_err());
        assert!(parse("method = \"sum\"").is_err());
        assert!(parse("dept = 3").is_err());
        let json = serde_json::to_string(&params()).expect("json");
        assert_eq!(json, r#"{"method":"rrf","k":60,"depth":"all"}"#);
    }

    /// Ranked lists of distinct keys drawn from a small alphabet, so lists overlap.
    fn lists() -> impl Strategy<Value = Vec<Vec<u8>>> {
        proptest::collection::vec(
            proptest::sample::subsequence((0u8..24).collect::<Vec<_>>(), 0..12).prop_shuffle(),
            1..4,
        )
    }

    proptest! {
        #[test]
        fn every_item_sums_one_over_k_plus_rank_over_the_lists_holding_it(
            lists in lists(),
            k in 0u32..100,
        ) {
            let refs: Vec<&[u8]> = lists.iter().map(Vec::as_slice).collect();
            let fused = rrf(&refs, FusionParams { k, ..params() });
            for f in &fused {
                let mut expected = 0.0;
                let mut ranks: Vec<usize> = lists
                    .iter()
                    .filter_map(|l| l.iter().position(|x| *x == f.item).map(|p| p + 1))
                    .collect();
                ranks.sort_unstable();
                for r in ranks {
                    expected += 1.0 / (f64::from(k) + rank_f64(r));
                }
                prop_assert!((f.score - expected).abs() < 1e-12);
            }
            let distinct: std::collections::BTreeSet<u8> = lists.iter().flatten().copied().collect();
            prop_assert_eq!(fused.len(), distinct.len());
        }

        #[test]
        fn results_descend_by_score_and_break_ties_by_ascending_key(lists in lists()) {
            let refs: Vec<&[u8]> = lists.iter().map(Vec::as_slice).collect();
            let fused = rrf(&refs, params());
            for pair in fused.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                prop_assert!(a.score.total_cmp(&b.score).then(b.item.cmp(&a.item)).is_gt());
            }
        }

        #[test]
        fn scaling_raw_scores_leaves_the_fusion_unchanged(
            scored in proptest::collection::vec(
                proptest::collection::btree_map(0u8..24, 0u32..10_000, 0..12),
                1..4,
            ),
            factor in 1u32..1000,
        ) {
            // Each list is ranked by its own raw scores (descending, key tie-break),
            // the order single-modality search produces. Integer scores and factors
            // keep the scaling exact, so it never changes a list's order.
            let factor = f64::from(factor);
            let ranked = |scale: f64| -> Vec<Vec<u8>> {
                scored
                    .iter()
                    .map(|list| {
                        let mut items: Vec<(f64, u8)> =
                            list.iter().map(|(k, s)| (f64::from(*s) * scale, *k)).collect();
                        items.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
                        items.into_iter().map(|(_, k)| k).collect()
                    })
                    .collect()
            };
            let (plain, scaled) = (ranked(1.0), ranked(factor));
            let fuse = |lists: &[Vec<u8>]| {
                let refs: Vec<&[u8]> = lists.iter().map(Vec::as_slice).collect();
                rrf(&refs, params())
            };
            prop_assert_eq!(fuse(&plain), fuse(&scaled));
        }

        #[test]
        fn list_order_does_not_change_scores_or_ranking(lists in lists()) {
            let refs: Vec<&[u8]> = lists.iter().map(Vec::as_slice).collect();
            let reversed: Vec<&[u8]> = refs.iter().rev().copied().collect();
            let forward: Vec<(u8, f64)> =
                rrf(&refs, params()).into_iter().map(|f| (f.item, f.score)).collect();
            let backward: Vec<(u8, f64)> =
                rrf(&reversed, params()).into_iter().map(|f| (f.item, f.score)).collect();
            prop_assert_eq!(forward, backward);
        }
    }
}
