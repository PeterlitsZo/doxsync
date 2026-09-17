# ROADMAP

## 2026-09

- [ ] Avoid panics when a message contains more map keys than the string pool
      can hold.
- [ ] Decode messages transactionally so invalid data does not mutate consumer
      state.
- [ ] Enforce size and key limits on the consumer string pool.
