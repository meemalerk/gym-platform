# 2. Research (Literature Review)

This chapter reviews the literature against the five research questions in §1.4;
each section names the question it serves. The review is critical rather than
descriptive: several design decisions were made by reading a source and declining to
follow it, and the disagreement is defended where that happened.

## 2.1 Mobile fitness applications *(RQ1)*

The category is large and long-established: Middelweerd *et al.* (2014) recorded
23,490 Health and Fitness applications on iTunes and 17,756 on Google Play in 2013,
resting on the premise that a continuously carried device can monitor and coach
during ordinary activity (Op den Akker, Jones and Hermens, 2014).

Volume is not quality. Middelweerd *et al.* (2014) coded 64 applications against a
23-item taxonomy of behaviour change techniques and found an average of five
implemented; Conroy, Yang and Maher (2014) coded 167 top-ranked applications and
found most described fewer than four. Two caveats bound this: both are a decade old,
and Conroy, Yang and Maher coded *marketing copy* rather than software, making
Middelweerd *et al.* the stronger claim. The direction survives both — a small
fraction of the techniques known to change behaviour is implemented, and structured
prescription with feedback against it is among the sparsest.

## 2.2 Gym and workout tracking *(RQ2)*

What a tracker should record is settled by what training science needs to make the
next decision. Kraemer and Ratamess (2004) describe progression as an explicit cycle
of prescription, evaluation and adjustment, parameterised by load, volume,
repetition range and rest. Each is a field, and the cycle closes only if the
prescription still exists when the evaluation happens.

Two findings sharpen what to capture beyond load and repetitions. Emanuel *et al.*
(2022) found coaches systematically misjudge proximity to failure by observation, so
effort must be recorded *from the athlete, at the time* rather than inferred later.
Grgic *et al.* (2020) found one-repetition-maximum testing reliable but costly and
risky under fatigue, making frequent testing impractical; the platform therefore
reports an *estimated* 1RM from sets logged anyway — an accuracy-for-frequency trade,
stated as an estimate rather than a measurement.

## 2.3 Personalised workout planning *(RQ5)*

Op den Akker, Jones and Hermens (2014) survey activity-coaching systems and conclude
the field needs tailoring grounded in theory rather than improvised heuristics —
endorsing personalisation while warning that most implementations are not
theoretically motivated.

The modern instrument is a learned recommender, and Zhang *et al.* (2019) survey
deep-learning approaches persuasively on accuracy over large datasets. This project
declines them for three reasons. **Data:** they need interaction volume a single gym
lacks, and cold start is the normal state here. **Accountability:** "the model ranked
it highly" is not an answer to a member asking why. **Health adjacency:** a
suggestion shaping physical training sits closer to advice than to a film
recommendation. The platform personalises deterministically, every suggestion
carrying a readable reason — the accepted cost of an explanation true by
construction.

## 2.4 Fitness goal setting and progress monitoring *(RQ2, RQ4)*

Locke and Latham (2002), summarising thirty-five years of work, report that specific,
difficult goals outperform vague ones, moderated by feedback and commitment. Feedback
is not optional there: a difficult goal without knowledge of progress does not
produce the effect.

Harkin *et al.* (2016) supply the meta-analytic case across 138 studies
(N = 19,951), and their most useful finding for design is that the effect was larger
when progress was *recorded physically or reported publicly* — monitoring that leaves
an artefact works better, so a tracker does more than bookkeeping. Two commitments
follow: goals must target observable metrics with a baseline captured at creation,
and progress must be computed from the same immutable history the coach reads. Halson
(2014) adds that monitoring must draw on data members already log, because the burden
of a second diary is what makes such schemes fail.

## 2.5 User motivation and gamification *(RQ4)*

Ryan and Deci (2000) locate durable motivation in competence, autonomy and
relatedness, noting that contingent external rewards can undermine intrinsic
motivation — which matters because most gamification is extrinsic by construction.

The evidence is genuinely mixed. Hamari, Koivisto and Sarsa (2014) found positive
effects heavily dependent on context and user; Johnson *et al.* (2016), reviewing 21
health studies, found the balance positive but with mixed results and methodological
limits; Koivisto and Hamari (2019), across 819 studies, reach the same shape at far
greater scale, with points, badges and leaderboards still dominant. Gamification is
therefore a context-dependent amplifier, not a substitute for a system worth using.
Read alongside Ryan and Deci (2000), the defensible investment is visible
**competence** — evidence of getting stronger, traceable to one's own logged sets.
That is a judgement extrapolated from mixed evidence; the system has not been
evaluated for engagement effects.

## 2.6 User experience and usability *(RQ4)*

Stoyanov *et al.* (2015) developed the Mobile App Rating Scale because no app-quality
instrument beyond star ratings existed — itself the finding worth carrying, since a
five-star average cannot express information quality.

Usability failures here surface as attrition rather than complaints. Eysenbach (2005)
argued dropout is a defining property of eHealth tools; Amagai *et al.* (2022),
reviewing 62 studies seventeen years later, found retention still an omnipresent
challenge — surviving two decades of iteration suggests a structural problem rather
than a matter of interface polish. One structural cause is addressable: Kleppmann
*et al.* (2019) argue for the device as primary copy, the network an enhancement. For
a member logging sets on a gym floor with no signal, an application that will not
record without connectivity is not degraded but broken at the one moment it is
used.

## 2.7 Limitations of existing fitness applications *(RQ1)*

Five limitations recur. **Thin behavioural content** (Middelweerd *et al.*, 2014;
Conroy, Yang and Maher, 2014). **No versioning of prescriptions** — neither content
analysis found it, so a programme edited mid-cycle silently rewrites what the member
was told, breaking the cycle Kraemer and Ratamess (2004) describe. **Opaque or
ungrounded personalisation** (Op den Akker, Jones and Hermens, 2014; Zhang *et al.*,
2019). **Motivation mechanics substituting for substance** (Koivisto and Hamari,
2019; Ryan and Deci, 2000). **Unresolved attrition** (Eysenbach, 2005;
Amagai *et al.*, 2022). A sixth is invisible in this literature because it studies
consumer applications: none addresses a *gym* — an organisation with staff, roles and
members whose data different people may legitimately see to different extents.

## 2.8 The research gap *(RQ3)*

The behavioural evidence says structured prescription, feedback and progress
monitoring are what work (Locke and Latham, 2002; Harkin *et al.*, 2016); the content
analyses say these are what existing applications implement least. The
training-science literature says the cycle requires the prescription to survive
intact (Kraemer and Ratamess, 2004), and dictates what must come from the athlete and
how far derived figures may be trusted (Emanuel *et al.*, 2022; Grgic *et al.*, 2020).

**The gap this project addresses** is the absence of a gym-scale platform in which a
training prescription is a versioned, immutable artefact — reviewed before
publication, pinned to the member assigned it, and preserved unchanged when the coach
writes the next version — so that logged history remains interpretable against what
the member was actually told to do.

That property makes the rest tractable: progress can be computed rather than stored
because history is immutable; a recommendation can carry a true reason because the
prescription it points at still exists; and a member can train offline because
appending to an immutable log needs no coordination. Chapter 3 turns this into
requirements.

## References

Amagai, S., Pila, S., Kaat, A. J., Nowinski, C. J. and Gershon, R. C. (2022)
'Challenges in participant engagement and retention using mobile health apps:
literature review'. *Journal of Medical Internet Research*, 24 (4), e35120.
Available from: https://www.jmir.org/2022/4/e35120/ [Accessed 27 August 2026].

Conroy, D. E., Yang, C.-H. and Maher, J. P. (2014) 'Behavior change techniques in
top-ranked mobile apps for physical activity'. *American Journal of Preventive
Medicine*, 46 (6), pp. 649–652. Available from:
https://doi.org/10.1016/j.amepre.2014.01.010 [Accessed 27 August 2026].

Emanuel, A., Har-Nir, I., Obolski, U. and Halperin, I. (2022) 'Seeing effort:
assessing coaches' prediction of the number of repetitions in reserve before
task-failure'. *Sports Medicine – Open*, 8 (1). Available from:
https://doi.org/10.1186/s40798-022-00516-w [Accessed 19 July 2026].

Eysenbach, G. (2005) 'The law of attrition'. *Journal of Medical Internet
Research*, 7 (1), e11. Available from: https://www.jmir.org/2005/1/e11/
[Accessed 27 August 2026].

Grgic, J., Lazinica, B., Schoenfeld, B. J. and Pedisic, Z. (2020) 'Test–retest
reliability of the one-repetition maximum (1RM) strength assessment: a systematic
review'. *Sports Medicine – Open*, 6 (1). Available from:
https://doi.org/10.1186/s40798-020-00260-z [Accessed 19 July 2026].

Halson, S. L. (2014) 'Monitoring training load to understand fatigue in athletes'.
*Sports Medicine*, 44 (Suppl. 2), pp. 139–147. Available from:
https://doi.org/10.1007/s40279-014-0253-z [Accessed 19 July 2026].

Hamari, J., Koivisto, J. and Sarsa, H. (2014) 'Does gamification work? — a
literature review of empirical studies on gamification'. *Proceedings of the 47th
Hawaii International Conference on System Sciences*, pp. 3025–3034. Available
from: https://doi.org/10.1109/HICSS.2014.377 [Accessed 27 August 2026].

Harkin, B., Webb, T. L., Chang, B. P. I., Prestwich, A., Conner, M., Kellar, I.,
Benn, Y. and Sheeran, P. (2016) 'Does monitoring goal progress promote goal
attainment? A meta-analysis of the experimental evidence'. *Psychological
Bulletin*, 142 (2), pp. 198–229. Available from:
https://doi.org/10.1037/bul0000025 [Accessed 27 August 2026].

Johnson, D., Deterding, S., Kuhn, K.-A., Staneva, A., Stoyanov, S. and Hides, L.
(2016) 'Gamification for health and wellbeing: a systematic review of the
literature'. *Internet Interventions*, 6, pp. 89–106. Available from:
https://www.sciencedirect.com/science/article/pii/S2214782916300380
[Accessed 27 August 2026].

Kleppmann, M., Wiggins, A., van Hardenberg, P. and McGranaghan, M. (2019)
'Local-first software: you own your data, in spite of the cloud'. *Proceedings of
the 2019 ACM SIGPLAN International Symposium on New Ideas, New Paradigms, and
Reflections on Programming and Software (Onward!)*. Available from:
https://www.inkandswitch.com/local-first/ [Accessed 19 July 2026].

Koivisto, J. and Hamari, J. (2019) 'The rise of motivational information systems:
a review of gamification research'. *International Journal of Information
Management*, 45, pp. 191–210. Available from:
https://doi.org/10.1016/j.ijinfomgt.2018.10.013 [Accessed 27 August 2026].

Kraemer, W. J. and Ratamess, N. A. (2004) 'Fundamentals of resistance training:
progression and exercise prescription'. *Medicine & Science in Sports & Exercise*,
36 (4), pp. 674–688. Available from: https://pubmed.ncbi.nlm.nih.gov/15064596/
[Accessed 27 August 2026].

Locke, E. A. and Latham, G. P. (2002) 'Building a practically useful theory of
goal setting and task motivation: a 35-year odyssey'. *American Psychologist*,
57 (9), pp. 705–717. Available from: https://doi.org/10.1037/0003-066X.57.9.705
[Accessed 27 August 2026].

Middelweerd, A., Mollee, J. S., van der Wal, C. N., Brug, J. and te Velde, S. J.
(2014) 'Apps to promote physical activity among adults: a review and content
analysis'. *International Journal of Behavioral Nutrition and Physical Activity*,
11, 97. Available from: https://doi.org/10.1186/s12966-014-0097-9
[Accessed 27 August 2026].

Op den Akker, H., Jones, V. M. and Hermens, H. J. (2014) 'Tailoring real-time
physical activity coaching systems: a literature survey and model'. *User Modeling
and User-Adapted Interaction*, 24. Available from:
https://doi.org/10.1007/s11257-014-9146-y [Accessed 27 August 2026].

Ryan, R. M. and Deci, E. L. (2000) 'Self-determination theory and the facilitation
of intrinsic motivation, social development, and well-being'. *American
Psychologist*, 55 (1), pp. 68–78. Available from:
https://doi.org/10.1037/0003-066X.55.1.68 [Accessed 27 August 2026].

Stoyanov, S. R., Hides, L., Kavanagh, D. J., Zelenko, O., Tjondronegoro, D. and
Mani, M. (2015) 'Mobile App Rating Scale: a new tool for assessing the quality of
health mobile apps'. *JMIR mHealth and uHealth*, 3 (1), e27. Available from:
https://mhealth.jmir.org/2015/1/e27/ [Accessed 27 August 2026].

Zhang, S., Yao, L., Sun, A. and Tay, Y. (2019) 'Deep learning based recommender
system: a survey and new perspectives'. *ACM Computing Surveys*, 52 (1),
pp. 1–38. Available from: https://arxiv.org/abs/1707.07435
[Accessed 19 July 2026].
