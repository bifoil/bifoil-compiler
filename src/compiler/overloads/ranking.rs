//! Shared overload ordering for ordinary calls and closed capture call profiles.

pub(in crate::compiler) struct OverloadRank {
    pub(in crate::compiler) conversions: Vec<u8>,
    pub(in crate::compiler) omitted_defaults: usize,
    pub(in crate::compiler) variadic: bool,
    pub(in crate::compiler) generic: bool,
}
impl OverloadRank {
    fn dominates(&self, other: &Self) -> bool {
        self.conversions.iter().zip(&other.conversions).all(|(a, b)| a <= b)
            && self.conversions.iter().zip(&other.conversions).any(|(a, b)| a < b)
    }
    fn tie_breaker(&self) -> (bool, usize, bool) {
        (self.variadic, self.omitted_defaults, self.generic)
    }
}

/// A tie-breaker applies only when every surviving conversion vector matches.
/// Unknown or ambiguous profiles remain conservative at the caller.
pub(in crate::compiler) fn best_match<'a>(ranks: impl IntoIterator<Item = &'a OverloadRank>) -> Option<usize> {
    let mut input = ranks.into_iter();
    let first = input.next()?;
    let Some(second) = input.next() else { return Some(0) };
    let mut ranks = vec![first, second];
    ranks.extend(input);
    let mut survivors = ranks
        .iter()
        .enumerate()
        .filter_map(|(index, rank)| (!ranks.iter().any(|other| other.dominates(rank))).then_some(index))
        .collect::<Vec<_>>();
    if survivors.len() > 1
        && survivors
            .iter()
            .all(|index| ranks[*index].conversions == ranks[survivors[0]].conversions)
    {
        let best = survivors.iter().map(|index| ranks[*index].tie_breaker()).min().unwrap();
        survivors.retain(|index| ranks[*index].tie_breaker() == best);
    }
    if survivors.len() == 1 { survivors.pop() } else { None }
}
