# Screenshots — every view, per role

> **Stale as of 2026-08-24.** Every capture here predates
> [ADR-0030](../docs/adr/0030-signal-design-language.md), so it shows the old
> Modernist language: square corners, 2px ink borders, warm stone, the
> red-orange accent, Archivo. The *content* of each screen is still an accurate
> index of what each role sees; the *look* is two languages ago. Re-run the
> harness below before using any of these to show somebody the product.

Captured from the running app (the react-native-web build served through the LAN
proxy, backed by the live API and the seeded demo data) at iPhone-ish 390 pt
width, 2× scale, timezone Asia/Karachi. Each folder is one signed-in identity,
because the same binary renders a different app for each: navigation and screen
content derive from the capacities held in the *active* gym.

Regenerate any time: `scripts/seed-demo.sh`, then run the puppeteer harness
(`~/.cache/gym-shots/shoot.mjs` on the dev machine — it signs in through the
real UI, resolves ids through the real API, sets the colour scheme per folder,
and grows the viewport to fit each screen's full scroll content).

All demo accounts use the password `demopassword` — see
[docs/test-accounts.md](../docs/test-accounts.md).

## What the folders show

| Folder | Who | The point |
|---|---|---|
| `00-signed-out` | — | The door in |
| `01-newbie-onboarding` | `newbie@` | An account with no gym yet |
| `02-member` | `member@` | The training loop, end to end |
| `03-trainer` | `trainer@` | Coaching *some* people |
| `04-head-coach` | `headcoach@` | Coaching authority without gym management |
| `05-owner` | `owner@` | Running the gym, including the money |
| `06-admin-riverside` | `admin@` | A second gym — nothing leaks between them |
| `07-multi-gym` | `multi@` | One account, three gyms, different standing in each |
| `08-light-scheme` | `multi@` | The same app in the light scheme |

## 00-signed-out

| File | Shows |
|------|-------|
| `01-sign-in.png` | The accent poster, the form, and (dev builds only) one-tap demo accounts |
| `02-sign-up.png` | One identity; gyms and standings attach to it |

## 01-newbie-onboarding — `newbie@demo.test`

| File | Shows |
|------|-------|
| `01-welcome.png` | The numbered fork: train solo / run a gym / join a gym |
| `02-create-gym.png` | Naming an organisation, with the owner-capacity consequence stated |
| `03-join-with-code.png` | Redeeming an invitation |

## 02-member — `member@demo.test`

| File | Shows |
|------|-------|
| `01-today.png` | One focal element — the workout in progress, or the *next* one computed from the pinned programme — then goals, programmes and reasoned suggestions |
| `02-library.png` | Catalogue with search and modality filters |
| `03-you.png` | Identity, gyms with per-gym capacities, body summary |
| `04-body-tracking.png` | Weight/BMI/body-fat, trend line, inline logging |
| `05-edit-profile.png` | Person-owned profile — it follows you between gyms |
| `06-programme.png` | The immutable published version being trained from |
| `07-exercise-progress.png` | Estimated-1RM trend, computed from logged sets and never stored |
| `08-live-workout.png` | One exercise in focus, plate maths, thumb-sized steppers |
| `09-finished-workout.png` | A completed session |
| `10-your-gyms.png` | The switcher, single-gym case |

## 03-trainer — `trainer@demo.test`

| File | Shows |
|------|-------|
| `01-today.png` | "Your floor": clients, who trained, who needs attention — each with its reason |
| `02-library.png` | Catalogue with authoring |
| `03-people-clients.png` | Only their own clients |
| `04-you.png` | Their account |
| `05-edit-profile-with-coach-section.png` | Coaching headline and specialties, which feed recommendations |

## 04-head-coach — `headcoach@demo.test`

Coaching authority without gym management. **No Activity tab and no
invitations** — both are `can_manage_gym`, which a head coach does not hold, so
those routes are unreachable rather than merely hidden.

| File | Shows |
|------|-------|
| `01-today.png` | Coaching overview |
| `02-library.png` | Catalogue with authoring |
| `03-people-roster.png` | The roster, plus invited-but-not-joined in place |
| `04-you.png` | Head-coach capacity |
| `05-pair-coach-athlete.png` | Pairing, with the access it grants stated before it happens |
| `06-new-exercise.png` | Modality-keyed prescription rules |

## 05-owner — `owner@demo.test`

Five tabs: Today · People · **Billing** · Activity · You. Managers trade the
Library *tab* for Billing (five is the ceiling), and reach the catalogue from
Today's quick actions instead — `08-library-via-quick-action.png` is that route.

| File | Shows |
|------|-------|
| `01-today.png` | The gym's floor |
| `02-people.png` | Roster with capacity badges |
| `03-billing.png` | Monthly recurring, plans, and every invoice state |
| `04-new-plan.png` | Price typed in major units, stored in minor |
| `05-activity-audit.png` | The audit trail as a ledger |
| `06-you.png` | Owner account |
| `07-invite.png` | Inviting with a chosen capacity set |
| `08-library-via-quick-action.png` | The catalogue, reached without a tab |

## 06-admin-riverside — `admin@demo.test`

The *second* gym. Same binary, different tenant, different data.

| File | Shows |
|------|-------|
| `01-today.png` | Riverside's own world |
| `02-people.png` | Riverside's roster |
| `03-billing.png` | Riverside's money — none of Iron Box's |
| `04-activity-audit.png` | Riverside's trail |
| `05-you.png` | Admin capacity |

## 07-multi-gym — `multi@demo.test`

One person, three gyms, different capacities in each. The identity model's proof.

| File | Shows |
|------|-------|
| `01-today-iron-box.png` | Trainer + member in one gym |
| `02-people-both-sides.png` | Coaches *and* is coached |
| `03-you-three-gyms.png` | All three, with per-gym capacities |
| `04-gym-switcher.png` | Switching re-shapes the app; "it does not change who you are" |
| `05-library.png` | Scoped to the active gym |

## 08-light-scheme — `multi@demo.test`

The app follows the device scheme, and both are verified against WCAG AA by
`scripts/verify-contrast.mjs`. The two schemes are deliberately not mirror
images ([ADR-0020](../docs/adr/0020-design-system.md)): **light draws ink rules**
around containers, **dark steps the surface** and keeps lines quiet, because a
near-white 2px border around every box is a wireframe that glares on OLED.

| File | Shows |
|------|-------|
| `01-today.png` | Ink-ruled containers on stone |
| `02-people.png` | Hairline rows, badge tones |
| `03-you.png` | The same hierarchy, inverted ground |
