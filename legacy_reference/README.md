# Legacy Reference

> **Purpose:** Reference files preserved from the original frontend before rebuilding with OpenAPI + TanStack Query.
> 
> **DO NOT import these files directly.** Extract the logic patterns and reimplement them cleanly.

---

## 📁 Contents

### `terminal/TerminalWidget.tsx`
**xterm.js + WebSocket integration**

Key patterns to extract:
- **Lines 22-58:** xterm.js Terminal initialization with FitAddon
- **Line 66:** Binary WebSocket: `ws.binaryType = 'arraybuffer'`
- **Lines 73-80:** ArrayBuffer message decoding with `TextDecoder`
- **Lines 108-112:** Cleanup: `ws.close()` and `terminal.dispose()`

```typescript
// Recommended abstraction for rebuild:
function useTerminal(containerRef: RefObject<HTMLDivElement>) {
  // Extract logic from useEffect into a clean hook
}
```

---

### `stores/appStore.ts`
**SSE Event Handling (The Brain)**

Key patterns to extract:
- **Lines 6-14:** `SwarmEventKind` type union (keep in sync with Rust `events.rs`)
- **Lines 86-161:** `handleEvent` switch statement for state transitions
- **Lines 165-178:** `EventSource` lifecycle with reconnection

```typescript
// In new stack, this becomes a TanStack Query subscription:
const { data: events } = useQuery({
  queryKey: ['swarm-events'],
  queryFn: subscribeToSSE,
});
```

---

### `stores/reactorStore.ts`
**Pipeline Types & Feature API**

Key patterns to extract:
- **Lines 18-27:** `PipelineStage` ordered type union
- **Lines 29-35:** `Feature` interface
- API endpoints: `/api/braindump`, `/api/reactor/features`, `/api/reactor/ignite`

---

### `factory/ReactorPipeline.tsx`
**Stage Visualization Logic**

Key patterns to extract:
- **Lines 5-14:** `PIPELINE_STAGES` ordered array (determines visual order)
- **Lines 16-26:** `stageColors` mapping
- **Lines 111-127:** `FeatureRow` stage state logic (`isActive`, `isPast`, `isCurrent`)

---

### `agents/AgentsView.tsx`
**Agent List Definition**

Key patterns to extract:
- **Lines 4-11:** `agentList` with IDs matching backend agent names
- **Lines 87-94:** Event kind color mapping

---

## 🚀 Rebuild Stack

| Old | New |
|-----|-----|
| Manual fetch calls | OpenAPI generated client |
| Zustand stores | TanStack Query |
| Custom CSS | Shadcn/UI |
| Inline terminal logic | `useTerminal` hook |

---

## 🗑️ When to Delete

Delete this folder after you have:
- [ ] Re-implemented terminal WebSocket handling
- [ ] Re-implemented SSE event subscription
- [ ] Re-created pipeline stage visualization
- [ ] Verified all agent status transitions work
