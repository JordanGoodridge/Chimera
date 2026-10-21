//! Work a map loader does on several threads: `par_map`, in order. Both
//! games decode their textures with it (`MapTexture`s with their mip
//! chains: the loads' biggest cost where textures are drawn, F1 of the
//! architecture review).

/// The most threads one `par_map` uses: a load runs beside other work
/// (tests run several at once), and decoding stops scaling past this.
pub const MAX_THREADS: usize = 8;

/// `f` over `items` on up to `MAX_THREADS` threads, results in the order
/// of `items`. Few items (under 8) or one CPU: on this thread. A panic
/// in `f` propagates.
pub fn par_map<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_THREADS);
    if threads < 2 || items.len() < 8 {
        return items.into_iter().map(f).collect();
    }
    let per = items.len().div_ceil(threads);
    let mut chunks: Vec<Vec<T>> = Vec::new();
    let mut items = items.into_iter().peekable();
    while items.peek().is_some() {
        chunks.push(items.by_ref().take(per).collect());
    }
    let f = &f;
    std::thread::scope(|s| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|c| s.spawn(move || c.into_iter().map(f).collect::<Vec<_>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::par_map;

    #[test]
    fn results_keep_the_order_of_the_items() {
        let items: Vec<u32> = (0..100).collect();
        assert_eq!(par_map(items.clone(), |i| i * 2), items.iter().map(|i| i * 2).collect::<Vec<_>>());
        assert_eq!(par_map(vec![3u32, 1], |i| i + 1), [4, 2], "few items stay on this thread");
        assert!(par_map(Vec::<u32>::new(), |i| i).is_empty());
    }
}
