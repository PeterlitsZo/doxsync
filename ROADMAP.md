# ROADMAP

## 2026-09

- [x] Support messages with more map keys than the string pool can hold by
      encoding uncached keys inline with a two-bit tag.
- [x] Decode messages transactionally so invalid data does not mutate consumer
      state.
- [x] Enforce size and key limits on the consumer string pool.
- [ ] Limit string lengths in the string pool.
- [x] Cache TStr values alongside map keys and use pool references when shorter.
- [ ] Add a FIFO/LRU admission queue for caching TStr values.
- [ ] Support caching BStr values.
- [ ] Represent each path using a parent reference and the current segment to
      reduce memory usage.
- [ ] Support `Path::parse("foo.bar.42.'43'")`.
