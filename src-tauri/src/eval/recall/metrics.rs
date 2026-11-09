//! Ranked-recall metrics: hit@1/3/5/10, MRR@10 and p50 latency.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RankedResult {
    /// 1-based rank of the first hit within the top 10, if any.
    pub first_hit_rank: Option<usize>,
    pub latency_ms: f64,
}

pub const HIT_KS: [usize; 4] = [1, 3, 5, 10];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupStats {
    pub n: usize,
    /// Fractions in [0,1] for k = 1, 3, 5, 10.
    pub hit_at: [f64; 4],
    pub mrr: f64,
    pub p50_ms: f64,
}

pub fn group_stats(results: &[RankedResult]) -> GroupStats {
    let n = results.len();
    if n == 0 {
        return GroupStats { n, hit_at: [0.0; 4], mrr: 0.0, p50_ms: 0.0 };
    }
    let rank = |r: &RankedResult| r.first_hit_rank.filter(|k| *k >= 1 && *k <= 10);
    let mut hit_at = [0.0; 4];
    for (slot, k) in hit_at.iter_mut().zip(HIT_KS) {
        *slot = results.iter().filter(|r| rank(r).is_some_and(|x| x <= k)).count() as f64 / n as f64;
    }
    let mrr = results.iter().map(|r| rank(r).map_or(0.0, |x| 1.0 / x as f64)).sum::<f64>() / n as f64;
    let mut lat: Vec<f64> = results.iter().map(|r| r.latency_ms).collect();
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50_ms = if n % 2 == 1 { lat[n / 2] } else { (lat[n / 2 - 1] + lat[n / 2]) / 2.0 };
    GroupStats { n, hit_at, mrr, p50_ms }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(rank: Option<usize>, ms: f64) -> RankedResult {
        RankedResult { first_hit_rank: rank, latency_ms: ms }
    }

    #[test]
    fn group_stats_hit_at_k_mrr_and_p50() {
        let s = group_stats(&[r(Some(1), 10.0), r(Some(4), 30.0), r(None, 20.0), r(Some(12), 40.0)]);
        assert_eq!(s.n, 4);
        assert_eq!(s.hit_at, [0.25, 0.25, 0.5, 0.5]);
        assert!((s.mrr - (1.0 + 0.25) / 4.0).abs() < 1e-9, "rank > 10 counts as a miss");
        assert_eq!(s.p50_ms, 25.0, "median of an even count averages the middle two");
        let empty = group_stats(&[]);
        assert_eq!(empty.n, 0);
        assert_eq!(empty.mrr, 0.0);
    }
}
