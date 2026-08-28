# Test accounts

Seeded by `bash scripts/seed-demo.sh` (idempotent — safe to re-run; it logs in
instead of failing if an account already exists).

Four accounts, each holding exactly one capacity, at the one gym this deployment
serves ([ADR-0023](adr/0023-single-gym-deployment.md)).

There are **two** trainers on purpose. Under
[ADR-0034](adr/0034-who-prescribes-and-who-consents.md) a trainer may prescribe
only for their own clients, and a class roster is readable by that class's own
instructor and nobody else — rules that are invisible with one trainer, because
there is no second party for them to be refused against.

**Password for every account below** (including `trainer2@`, whose generated
one-time password was changed to the shared one through its own
change-password flow — ADR-0032's whole point):

```
demopassword
```

Short enough to type on a phone, long enough to clear the server's 12-character
minimum.

---

## Iron Box Strength — one account per capacity

Sign in as each to watch the interface change. **Nothing is hidden by cleverness on
the client — the server independently returns 403/404 for anything these accounts
may not do.**

| Account | Capacity | Tabs | Demonstrates |
|---------|----------|------|--------------|
| `owner@demo.test` | `owner` | Today · Library · People · Activity · You | Full management: roster, audit trail, authoring the catalogue and programmes, pairing coaches, billing |
| `trainer@demo.test` | `trainer` | Today · Library · People · You | Sees **only their own clients** in People, has a coaching profile (headline + specialties feed recommendations), can set a goal for the athlete they coach |
| `trainer2@demo.test` | `trainer` | Today · Library · People · You | The **second** trainer, and the reason the boundaries are visible at all: sign in as them and `trainer@`'s clients are absent, their programmes cannot be prescribed to, and `trainer@`'s class rosters are refused |
| `member@demo.test` | `member` | Today · Library · You | The data-rich one — see below |
| `solo@demo.test` | `member` | Today · Library · You | **No coach, Open Gym plan.** The ADR-0035 case: nobody will ever prescribe them anything, so Today offers *Start your own workout* instead of a programme |

### Signing in to the console

The console (`apps/console`, `npm run dev` → http://localhost:5174) takes the same
accounts. Its navigation turns on two derived rights rather than on role names:

- **`manages`** = `owner` → Billing, Activity, Settings
- **`curates`** = `manages` → Catalogue

Both resolve to "is an owner" since ADR-0036 cut the ladder to three rungs. They stay
as two names because they gate different things and would separate again if a rung
were ever added between owner and trainer.

| Account | Console tabs |
|---------|--------------|
| `owner@demo.test` | Overview · People · Billing · Catalogue · Activity · Settings |
| `trainer@demo.test` / `trainer2@demo.test` | Overview · **Your clients** — the people they coach, not the gym roster, and no staff controls |
| `member@demo.test` / `solo@demo.test` | Overview · Your clients (empty — they coach nobody) |

`admin` and `head_coach` no longer exist (ADR-0036). The server refuses both
strings, the database has a CHECK constraint against them, and `Capacity::parse`
returns nothing for either — so a stale row grants no authority.

Routes are registered only when the capacity allows, so a hand-typed URL falls
through to a redirect rather than rendering a page whose every request 403s —
the same "hidden and unreachable mean the same thing" rule as the app's tabs.

### `member@demo.test` is the demo's centrepiece

Seeded with everything the member experience has:

- **Two assigned programmes** (Beginner Strength, Hypertrophy Block — each pinned to
  a specific published immutable version) and one **open workout** to continue
- **Workout history** — sessions with logged sets (reps × weight, RPE), so the
  per-exercise **estimated-1RM trend** has a real curve
- **Body measurements** — weight history (BMI computed from profile height)
- **Two active goals** with baselines: Back Squat est. 1RM → 100 kg (set by their
  coach, `trainer@`), bodyweight cut → 78 kg (self-set) — progress bars are computed
  from the logged data, never stored
- **"Suggested for you"** on Today: the cut goal surfaces the *Engine Builder*
  conditioning programme, and `owner@`'s trainer profile (a matching specialty,
  and not already their coach) — every suggestion with a readable *because*
- Billing: a paid **Coaching** subscription plus a part-paid **Drop-in** invoice

---

## Classes

The seed puts four on the timetable, so all three accounts have something to see:

| Class | When | Instructor | Places |
|-------|------|-----------|--------|
| Zumba | Mondays 18:00 | `trainer@` | 20 |
| High Intensity Cardio | Tuesdays 07:00 | `trainer@` | 20 |
| Yoga | Wednesdays 19:00 | `owner@` | 15 |
| Pilates | Thursdays 18:30 | `trainer@` | 12 |

A class is a **weekly slot**, not a list of dates — every Monday's Zumba is derived
from the one row. What each account sees:

- **`member@`** — the week's timetable on Today, with the places they already hold,
  and Book on anything with room. A place can be given back right up to the moment
  the class starts.
- **`trainer@`** — "you are teaching" on Today for their own classes, and the
  **roster** (who is booked) for those. Another trainer's roster is refused.
- **`owner@`** — the same timetable plus how full the place is, and *Add a class*.
  Yoga is deliberately taught by `owner@` so the roster is reachable from two
  different staff accounts.

## Who does what (changed 2026-08-26)

The three accounts now have genuinely different jobs, and the interesting thing
to try is that **none of them can do another's**:

| | `owner@` | `trainer@` | `member@` |
|---|---|---|---|
| Write & publish programmes | ✅ | ❌ (reads only) | ❌ |
| Put a member on a programme | ❌ | ✅ own clients | ✅ themselves |
| Decide who coaches whom | ✅ proposes | ❌ | ❌ |
| Accept a coaching pairing | ❌ | ✅ | — |
| Manage the class timetable | ✅ | ❌ | ❌ |

Worth trying in this order:

1. **As `owner@`**, open a member and *Propose a coach*. Pick `trainer@`. Nothing
   happens yet — the pairing does not exist.
2. **Sign in as `trainer@`.** Today shows *A new client for you* with Accept and
   *Not me*. Accept, and only now does the pairing exist. Try accepting as
   `owner@` first if you like: it is refused, because a handshake one person can
   complete alone is not one.
3. **Still as `trainer@`**, open that client and *Put them on a programme*. Then
   sign in as `owner@` and open the same person — there is no such option. The
   owner writes the catalogue; the trainer decides who trains on what.
4. **As `trainer@`, open Programmes.** No **+** button. They read the library and
   assign from it.

## Solo training, and leaving

Two things worth trying that used to be dead ends:

- **Sign up a brand-new account** through the open door and open any published
  programme. *Start this programme* puts you on it with no coach involved — before,
  a member with no coach could read the library and train against none of it.
- **As `member@`, open Membership.** *Cancel membership* and *Switch to Open Gym*
  are both self-service now. Cancelling keeps access to the end of the period
  already paid for and does **not** remove you from the gym — history and
  programmes stay, which is what makes "coached → solo" one step rather than
  leaving and rejoining.

## Things worth trying

1. **Log a workout.** As `member@demo.test`, tap *Continue* on Today (or open a
   programme and start one). Steppers for reps/weight/RPE, a wall-clock rest timer
   that survives backgrounding, finish → it appears in history and moves the
   estimated-1RM trend.
2. **Watch a goal move.** Note the Back Squat goal's percentage, log heavier squat
   sets, finish the workout, return to Today — the bar moved because progress is
   computed from the sets, not stored.
3. **Read a suggestion's reason.** Every row under "Suggested for you" says *why* —
   e.g. "Matches your goal to cut to 78 kg". No unexplained numbers anywhere.
4. **See a trainer's narrow view.** As `trainer@demo.test`, open People: only
   `member@` appears — a second trainer account would see nothing for a client
   they do not coach (relationship, not capacity, gates per-client data).
5. **Write a programme.** As `owner@demo.test`, Today → *Programmes* → the **+**
   button. Add a week, add a workout, add exercises to it — the form changes
   depending on how the movement is measured. Submit it for review, then approve
   it yourself: this gym's single-owner setup allows self-approval (there is no
   second catalogue-manager to hand review to).
6. **Invite someone.** As `owner@demo.test`, open *Invite people*, invite a new
   email address as `trainer` or `member`, copy the code. Sign up as that email,
   then paste the code on the join screen.
7. **Try someone else's code.** Generate a code for one address, then try to redeem
   it while signed in as a different account — refused, because an invitation is
   bound to the email it was sent to.
8. **Re-use a code.** Redeem the same code twice; the second attempt fails.
   Invitations are single-use.
9. **Audit yourself.** After any of the above, open Activity as `owner@` — every
   mutation is there, grouped by day, written in the same transaction as the change.

## Resetting

Re-running `bash scripts/seed-demo.sh` is safe. To start completely clean:

```bash
docker compose down -v && docker compose up -d postgres
# run the server once to apply migrations, then re-seed
```

**These accounts are for local development only.** The password is weak on purpose
and the seed script talks to a local server; never run it against anything real.
