# Pretext Research Notes

Date: 2026-04-22

## What Pretext Is

`@chenglou/pretext` is a text measurement and layout library designed to avoid DOM reflow-heavy measurement (`getBoundingClientRect`, `offsetHeight`) by separating work into:
- `prepare()` (one-time text/font analysis)
- `layout()` (fast repeated width/height calculations)

Primary sources:
- https://github.com/chenglou/pretext
- https://pretextjs.net/

## Why It Fits This Rewrite

The UI v2 has dense operational labels and card headings that need predictable wrapping without layout jitter.

Using Pretext allows us to:
- estimate wrapped line count/height before browser layout reflow loops
- keep card/header heights stable across resize
- prevent abrupt label shift in control-heavy views

## How It Was Incorporated in UI v2

Implementation location:
- `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/src/lib/pretext.ts`
- `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/src/components/layout/pretext-title.tsx`

Integration pattern:
1. `prepare(text, font, options)` is memoized per text payload.
2. `layout(prepared, width, lineHeight)` runs on resize-derived width.
3. Calculated height drives controlled title block height and overflow behavior.

Active usage in v2:
- metric cards
- section headers

This keeps typography consistent and avoids ad-hoc overflow handling.

