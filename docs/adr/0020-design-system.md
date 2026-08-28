# ADR-0020: Design system — verified contrast, scheme-specific containment, one focal element

- **Status:** Superseded in part by [ADR-0030](0030-signal-design-language.md) — its
  *method* (a script-verified palette, a stated scheme asymmetry, one focal element
  per screen) survives; its *palette and shape rules* do not.
- **Date:** 2026-07-25
- **Deciders:** Project author

## Context

The product needed a design language of its own, and a reference design was
produced for it (`gym-app/Gym App Views.dc.html`, "Modernist"): sharp corners,
2px ink borders, Archivo, warm stone neutrals, one red-orange accent, uppercase
kickers, tabular numerals. The voice is right and the screen inventory is
useful. Implementing it as drawn, however, surfaced three problems that a static
mockup cannot show:

1. **The dark scheme is the light scheme inverted.** `ink` becomes near-white,
   and every container keeps its 2px border — so each card is outlined in bright
   near-white on near-black. On a phone that is a wireframe, and on OLED in a
   bright gym it glares.
2. **Uniform framing removes hierarchy.** When every box is bordered, nothing is
   emphasised; the eye has nowhere to land. Modernist typography gets hierarchy
   from scale, weight and space, using rules *sparingly* as structure.
3. **Several pairings fail WCAG 2.1 AA.** Measured, not guessed: placeholder
   grey failed in both schemes (3.5:1 and 4.4:1), white-on-accent failed as a
   button label (4.2:1), and the obvious "muted text on an ink card" failed
   because on a light-ink fill the muted tone must get *darker*, not lighter.

## Decision

Keep the reference's voice; change how it is built.

1. **The palette is verified, not eyeballed.** `scripts/verify-contrast.mjs`
   checks every pairing the components actually paint, in both schemes, against
   AA (4.5 text / 3.0 large / 3.0 meaningful non-text). It is part of
   `all-check.sh`. Values that look arbitrary are arbitrary *because the obvious
   choice failed the check* — a comment in `theme.ts` says so.

2. **Containment is scheme-specific, and stated as such.**
   - *Light* draws **ink rules**: a dark 2px line on stone is crisp and cheap,
     so containers are bordered and their grounds sit close in tone.
   - *Dark* uses **tone**: containers step up through `surface → surface2 →
     surface3` and the structural line retreats to a mid-tone (`rule`) that
     still clears 3:1. No near-white boxes.
   This is one asymmetry, encoded once in `containerStyle()`, not a per-screen
   decision.

3. **One focal element per screen.** Containers carry a tone: `quiet` (the
   default) or `focal` (accent-bordered, or accent-filled for a primary action).
   A screen may use `focal` once. Today's focal element is the workout in
   progress — or, when nothing is open, the *next* workout computed from the
   pinned programme, so the screen answers "what now" instead of listing what
   exists.

4. **`onAccent` is a token.** Label colour on an accent fill differs by scheme
   (white on the light accent; near-black on the brighter dark accent, which
   white fails against). Call sites never hard-code it.

## Alternatives considered

- **Implement the reference verbatim.** Fastest, and wrong: it ships a dark mode
  that glares and a palette with known AA failures. The mockups are a
  specification of *voice*, not of contrast ratios.
- **One palette, opacity-derived tones.** Compact, but opacity over an unknown
  ground makes contrast unverifiable — precisely the property being bought here.
- **Follow the system scheme immediately.** Deferred: the provider pins dark
  until every screen is converted, because a half-migrated app would render a
  broken light/dark hybrid. `FOLLOW_SYSTEM` in `theme-context.tsx` flips it.

## Consequences

- **Positive:** dark mode reads as a designed surface rather than a wireframe;
  hierarchy is enforced by the type system rather than by discipline; the
  accessibility claim is reproducible by anyone with `node`; light mode is ready
  and verified before a single screen switches to it.
- **Negative / costs:** two containment idioms to hold in mind; the contrast
  script must be extended whenever a new pairing appears (an unlisted pairing is
  an unverified one, which the file says out loud); AA fixed some tones close
  together (`mut` vs `faint`), so tonal *hierarchy* in text is shallower than a
  designer might draw.
- **Follow-ups:** ~~flip `FOLLOW_SYSTEM` when the last screen converts~~ — **done
  2026-07-25**: every screen now reads tokens through the hooks, the static
  palette export is deleted (a module-level constant cannot answer "which
  scheme is this?", so any remaining import would have rendered dark inside a
  light app), and the app follows the device. Extend the script with any new
  pairing; a WCAG 2.1 AA pass for touch targets, focus order and reduced motion
  remains its own roadmap item (UX-5) — contrast is one criterion, not the
  whole standard.

## References

- Reference design: `gym-app/Gym App Views.dc.html`
- W3C (2018) *Web Content Accessibility Guidelines 2.1* — 1.4.3, 1.4.11
- [ADR-0019](0019-verification-first-development.md) — why the palette gets a suite
- `apps/mobile/src/ui/theme.ts`, `theme-context.tsx`, `components.tsx`
