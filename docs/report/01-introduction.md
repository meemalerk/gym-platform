# 1. Introduction

## 1.1 The real-world problem

Gyms that sell coaching run two businesses at once: the administrative one
(memberships, billing, timetables) and the professional one (designing programmes,
assigning them, watching what people do, adjusting). The software market has split
along that seam, and the split is the problem this project addresses.

On one side sit gym management platforms (Mindbody, Glofox, PushPress), whose
programming features are thin: a workout is a flat template, cloned and edited,
with no record of what a member was prescribed last March. On the other sit
coaching tools (Trainerize, TrueCoach, TrainHeroic), which handle the professional
business but assume a single coach with a client list rather than an organisation
with staff and governance. Neither treats a training programme as a versioned
artefact with a review step and an immutable published form.

That absence has a concrete cost. A coach who edits a programme a member is
following silently rewrites what that member was told to do, so when the history is
read back — to compute a strength trend or review adherence — the prescription it
is compared against no longer exists in the form it was given. Since progression in
resistance training is explicitly a cycle of prescribing, evaluating against the
prescription, and adjusting (Kraemer and Ratamess, 2004), losing the prescription
removes the term the evaluation was against.

## 1.2 Why it matters

**Fitness applications lose their users quickly.** Attrition is a defining property
of digital health tools rather than an occasional disappointment (Eysenbach, 2005),
and remains unsolved seventeen years later (Amagai *et al.*, 2022). Software that
cannot show a member evidence of their own progress has little to hold them with.

**What is recorded determines what can ever be known.** Monitoring progress against
a goal is among the better-evidenced behaviour-change strategies available
(Harkin *et al.*, 2016), but only against data the application chose to keep. A
design that overwrites prescriptions forecloses the analysis before anyone asks.

**The domain is health-adjacent.** A member may reasonably ask *why* they were
told to do something — answerable if the assigned version still exists and the
suggestion carries a readable reason, unanswerable if the plan was edited in place
or the recommendation came from an opaque ranker (Zhang *et al.*, 2019).

## 1.3 Aim, objectives and scope

**Aim:** to design, build and evaluate a gym coaching and management platform
whose core domain — programmes, coaching relationships and execution history — is
verifiably correct, and to justify its design against the relevant literature.

**Objectives:**

- **O1 — Tenancy.** Isolation enforced at two independent layers, so cross-tenant
  access is not representable through the normal interface.
- **O2 — Identity.** One account holding a *set* of capacities, with every
  permission question answered through a single capability surface.
- **O3 — Programme model.** Versioned authoring with a review workflow, in which a
  published version is immutable and editing produces a new draft.
- **O4 — Coaching and execution.** Relationships gating access to personal data,
  assignment of a *specific* version, and append-only logging that is idempotent
  under replay.
- **O5 — Member value.** Progress computed from immutable history rather than
  stored, plus recommendations carrying a readable reason.
- **O6 — Verification.** Every claim above demonstrable by an executable script.

**Scope.** The project covers the member application, a staff console, the server
and data model, billing, classes and the verification harness. Three exclusions are
deliberate: nutrition is coach-authored guidance, never generated prescription;
recommendation is deterministic rather than learned; and the offline
*synchronisation transport* is specified but unbuilt — the idempotent write path it
needs is built and tested, the queue that would drive it is not.

## 1.4 Research questions

1. What do existing mobile fitness applications implement, and what do they omit
   that coached gym training requires?
2. What must be recorded, and what may be derived, for a member's training history
   to remain trustworthy over time?
3. How should a training programme be modelled so that logged history stays
   interpretable after the programme changes?
4. What sustains a member's engagement, and how far can gamification be relied on
   to provide it?
5. Where is transparency preferable to accuracy in personalised recommendation?

## 1.5 What the literature review found

Chapter 2 answers these across eight themes. Four findings shaped the design, and
the most useful were *rejections*.

**Existing applications are thin where this project is thick.** They implement an
average of five of twenty-three established behaviour change techniques
(Middelweerd *et al.*, 2014), and most describe fewer than four (Conroy, Yang and
Maher, 2014); neither review found version-controlled prescription.

**What is recorded must come from the athlete; what is derived must be honest about
its error.** Coaches systematically misjudge proximity to failure by observation
(Emanuel *et al.*, 2022), so effort is a field the member fills in at the time.
One-repetition-maximum testing is reliable but costly to administer often
(Grgic *et al.*, 2020), justifying a continuous estimate labelled as one.

**Motivation mechanics are context-dependent, not a substitute for substance.**
Gamification's effects depend heavily on context (Hamari, Koivisto and Sarsa, 2014),
while self-determination theory locates durable motivation in competence and autonomy
rather than extrinsic reward (Ryan and Deci, 2000) — so the platform invests in
visible computed progress rather than points.

**Availability is a usability requirement, not an optimisation.** Local-first design
argues for the device as primary copy (Kleppmann *et al.*, 2019), while the taxonomy
of highly available transactions maps which guarantees survive a partition
(Bailis *et al.*, 2013).

## 1.6 Approach

The system is a Rust modular monolith over PostgreSQL, with a React Native member
application and a React staff console. Rust was chosen because the domain layer is
where correctness is claimed, and its ownership discipline is both formally sound
(Jung *et al.*, 2018) and empirically adhered to in practice (Evans, Campbell and
Soffa, 2020). Invariants are pushed into types where a type can carry them
(Wadler, 2015) and into database constraints where they are properties of a set of
rows. Development was verification-first: every non-trivial claim has an executable
script asserting it against a live server, and every decision is an ADR.

## 1.7 Outcome

The delivered system implements tenancy, identity, the versioned programme model,
coaching relationships, execution logging, computed progress, billing and classes.
Members without a coach can build and log their own sessions, so training is recorded
for the membership tier gyms sell most of. Correctness is evidenced by roughly 1,100
assertions across unit tests and verification scripts. Chapter 7 evaluates this
against O1–O6 and is candid about what remains unbuilt.

## 1.8 Report structure

**Chapter 2** critically reviews the literature and identifies the research gap.
**Chapter 3** states the requirements. **Chapter 4** describes the methodology,
including the declared use of generative AI assistance. **Chapter 5** presents the
design and **Chapter 6** the implementation and testing. **Chapter 7** evaluates
the project against its objectives; **Chapter 8** concludes.

## References

Amagai, S., Pila, S., Kaat, A. J., Nowinski, C. J. and Gershon, R. C. (2022)
'Challenges in participant engagement and retention using mobile health apps:
literature review'. *Journal of Medical Internet Research*, 24 (4), e35120.
Available from: https://www.jmir.org/2022/4/e35120/ [Accessed 27 August 2026].

Bailis, P., Davidson, A., Fekete, A., Ghodsi, A., Hellerstein, J. M. and Stoica,
I. (2013) 'Highly available transactions: virtues and limitations'. *Proceedings
of the VLDB Endowment*, 7 (3), pp. 181–192. Available from:
https://arxiv.org/abs/1302.0309 [Accessed 19 July 2026].

Conroy, D. E., Yang, C.-H. and Maher, J. P. (2014) 'Behavior change techniques in
top-ranked mobile apps for physical activity'. *American Journal of Preventive
Medicine*, 46 (6), pp. 649–652. Available from:
https://doi.org/10.1016/j.amepre.2014.01.010 [Accessed 27 August 2026].

Emanuel, A., Har-Nir, I., Obolski, U. and Halperin, I. (2022) 'Seeing effort:
assessing coaches' prediction of the number of repetitions in reserve before
task-failure'. *Sports Medicine – Open*, 8 (1). Available from:
https://doi.org/10.1186/s40798-022-00516-w [Accessed 19 July 2026].

Evans, A. N., Campbell, B. and Soffa, M. L. (2020) 'Is Rust used safely by
software developers?'. *Proceedings of the 42nd International Conference on
Software Engineering (ICSE)*. Available from: https://arxiv.org/abs/2007.00752
[Accessed 19 July 2026].

Eysenbach, G. (2005) 'The law of attrition'. *Journal of Medical Internet
Research*, 7 (1), e11. Available from: https://www.jmir.org/2005/1/e11/
[Accessed 27 August 2026].

Grgic, J., Lazinica, B., Schoenfeld, B. J. and Pedisic, Z. (2020) 'Test–retest
reliability of the one-repetition maximum (1RM) strength assessment: a systematic
review'. *Sports Medicine – Open*, 6 (1). Available from:
https://doi.org/10.1186/s40798-020-00260-z [Accessed 19 July 2026].

Hamari, J., Koivisto, J. and Sarsa, H. (2014) 'Does gamification work? — a
literature review of empirical studies on gamification'. *Proceedings of the 47th
Hawaii International Conference on System Sciences*, pp. 3025–3034. Available
from: https://doi.org/10.1109/HICSS.2014.377 [Accessed 27 August 2026].

Harkin, B., Webb, T. L., Chang, B. P. I., Prestwich, A., Conner, M., Kellar, I.,
Benn, Y. and Sheeran, P. (2016) 'Does monitoring goal progress promote goal
attainment? A meta-analysis of the experimental evidence'. *Psychological
Bulletin*, 142 (2), pp. 198–229. Available from:
https://doi.org/10.1037/bul0000025 [Accessed 27 August 2026].

Jung, R., Jourdan, J.-H., Krebbers, R. and Dreyer, D. (2018) 'RustBelt: securing
the foundations of the Rust programming language'. *Proceedings of the ACM on
Programming Languages*, 2 (POPL). Available from: https://doi.org/10.1145/3158154
[Accessed 19 July 2026].

Kleppmann, M., Wiggins, A., van Hardenberg, P. and McGranaghan, M. (2019)
'Local-first software: you own your data, in spite of the cloud'. *Proceedings of
the 2019 ACM SIGPLAN International Symposium on New Ideas, New Paradigms, and
Reflections on Programming and Software (Onward!)*. Available from:
https://www.inkandswitch.com/local-first/ [Accessed 19 July 2026].

Kraemer, W. J. and Ratamess, N. A. (2004) 'Fundamentals of resistance training:
progression and exercise prescription'. *Medicine & Science in Sports & Exercise*,
36 (4), pp. 674–688. Available from: https://pubmed.ncbi.nlm.nih.gov/15064596/
[Accessed 27 August 2026].

Middelweerd, A., Mollee, J. S., van der Wal, C. N., Brug, J. and te Velde, S. J.
(2014) 'Apps to promote physical activity among adults: a review and content
analysis'. *International Journal of Behavioral Nutrition and Physical Activity*,
11, 97. Available from: https://doi.org/10.1186/s12966-014-0097-9
[Accessed 27 August 2026].

Ryan, R. M. and Deci, E. L. (2000) 'Self-determination theory and the facilitation
of intrinsic motivation, social development, and well-being'. *American
Psychologist*, 55 (1), pp. 68–78. Available from:
https://doi.org/10.1037/0003-066X.55.1.68 [Accessed 27 August 2026].

Wadler, P. (2015) 'Propositions as types'. *Communications of the ACM*, 58 (12),
pp. 75–84. Available from: https://doi.org/10.1145/2699407
[Accessed 19 July 2026].

Zhang, S., Yao, L., Sun, A. and Tay, Y. (2019) 'Deep learning based recommender
system: a survey and new perspectives'. *ACM Computing Surveys*, 52 (1),
pp. 1–38. Available from: https://arxiv.org/abs/1707.07435
[Accessed 19 July 2026].
