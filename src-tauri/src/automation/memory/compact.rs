// Phase 1: archive cleanup is a no-op. MemoryStore::compact does the work
// (rename memory.md → archives/{ISO8601}.md). Size-based / max-archives
// rotation lands in Phase 2.
