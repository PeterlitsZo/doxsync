# ROADMAP

## 2026-09

- [x] Avoid panics when a message contains more map keys than the string pool
      can hold.
- [x] Decode messages transactionally so invalid data does not mutate consumer
      state.
- [x] Enforce size and key limits on the consumer string pool.
- [ ] Limit string lengths in the string pool.
- [ ] Add a FIFO/LRU admission queue for caching TStr values.
- [ ] Support caching BStr values.
- [ ] Represent each path using a parent reference and the current segment to
      reduce memory usage.
