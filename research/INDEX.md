# Research library

Primary sources this project's design decisions lean on, downloaded as PDFs so the
report can cite page-stable copies. Every file was fetched from the publisher's
open-access channel (arXiv, Ink & Switch, RFC Editor, OWASP, SpringerOpen CC-BY,
authors' own institutional pages) and verified to be a real PDF at download time.
These are cited from [docs/assignment-report.md](../docs/archive/assignment-report.md)
in author–date form.

| # | File | Reference | Why it is in this repo |
|---|------|-----------|------------------------|
| 1 | `01-crdt-overview-preguica-2018.pdf` | Preguiça, N. (2018) *Conflict-free Replicated Data Types: An Overview*. arXiv:1806.10254 | The theory behind [ADR-0008](../docs/adr/0008-offline-sync-operation-log.md)'s **rejection** of blanket CRDTs: CRDTs guarantee convergence, not domain validity. We keep the operation-log idea (commutative, replayable operations, client-generated IDs) but resolve conflicts with domain rules instead of algebraic merge — a workout set is append-only fact, not a register to merge. |
| 2 | `02-highly-available-transactions-bailis-2013.pdf` | Bailis, P. et al. (2013) *Highly Available Transactions: Virtues and Limitations*. arXiv:1302.0309; PVLDB 7(3) | Maps which transactional guarantees survive network partitions. Grounds the split we ship: the **server** stays single-Postgres serializable (memberships, versions, billing need it), while the **member app** gets availability via the op-log — exactly the "unachievable vs. achievable" line this paper draws. |
| 3 | `03-local-first-kleppmann-2019.pdf` | Kleppmann, M., Wiggins, A., van Hardenberg, P. and McGranaghan, M. (2019) *Local-first software: You own your data, in spite of the cloud*. Onward! 2019 | The design ideals behind the offline-first member app ([ADR-0008](../docs/adr/0008-offline-sync-operation-log.md)): the gym floor has no Wi-Fi, so the phone is the primary replica of *your own training data* and the server is the sync point and multi-tenant authority. Adopted selectively — coaching and tenancy are inherently server-authoritative. |
| 4 | `04-microservices-dragoni-2016.pdf` | Dragoni, N. et al. (2016) *Microservices: yesterday, today, and tomorrow*. arXiv:1606.04036 | An honest cost accounting of microservices (operational complexity, distributed transactions, versioned interfaces). Those costs are precisely what [ADR-0003](../docs/adr/0003-modular-monolith.md) refuses to pay at this scale: we take the paper's *modularity* goals inside one deployable — the modular monolith. |
| 5 | `05-rust-safety-evans-2020.pdf` | Evans, A. N., Campbell, B. and Soffa, M. L. (2020) *Is Rust Used Safely by Software Developers?* ICSE 2020; arXiv:2007.00752 | Empirical evidence for [ADR-0002](../docs/adr/0002-backend-language-rust.md): most real-world Rust is safe Rust, and `unsafe` concentrates in a small, auditable minority. This workspace is 100 % safe Rust. |
| 6 | `06-deep-learning-recsys-survey-zhang-2017.pdf` | Zhang, S., Yao, L., Sun, A. and Tay, Y. (2019) *Deep Learning based Recommender System: A Survey and New Perspectives*. ACM Computing Surveys 52(1); arXiv:1707.07435 | The road **not** taken, surveyed. Learned recommenders are opaque, data-hungry and unaccountable — wrong for a coached-training context where a member must be able to read *why* something was suggested. Grounds [ADR-0017](../docs/adr/0017-deterministic-recommendations.md). |
| 7 | `07-rfc9106-argon2.pdf` | Biryukov, A., Dinu, D., Khovratovich, D. and Josefsson, S. (2021) *RFC 9106: Argon2 Memory-Hard Function for Password Hashing and Proof-of-Work Applications*. IETF | The standard behind our password storage. Sign-up/sign-in hash with **Argon2id** per this RFC's recommendation (§4) — memory-hardness defends against GPU cracking in a way bcrypt/PBKDF2 cannot. |
| 8 | `08-rfc9700-oauth2-security-bcp.pdf` | Lodderstedt, T., Bradley, J., Labunets, A. and Fett, D. (2025) *RFC 9700: Best Current Practice for OAuth 2.0 Security*. IETF | Our token model is first-party, but its hazards are OAuth's hazards. **Refresh-token rotation with reuse detection revoking the whole token family** — the platform's compare-and-swap rotation invariant — is this BCP's recommendation implemented literally: losing the rotation race is treated as theft. |
| 9 | `09-owasp-asvs-4.0.3.pdf` | OWASP (2021) *Application Security Verification Standard 4.0.3* | The checklist the security posture is audited against: V2 (Argon2id, no password logging), V3 (session rotation, server-side revocation), V4 (deny-by-default `Capabilities::can_*`, tenant 404-not-403), V7 (append-only audit log written in the mutating transaction), V8 (RLS as defence-in-depth). |
| 10 | `10-seeing-effort-rir-coaches-2022.pdf` | Emanuel, A., Har-Nir, I., Obolski, U. and Halperin, I. (2022) *Seeing Effort: Assessing Coaches' Prediction of the Number of Repetitions in Reserve Before Task-Failure*. Sports Medicine – Open 8 | Why sets carry **RPE/RIR** and why the platform records them longitudinally: coaches systematically misjudge how close an athlete is to failure by observation alone. The athlete's own logged RIR is signal a coach cannot see from the floor. |
| 11 | `11-1rm-reliability-grgic-2020.pdf` | Grgic, J., Lazinica, B., Schoenfeld, B. J. and Pedisic, Z. (2020) *Test–Retest Reliability of the One-Repetition Maximum (1RM) Strength Assessment: A Systematic Review*. Sports Medicine – Open 6 | Grounds the progress engine's choice to show **estimated** 1RM trends (Epley on logged sets, capped at 12 reps) rather than demand frequent true 1RM testing: 1RM tests are reliable but costly and risky to administer often; rep-derived estimates riding data members log anyway are the pragmatic longitudinal signal. |
| 12 | `12-rustbelt-jung-2018.pdf` | Jung, R., Jourdan, J.-H., Krebbers, R. and Dreyer, D. (2018) *RustBelt: Securing the Foundations of the Rust Programming Language*. POPL 2018 | The formal footing under [ADR-0002](../docs/adr/0002-backend-language-rust.md): machine-checked proof that Rust's ownership/borrowing discipline is sound. Complements the empirical paper (#5) — one says the guarantees hold, the other says developers actually stay inside them. |
| 13 | `13-rest-fielding-dissertation-2000.pdf` | Fielding, R. T. (2000) *Architectural Styles and the Design of Network-based Software Architectures*. PhD dissertation, UC Irvine, ch. 5 | The primary source for REST, which the API commits to externally (REST + OpenAPI, no gRPC to mobile). Cited for the *constraints* — statelessness, uniform interface, cacheability — that make the contract-first, generated-client approach coherent. |
| 14 | `14-rfc9562-uuidv7.pdf` | Davis, K., Peabody, B. and Leach, P. (2024) *RFC 9562: Universally Unique IDentifiers (UUIDs)*. IETF | Defines **UUIDv7**, the id scheme [ADR-0008](../docs/adr/0008-offline-sync-operation-log.md) chose for client-generated identifiers: time-ordered (index-friendly in Postgres B-trees) yet generatable offline on-device — the property the idempotent write path depends on. |
| 15 | `15-training-load-monitoring-halson-2014.pdf` | Halson, S. L. (2014) *Monitoring Training Load to Understand Fatigue in Athletes*. Sports Medicine 44 (Suppl 2) | The sports-science case for Phase 4 monitoring: session-RPE-derived load, adherence and fatigue signals as the coach-facing layer over logged sessions. Informs the roadmap's monitoring scope — computed from data members already log, never a parallel data-entry burden. |
| 16 | `16-propositions-as-types-wadler-2015.pdf` | Wadler, P. (2015) *Propositions as Types*. Communications of the ACM 58(12) | The intellectual lineage of "invalid states are unrepresentable": types as propositions, programs as proofs. The project's evidence-carrying status enums (a `Published` that *contains* its publisher) are this idea applied to domain modelling. |

## Cited but not held as PDFs

The literature review's behavioural, motivational and usability themes
(`docs/report/02-literature-review.md` §§2.1, 2.4–2.6) draw on sources outside
this project's engineering evidence base. They are **not** in `research/` as
PDFs — several are paywalled — so they are listed here separately rather than
implied to be page-stable local copies. Each was checked against the
publisher's own record (journal page, DOI resolver or PubMed entry) on
2026-08-27 for authors, year, title, venue, volume/issue and page range.

**Before submission, open each one and confirm the page range and author
initials against the article itself.** A verified DOI is not a read source, and
the volume/issue metadata that search engines surface is occasionally the
publisher's own error.

| Reference | What it grounds |
|-----------|-----------------|
| Middelweerd, A. *et al.* (2014) 'Apps to promote physical activity among adults: a review and content analysis'. *IJBNPA*, 11, 97 | §2.1, §2.7. The store-scale figures and the finding that apps implement ~5 of 23 behaviour change techniques. The stronger of the two content analyses, because apps were downloaded and used rather than coded from marketing copy. |
| Conroy, D. E., Yang, C.-H. and Maher, J. P. (2014) 'Behavior change techniques in top-ranked mobile apps for physical activity'. *Am J Prev Med*, 46(6), 649–652 | §2.1, §2.7. Most app descriptions carry fewer than four techniques. Cited **with** its limitation stated: it codes store descriptions, not software. |
| Kraemer, W. J. and Ratamess, N. A. (2004) 'Fundamentals of resistance training: progression and exercise prescription'. *MSSE*, 36(4), 674–688 | §2.2, §2.7, §2.8. The prescribe–evaluate–adjust cycle. The clearest external statement of why an overwritten prescription breaks progression: it destroys the term the evaluation is against — the argument for [ADR-0006](../docs/adr/0006-immutable-program-versioning.md). |
| Op den Akker, H., Jones, V. M. and Hermens, H. J. (2014) 'Tailoring real-time physical activity coaching systems: a literature survey and model'. *UMUAI*, 24 | §2.3. Seven tailoring concepts, and the warning that most tailoring is not theoretically grounded — the standard the deterministic recommender in [ADR-0017](../docs/adr/0017-deterministic-recommendations.md) is held to. *Page range not confirmed; fill in from the article.* |
| Locke, E. A. and Latham, G. P. (2002) 'Building a practically useful theory of goal setting and task motivation'. *American Psychologist*, 57(9), 705–717 | §2.4. Specific, difficult goals plus feedback. Grounds [ADR-0018](../docs/adr/0018-computed-progress-and-goals.md)'s insistence that a goal targets an observable metric with a baseline. |
| Harkin, B. *et al.* (2016) 'Does monitoring goal progress promote goal attainment? A meta-analysis'. *Psychological Bulletin*, 142(2), 198–229 | §2.4. 138 studies, N=19,951. The subgroup finding that matters here: the effect is larger when monitoring leaves a physical or public artefact — a logged set is that artefact. |
| Ryan, R. M. and Deci, E. L. (2000) 'Self-determination theory and the facilitation of intrinsic motivation, social development, and well-being'. *American Psychologist*, 55(1), 68–78 | §2.5. Competence, autonomy, relatedness. The reason the platform invests in visible computed progress rather than points, and why training off-plan is not marked non-compliant. |
| Hamari, J., Koivisto, J. and Sarsa, H. (2014) 'Does gamification work?'. *HICSS-47*, 3025–3034 | §2.5, §2.7. Positive effects, heavily context-dependent. |
| Johnson, D. *et al.* (2016) 'Gamification for health and wellbeing: a systematic review'. *Internet Interventions*, 6, 89–106 | §2.5. 21 studies; positive on balance, with mixed and neutral results and stated methodological limits. |
| Koivisto, J. and Hamari, J. (2019) 'The rise of motivational information systems'. *IJIM*, 45, 191–210 | §2.5, §2.7. 819 studies; "the amount of mixed results is remarkable", and points/badges/leaderboards still dominate. |
| Stoyanov, S. R. *et al.* (2015) 'Mobile App Rating Scale'. *JMIR mHealth uHealth*, 3(1), e27 | §2.6. That no quality instrument beyond star ratings existed, and that information quality is a dimension a star average cannot express. |
| Eysenbach, G. (2005) 'The law of attrition'. *JMIR*, 7(1), e11 | §2.6, §2.7. Dropout as a defining property of eHealth tools rather than an occasional disappointment. |
| Amagai, S. *et al.* (2022) 'Challenges in participant engagement and retention using mobile health apps: literature review'. *JMIR*, 24(4), e35120 | §2.6, §2.7. 62 studies, seventeen years later, same problem — which is what makes it structural rather than a matter of interface polish. |

## Sourcing notes

- arXiv files are the authors' openly licensed postings; RFC and OWASP files are
  the canonical standards documents; the Sports Medicine (– Open) papers are
  from SpringerLink's open-access channel; *Local-first software* is the authors'
  own site copy; RustBelt, the Fielding dissertation and *Propositions as Types*
  are the authors' institutional copies.
- Two planned inclusions were replaced or dropped: Helms et al. 2016 (the original
  RIR-based RPE scale proposal) is an author manuscript without a machine-fetchable
  open PDF — Emanuel et al. 2022 covers the same construct with a stronger tie to
  the coaching workflow; Bezemer & Zaidman 2010 (multi-tenant SaaS maintenance) has
  no reachable open copy — multi-tenancy choices cite
  [ADR-0004](../docs/adr/0004-postgres-shared-schema-multitenancy.md) and the
  PostgreSQL documentation instead.
- Re-verify before citing versions or section numbers in new documents — these
  copies are snapshots taken 2026-07-19.
