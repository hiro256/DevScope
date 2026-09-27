//! Shared timing only; detection, cleanup, and redraw remain domain-specific.
use crate::app::TransientEmphasisPhase;
use std::time::Duration;

pub(super) fn transient_emphasis_phase(elapsed: Duration) -> TransientEmphasisPhase {
    use TransientEmphasisPhase::*;
    if elapsed < Duration::from_millis(750) {
        Hot
    } else if elapsed < Duration::from_millis(1500) {
        Warm
    } else if elapsed < Duration::from_millis(2250) {
        Settling
    } else if elapsed < Duration::from_millis(3000) {
        Cooling
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_phase_boundaries() {
        use TransientEmphasisPhase::*;
        for (ms, phase) in [
            (0, Hot),
            (749, Hot),
            (750, Warm),
            (1499, Warm),
            (1500, Settling),
            (2249, Settling),
            (2250, Cooling),
            (2999, Cooling),
            (3000, None),
        ] {
            assert_eq!(transient_emphasis_phase(Duration::from_millis(ms)), phase);
        }
        assert_eq!(transient_emphasis_phase(Duration::MAX), None);
    }
}
