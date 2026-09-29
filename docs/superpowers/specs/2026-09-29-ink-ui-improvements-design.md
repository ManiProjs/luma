# Luma Ink UI Improvements Design

**Date**: 2026-09-29
**Author**: Claude
**Status**: Draft

## Executive Summary

This design covers 11 major improvements to Luma's terminal UI while preserving the existing architecture: Core → Session/Events → Ink Presentation. The implementation follows an incremental approach, with each feature building on stable foundations.

## Architecture Constraints

**Preserved**:
- Core owns: agent logic, provider logic, tool execution, planning, memory, configuration
- Session layer: state management, event folding, command routing
- Ink layer: presentation only
- Protocol: existing `ClientMessage`/`ServerEvent` wire format

**Extended**:
- UI state: add scroll offset, setup wizard state
- Actions: add scroll navigation, wizard actions
- Reducer: handle new actions
- Views: add markdown rendering, wizard, improve existing views

## Design System: Terminal-Theme-Agnostic UI

### Current State (Correct)

The existing palette already uses `undefined` for neutrals:
```typescript
// ui/components/theme.ts
export const palette = {
  brand: "yellow",
  highlight: undefined,  // terminal foreground
  accent: "cyan",
  text: undefined,       // terminal foreground
  muted: undefined,      // terminal foreground
  chrome: undefined,     // terminal foreground
  success: "green",
  warning: "yellow",
  danger: "red",
} as const;
```

This is exactly right. Colors sit on glyphs, not on prose.

### Changes Needed

1. **Document the design system** - Add comprehensive comments explaining:
   - Why neutrals are `undefined`
   - Why color sits on markers
   - How to add new markers

2. **Audit existing components** - Ensure no component uses:
   - `dimColor` on already-muted colors
   - Gray for primary content
   - Color to carry meaning alone

3. **Add accessibility helpers** - New functions in `theme.ts`:
   ```typescript
   /** Hierarchical emphasis without color */
   export function emphasis(level: "primary" | "secondary" | "tertiary"): {
     bold?: boolean;
     dim?: boolean;
   }
   ```

### Implementation

- **File**: `ui/components/theme.ts`
- **Changes**: Add documentation, ensure consistency
- **Tests**: Verify no dimmed-muted combinations

## Feature 1: Markdown Rendering

### Requirements

Support in assistant messages:
- Headings (h1-h6)
- Paragraphs
- **Bold**, *italic*
- `inline code`
- Fenced code blocks with language tag
- Bullet lists
- Numbered lists
- Links (display safely)

### Architecture

**No changes to protocol** - Markdown arrives as plain text in `TextDelta` events.

**New component**: `Markdown.tsx`

```typescript
// ui/components/Markdown.tsx
export type MarkdownProps = {
  content: string;
  width: number;
};

export function Markdown({ content, width }: MarkdownProps): React.ReactNode;
```

**Rendering strategy**:
1. Parse markdown into AST (lightweight, no dependencies if possible)
2. Walk AST, emit Ink `<Text>` elements
3. Handle wrapping within width budget
4. Code blocks: syntax-agnostic, distinctive styling

**Integration**:
- Update `MessageLine.tsx` to detect markdown in assistant messages
- Use markdown renderer for assistant role
- Plain text for all other roles

### Code Block Styling

```
┌─ src/lib.rs ───
│ fn main() {
│     println!("hello");
│ }
└────────────────
```

Rules:
- Top-left corner shows filename/language if available
- Left border: `│` (vertical line)
- Use code-aware wrapping (don't break mid-identifier)
- Expandable for very long blocks (future)

### Implementation

- **New file**: `ui/components/Markdown.tsx`
- **New file**: `ui/components/markdown-parser.ts` (pure parsing logic)
- **Modified**: `ui/views/MessageLine.tsx`
- **Tests**: `ui/test/markdown.test.ts`

## Feature 2: Transcript Scrolling

### Requirements

- Scroll through conversation history
- Up/down: line-by-line
- Page up/page down: page-by-page
- Home/end: jump to start/end
- Keep live response visible when at bottom
- Composer stays fixed at bottom

### State Changes

Add to `UiState`:

```typescript
// ui/state/types.ts
export type UiState = {
  // ... existing fields ...

  /** Vertical scroll offset into transcript, in lines. */
  scrollOffset: number;

  /** Whether the user is browsing history (scroll > 0). */
  scrolling: boolean;
};
```

### Actions

```typescript
// ui/state/actions.ts
export type Action =
  // ... existing actions ...
  | { type: "ui/scroll"; by: number }
  | { type: "ui/scroll_to"; index: number }
  | { type: "ui/scroll_home" }
  | { type: "ui/scroll_end" };
```

### Reducer Logic

```typescript
// ui/state/reducer.ts
function scroll(state: UiState, by: number): UiState {
  const totalLines = countLines(state.messages);
  const maxScroll = Math.max(0, totalLines - state.screenHeight);

  const nextOffset = clamp(state.scrollOffset + by, 0, maxScroll);

  return {
    ...state,
    scrollOffset: nextOffset,
    scrolling: nextOffset > 0,
  };
}
```

### Key Bindings

```typescript
// ui/state/commands.ts
// Up/Down in empty composer: scroll transcript
// PageUp/PageDown: always scroll
// Home/End: scroll to start/end
```

### View Changes

```typescript
// ui/views/Conversation.tsx
// Use scrollOffset to slice visible portion
const visible = sliceMessages(messages, scrollOffset, screenHeight);
```

**Challenge**: Ink's `Static` doesn't support partial rendering.

**Solution**:
1. Drop `Static` when scrolling
2. Use `Static` only for settled history when `scrollOffset === 0`
3. Render normally when `scrollOffset > 0`

### Implementation

- **Modified**: `ui/state/types.ts` - add scroll fields
- **Modified**: `ui/state/actions.ts` - add scroll actions
- **Modified**: `ui/state/reducer.ts` - handle scroll actions
- **Modified**: `ui/state/commands.ts` - bind scroll keys
- **Modified**: `ui/views/Conversation.tsx` - visible slice logic
- **Modified**: `ui/views/FullScreen.tsx` - pass screen height
- **Tests**: `ui/test/scroll.test.ts`

## Feature 3: Live vs Settled Architecture

### Current State

```typescript
// ui/state/derive.ts
export function liveFrom(messages: MessageView[], streamingIndex: number | null): number {
  // Returns index where live region starts
}
```

Used in `Conversation.tsx`:
- Static region: `messages.slice(0, liveFrom(...))`
- Live region: `messages.slice(liveFrom(...))`

### Clarification

The model is already correct. What needs improvement:

1. **Better documentation** - Explain the invariant
2. **Explicit types** - Name the regions

### Proposed Types

```typescript
// ui/state/types.ts
export type TranscriptRegion = {
  /** Index of first settled message. */
  settledStart: number;
  /** Index of first live message. */
  liveStart: number;
  /** Total settled lines (for scroll calculation). */
  settledLines: number;
};

export function transcriptRegions(
  messages: readonly MessageView[],
  streamingIndex: number | null
): TranscriptRegion;
```

### Implementation

- **Modified**: `ui/state/derive.ts` - add explicit type
- **Modified**: `ui/views/Conversation.tsx` - use explicit type
- **Tests**: Verify regions computed correctly

## Feature 4: Tool Rendering

### Current State

Tools show:
```
▸ Read src/lib.rs
✓ Read src/lib.rs
  142 lines
```

### Improvements

1. **Compact by default** - Single line
2. **Expandable** - Show full input/output on demand (already have `toggle_expanded`)
3. **State-aware markers**:
   - Running: `▸` (cyan)
   - Success: `✓` (green)
   - Failure: `✗` (red)

### Changes

Already implemented correctly in `MessageLine.tsx`. Minor improvements:

1. **Truncate long inputs better** - Show most relevant part
2. **Duration formatting** - Already good
3. **Tool output preview** - Show first line if short

### Implementation

- **Modified**: `ui/views/MessageLine.tsx` - refine details
- **No state changes**

## Feature 5: Planner Rendering

### Current State

```typescript
// ui/state/derive.ts
export function planChecklist(
  messages: readonly MessageView[],
  plan: string | null,
  width: number
): PlanStep[];
```

### Rendering

```
◆ Plan
  1. Inspect architecture  ✓
  2. Add markdown          ▸
  3. Run tests
```

### Improvements

Already well-designed. Minor refinements:

1. **Visual hierarchy** - Completed steps quieter
2. **Active step emphasis** - Bold or color
3. **Max steps shown** - Already limited to 12

### Implementation

- **Modified**: `ui/views/MessageLine.tsx` - PlanLines component
- **No state changes**

## Feature 6: First-Run Setup Wizard

### Requirements

Guide user through:
1. Detect available providers
2. Select provider
3. Select/configure model
4. Confirm configuration

### Protocol Support

Already exists in `protocol.ts`:
- `get_providers` → `providers`
- `get_models` → `models`
- `test_provider` → `provider_tested`
- `save_setup_config` → `setup_saved`

### State Changes

```typescript
// ui/state/types.ts
export type SetupPhase =
  | "providers"     // selecting provider
  | "models"       // selecting model
  | "configure"    // entering API key, endpoint
  | "test"         // testing connection
  | "confirm"      // review and save
  | "done";        // setup complete

export type SetupState = {
  phase: SetupPhase;
  providers: SupportedProvider[];
  models: SupportedModel[];
  selectedProvider: string | null;
  selectedModel: string | null;
  apiKey: string;
  endpoint: string;
  testing: boolean;
  error: string | null;
};

export type UiState = {
  // ... existing ...

  /** First-run setup wizard. Null when setup is complete. */
  setup: SetupState | null;
};
```

### Actions

```typescript
// ui/state/actions.ts
export type Action =
  // ... existing ...
  | { type: "ui/setup_start" }
  | { type: "ui/setup_select_provider"; id: string }
  | { type: "ui/setup_select_model"; id: string }
  | { type: "ui/setup_set_api_key"; key: string }
  | { type: "ui/setup_set_endpoint"; url: string }
  | { type: "ui/setup_test" }
  | { type: "ui/setup_save" }
  | { type: "core/providers"; providers: SupportedProvider[] }
  | { type: "core/models"; models: SupportedModel[] }
  | { type: "core/provider_tested" }
  | { type: "core/setup_saved" };
```

### View: Setup Wizard

```typescript
// ui/views/Setup.tsx
export function Setup({ state, dispatch }: SetupProps): React.ReactNode {
  switch (state.phase) {
    case "providers":
      return <ProviderSelection ... />;
    case "models":
      return <ModelSelection ... />;
    // ... etc
  }
}
```

### Integration

- In `FullScreen.tsx`, if `state.setup !== null`, render `<Setup />` instead of `<Welcome />`
- Setup wizard uses composer for input
- Keyboard navigation for selections

### Implementation

- **Modified**: `ui/state/types.ts` - add setup state
- **Modified**: `ui/state/actions.ts` - add setup actions
- **Modified**: `ui/state/reducer.ts` - handle setup logic
- **New file**: `ui/views/Setup.tsx`
- **Modified**: `ui/views/FullScreen.tsx` - integrate wizard
- **Tests**: `ui/test/setup.test.ts`

## Feature 7: Composer Improvements

### Current State

```typescript
// ui/views/Composer.tsx
// Already minimal: "› " prompt
```

### Improvements

1. **Status hints** - Already have: "esc to cancel", "? for keys"
2. **Confirmation states** - Show when waiting for approval
3. **History navigation** - Already implemented

### Changes

Already well-designed. Refinements only:

1. **Better hints** - Context-aware suggestions
2. **Draft persistence** - Stash draft when history navigating (already done)

### Implementation

- **Modified**: `ui/views/Composer.tsx` - refine hints
- **No state changes**

## Feature 8: Inline and Fullscreen Modes

### Current State

Already correctly implemented:
- `modes.ts` decides mode based on prompt and TTY
- Both modes share same session
- Inline: prints output, exits
- Fullscreen: interactive session

### Improvements

1. **Better inline output** - Use markdown rendering
2. **Smoother exit** - Ensure cleanup

### Implementation

- **Modified**: `ui/views/Inline.tsx` - use markdown
- **No architecture changes**

## Implementation Priority

### Phase 1: Foundation (Readability)

1. ✅ Design system audit
2. ✅ Document theme.ts
3. ✅ Verify no dim-muted combinations

### Phase 2: Content Rendering

1. Markdown parser (pure function)
2. Markdown component
3. Integrate into MessageLine

### Phase 3: Navigation

1. Add scroll state
2. Add scroll actions
3. Modify Conversation for scrolling
4. Bind scroll keys

### Phase 4: Architecture Refinement

1. Explicit transcript regions type
2. Improved live/settled documentation

### Phase 5: Tool/Planner Polish

1. Refine tool rendering
2. Refine planner rendering
3. Test with real agent output

### Phase 6: Setup Wizard

1. Add setup state
2. Add setup actions
3. Create Setup view
4. Wire into FullScreen

### Phase 7: Final Polish

1. Composer refinements
2. Inline mode markdown
3. Integration testing

## Testing Strategy

After each phase:

1. **Type check**: `npm run typecheck` (or `tsc --noEmit`)
2. **Unit tests**: `npm test`
3. **Manual verification**:
   - Light terminal
   - Dark terminal
   - Narrow width (< 80 cols)
   - Inline mode
   - Fullscreen mode
   - Long conversation
   - Tool execution
   - Markdown output

## Risks and Mitigations

### Risk: Markdown parser complexity

**Mitigation**: Start minimal. Support subset. Use simple regex-based parser initially.

### Risk: Scroll performance

**Mitigation**: Use Ink's `Static` when not scrolling. Only drop to normal rendering when user scrolls.

### Risk: Setup wizard state complexity

**Mitigation**: Keep wizard linear. No branching paths. Clear phases.

## Success Criteria

1. **Readability**: UI readable in dark, light, custom, high-contrast terminals
2. **Markdown**: Assistant messages render formatted content
3. **Scrolling**: User can review previous conversation
4. **Tools**: Tool execution clearly visible
5. **Planner**: Plan progress visible at a glance
6. **Setup**: New user can configure Luma without editing files
7. **Modes**: Both inline and fullscreen work correctly
8. **Tests**: All existing tests pass, new tests cover new features

## Open Questions / Decisions

1. **Markdown parser library** - Use a dependency (`marked` or `markdown-it`)
2. **Code block syntax highlighting** - Yes, with ANSI-safe colors
3. **Setup wizard skip** - Yes, detect existing config file

---

**Next Step**: Begin Phase 1 implementation.
