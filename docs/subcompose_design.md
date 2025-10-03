# Subcompose Support Design

## Background

`compose-rt` currently offers a positional memoization runtime similar to Jetpack Compose. Scopes and nodes are registered in a single tree, and recomposition is managed by `Composer` and `Recomposer`. To unlock more advanced layout strategies and staged composition patterns, we need a way to subcompose portions of the UI tree on demand. Jetpack Compose exposes this through `SubcomposeLayout`. This document sketches an initial design for bringing the same capability—without layout concerns—into `compose-rt`.

## Goals

- Allow a scope to **register subcompositions** keyed by an arbitrary slot identifier.
- Enable host scopes to **trigger subcomposition** lazily and multiple times per frame.
- Ensure subcomposition participates in the runtime's **state invalidation and recomposition** pipeline automatically.
- Provide a way to inject a **scoped context** to subcomposed content so it can observe host-specific data (e.g., measurement constraints in UI environments).
- Maintain **single-threaded semantics** consistent with the existing runtime (no cross-thread mutation).

## Non-Goals

- Implementing layout, measurement, or positioning logic.
- Supporting subcomposition across different `Composer` instances (the design stays within one runtime).
- Providing policies for slot reuse or eviction beyond a simple least-recently-used scheme.

## Conceptual Model

We introduce a `Subcomposition` concept. A host scope can register one or more `slot_id` values and compose content under those slots alongside the host's primary child tree. Each slot has its own `SubcomposeScope`, a thin wrapper around the main `Scope`, sharing the same `Composer` but with an isolated key path and contextual data.

```
Host Scope
   ├─ Primary children (existing)
   ├─ Slot "header" subtree (SubcomposeScope)
   └─ Slot "body" subtree  (SubcomposeScope)
```

Slots behave like lazily materialized children: when the host requests content for a slot, we run the associated composable while the host remains the current node. The resulting node key is cached and can be re-used until invalidated.

## Proposed API Surface

### Subcomposition Helper

Add a helper on `Scope`:

```rust
impl<S, N> Scope<S, N>
where
    S: 'static,
    N: ComposeNode,
{
   pub fn subcompose<C>(&self, content: C) -> Subcomposition<N>
    where
        C: Fn(SubcomposeRegistry<N>) + Clone + 'static;
}
```

Calling `subcompose` registers subcomposition bookkeeping on the current node and returns a `Subcomposition`. The host can request subcompositions through the returned registry:

```rust
host.subcompose(slot_id, slot_context, content);
```

### Registry & Scope Types

- `Subcomposition<N>`: lightweight handle stored by the host scope. Internally holds the `NodeKey` of the host node and mutable access to the runtime-managed slot map.
- `SubcomposeRegistry<N>`: exposes the runtime API to the host while the helper closure executes:
   - `fn subcompose<A, C>(&mut self, slot_id: SlotId, ctx: A, content: C) -> SubcomposeHandle`
- `SubcomposeScope<T, N>`: mirrors `Scope<T, N>` but additionally carries the slot context `C` supplied by the host. Exposes `fn context(&self) -> &C`.
- `SubcomposeHandle`: returns metadata (node key, dirty flag) that layout systems can use; in this runtime-only scenario it can simply return `NodeKey` or `ScopeId` for introspection.

### Slot Identifiers & Context

- Introduce a `SlotId` newtype: `pub struct SlotId(u64);` or a generic parameter. The API should accept any `Hash + Eq + 'static` value to follow Compose's flexibility.
- Slot context is a caller-provided value cloned into the subcomposed scope. For runtime-only usage, it's opaque to the runtime and stored in the host node's data.

## Internal Data Structures

### Composer Additions

Extend `Composer` with:

```rust
pub(crate) subcompositions: Map<NodeKey, SubcompositionEntry>;
```

`SubcompositionEntry` contains a map from `SlotId` to `SlotRecord`:

```rust
struct SlotRecord {
    scope_id: ScopeId,
   key: usize,
   node_key: Option<NodeKey>,
}
```

`ScopeId` is needed for reconciliation when re-running the slot, `key` seeds the scope's stable identity, and `node_key` tracks the most recent node associated with the slot.

### Scoped Context Propagation

`SubcomposeScope` wraps `Scope` but replaces calls to `child` so that subcompositions remain under their host node. When a slot runs, we:

1. Push the host node's key/child index.
2. Start a node keyed by the slot's `ScopeId` (reused if available).
3. Inject the slot context into a scoped container accessible via `SubcomposeScope::context()`.
4. Execute the slot content.
5. Record `composable` closure into the slot's `SlotRecord` for recomposition.

This follows the same pattern as `Scope::create_node`, which caches a `Composable` per node.

## Lifecycle

1. **Host Composition**
   - `Scope::subcompose` registers or retrieves a `SubcompositionEntry` keyed by the current node and returns a registry handle bound to that node.

2. **Subcompose Request**
   - When the host calls `registry.subcompose(slot_id, ctx, content)`, the runtime looks up or creates a `SlotRecord` under the host node.
   - If the record exists and isn't dirty, we skip recomposition and return the existing handle.
   - Otherwise, we run `content` in a `SubcomposeScope`, capture the resulting `NodeKey`, and store/replace the `SlotRecord`.

3. **Invalidation**
   - Slot records register state usage just like regular nodes. When a state used inside subcomposition changes, the runtime marks the corresponding slot node as dirty and schedules re-execution of its composable.

4. **Host Recomposition**
   - If the host node is removed, `Composer::end_node` must unmount all associated slots, cleaning up their compositor state.

5. **Cleanup**
   - During `Recomposer::recompose`, unreferenced slots (e.g., not subcomposed in the latest pass) are dropped when their nodes unmount; bookkeeping resets automatically on the next compose.

## Edge Cases & Considerations

- **Nested Subcomposition**: Subcomposed content should be able to call `subcompose` recursively. Since all subcompositions share the same `Composer`, we must ensure the key stack correctly reflects each nested host node.
- **Slot Reuse**: If a host stops requesting a slot temporarily, we can retain the `SlotRecord` and keep it around for quick reuse. After a configurable timeout or when `end_node` runs, old slot records are unmounted.
- **State Sharing**: States defined in subcomposition are isolated because each slot gets its own `ScopeId`. If the host wants to share state, it can pass handles through the slot context.
- **Key Stability**: Hosts must provide deterministic `slot_id`s across recompositions. Failing to do so will cause slot trees to unmount/remount, similar to Compose.
- **Performance**: Maintaining separate `SubcomposeScope`s ensures we only walk the slot tree when needed. Storing `last_used` enables optional trimming to prevent unbounded growth.

## Incremental Implementation Plan

1. **Scaffolding**
   - Add `SubcompositionEntry` storage to `Composer` and expose helper methods to register/lookup slot records.
   - Introduce `Subcomposition` node data struct to hold the entry.

2. **API Layer**
   - Extend `Scope` with `subcompose` and implement `SubcomposeRegistry`/`SubcomposeScope` wrappers.
   - Provide `SlotId` utilities (e.g., `SlotId::from_u64`, `impl From<&'static str>` for convenience).

3. **Execution Path**
   - Implement the slot execution pipeline analogous to `create_node`, ensuring composable caching and dirty tracking integrate with existing maps (`composables`, `states`, `uses`, `used_by`).

4. **Dirty Propagation**
   - Update `Recomposer::recompose` to include slot nodes when gathering dirty nodes, and ensure cleanup logic removes slot state.

5. **Testing & Examples**
   - Add unit tests validating slot reuse, state invalidation, and nested subcompositions.
   - Create an example demonstrating a simple `Subcomposition` that composes `header` and `body` slots on demand (see `examples/subcompose_layout.rs`).
   