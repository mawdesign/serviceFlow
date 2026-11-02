# Storage is accessed only through async repository traits, from day one

The core reads and writes songs, sets and assets only through async repository traits, together with a stream of change events. Phase 1 ships three implementations:

- A desktop filesystem implementation.
- A browser File System Access API implementation.
- A browser OPFS implementation.

Phase 2 adds remote storage and MCP. Those, and the browser storage, are inherently asynchronous. Starting with synchronous traits would mean breaking every caller later.

Storage results come back into state as commands (see ADR-0001), so the UI never waits on storage.
