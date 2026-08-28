# ADR-0030: The Signal design language — supersedes the Modernist palette and shape rules

- **Status:** Accepted
- **Date:** 2026-08-24
- **Deciders:** Project author
- **Supersedes:** [ADR-0020](0020-design-system.md) (its *shape* and *palette* decisions;
  its *method* is kept and extended)

## Context

ADR-0020 took a reference mockup ("Modernist") and made it safe: it kept the
voice — sharp corners, 2px ink borders, warm stone neutrals, one red-orange
accent, Archivo — and fixed the three things a static mockup cannot show
(an inverted dark scheme, uniform framing that removes hierarchy, and pairings
that failed WCAG AA).

That was the right fix for the wrong target. With every screen built, the
language's own problems became visible in a way no single mockup showed:

1. **Everything is a box.** A 2px rule under every section heading, a 2px
   border round every card, input, chip and badge. On a phone this reads as a
   form; on the console it reads as a spreadsheet. There is no elevation, no
   softness, and nothing about the surface says "gym" rather than "invoice".
2. **The palette dates the product.** Warm stone plus a red-orange accent is a
   specific and now very common look, and red-orange is doing double duty:
   it is the brand *and* the alarm. `chip-late` and the primary CTA were the
   same colour, so "pay this now" and "start your workout" shouted equally.
3. **There is no vocabulary for status.** One `danger` token and no ground to
   put it on, so every screen improvised: `invoiceTone` in the app mapped
   *overdue* to `accent` and *paid* to `outline`, which is to say the ledger's
   two most important states were told apart by the presence of a border.
4. **Two clients that agreed about colour disagreed about everything else.**
   The generated tokens kept the palettes in step and nothing kept the shapes
   in step, so the console grew its own chip, its own button and its own idea
   of what a section heading looks like.

Separately, the reference itself is one of the looks that a general-purpose
generator produces when given no direction — hairline rules, zero radius,
broadsheet columns. Distinctiveness was a real requirement here and the
starting point was working against it.

## Decision

Keep ADR-0020's **method** — a verified palette, a stated scheme asymmetry, one
focal element per screen, rules enforced by a script rather than remembered.
Replace what that method was being applied to.

1. **A new palette: violet-indigo, with one lime that only means *live*.**
   The brand is a violet-indigo; `signal` (an electric lime) is reserved for
   *happening right now* — a running rest timer, a session in progress,
   somebody on the floor. Reserving it is what makes it mean anything: the
   rest timer turning lime the moment a rest ends is a notification you can
   read from across the rack without reading a word.

   `signal` is only ever painted as a **fill carrying `onSignal` text**, with
   an `onSignal` hairline defining its edge. That is not an aesthetic
   preference: a lime bright enough to read as "live" on near-black cannot also
   be a 3:1 marker on white, and no single value is both. One pairing, verified
   once, identical in both schemes.

2. **Semantic colour is a triplet, not a colour.** `danger` / `dangerHi` /
   `dangerInk`, and the same for `success` and `warn` — a fill, a wash, and the
   text that goes on the wash. A status chip is now a token lookup instead of a
   per-screen invention, and *overdue* is rose everywhere rather than being
   whatever the accent happens to be.

3. **Containment is fill and elevation, not outline.** A card is a lighter
   ground with a hairline and a soft cast. `rule` is redefined as a **control
   edge** — the thing that tells you where an input or a pressable begins,
   which is what WCAG 1.4.11 actually asks 3:1 for — and `line` is a decorative
   hairline. The scheme asymmetry survives in a new form: light lifts a white
   card off a tinted ground; dark cannot cast a shadow on near-black, so
   `t.elevation()` returns nothing there and the surface steps do the work.

4. **Corners come from a scale.** `t.radius`, six values. The old rule was "no
   radius anywhere" and it was enforced by a script; the new rule is "every
   radius comes from the scale" and it is enforced by the same script. The
   check was inverted in the same commit as the decision, which is the whole
   point of having written it as code.

5. **Two typefaces with one job each.** Bricolage Grotesque for display —
   titles, stat values, the weight on the bar; Plus Jakarta Sans for UI and
   body. The largest thing on almost every screen in this product is a number
   somebody is reading mid-set, and a display face with a compressed,
   high-energy skeleton makes that number carry the personality instead of
   asking decoration to.

6. **The console gets the same language, and almost no icons.** The generated
   token file now carries radius, elevation and type as well as colour, so the
   two clients cannot drift on shape either. But the console is *read*, not
   browsed: a glyph beside a word in a table is a second thing to decode, and a
   row of small pictures next to "Approve" and "Retire" makes a dense screen
   busier without making it faster. The only marks in the whole console are the
   brand square in the rail and the caret on a sort header. Status is a pill
   with the status written in it.

## Alternatives considered

- **Retune the existing palette and keep the shapes.** Cheapest, and it would
  have fixed complaint (2) and none of the others. The boxes are the bigger
  problem: a warmer accent inside the same wireframe is still a wireframe.
- **Adopt a component library (shadcn, Tamagui, MUI).** It would supply the
  shapes and cost the thing that makes this codebase's UI verifiable — the
  palette lives in one file, is checked against AA by a script, and generates
  the second client's tokens. A library puts its own vocabulary between those
  two, and the contrast suite would be checking a palette the components do not
  necessarily use.
- **Keep one accent and let it carry status too.** That is what the old
  language did, and it is why an overdue invoice and a primary button were the
  same red. Semantic triplets cost eight tokens and remove a whole class of
  per-screen improvisation.
- **A green or blue brand.** Both were reasonable and both are what a gym
  product looks like by default. The violet-and-lime pairing is the risk taken
  here on purpose: it is unmistakable, it is legible in both schemes, and the
  lime earns its place by having exactly one meaning rather than being a second
  decorative colour.

## Consequences

- **Positive:** one language across two clients, enforced rather than agreed;
  status has a vocabulary, so a new screen with a new state has an obvious
  right answer; `signal` gives the product a colour that means something, which
  is rarer and more useful than a colour that looks nice; the console stops
  being a monochrome table dump.
- **Negative / costs:** every screen changed, and screens not individually
  reworked inherited the language through tokens and primitives rather than
  being redesigned — several forms are correct and unremarkable. Two font
  families are two more bundled assets and a second webfont request on the
  console. `signal`'s single-pairing rule is a real constraint that will be
  tempting to break the first time somebody wants a green dot.
- **Verification changed with the decision, not after it:**
  - `verify-contrast.mjs` now checks 100 pairings (was 50), including every
    semantic triplet, both grounds for each muted tone, the recessed input
    ground, and — new — **the composite of a label softened with `opacity`**.
    Softening a second-tier label to 85% on a coloured fill is the standard way
    to get a second tier, and the standard way to fall out of AA without
    noticing, because the *token* you wrote down still passes. The script
    composites and checks what actually ships.
  - `verify-design-consistency.mjs` gained three checks and inverted one: radii
    must come from `t.radius`; border widths from `t.border`; elevation from
    `t.elevation()`; and **every sized text style must name its font file** —
    that last one immediately found five styles in `program/[version].tsx` that
    had been rendering in the device's system font since the file was written.
    They were invisible next to a neutral grotesque and would have been obvious
    next to a display face — after shipping.
  - `generate-console-tokens.mjs` now emits radius, elevation and both type
    families alongside the palette, so the drift check in `all-check.sh` covers
    shape as well as colour.

## References

- [ADR-0020](0020-design-system.md) — the language this replaces, and the
  method it keeps
- [ADR-0019](0019-verification-first-development.md) — why a palette gets a suite
- W3C (2018) *Web Content Accessibility Guidelines 2.1* — 1.4.3, 1.4.11
- `apps/mobile/src/ui/theme.ts`, `components.tsx`; `apps/console/src/app.css`,
  `src/ui.tsx`
