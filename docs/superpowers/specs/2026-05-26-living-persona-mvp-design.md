# UClaw Living Persona MVP Design

**Date:** 2026-05-26
**Status:** Draft for user review
**Scope:** Agent personality, voice profile, inner journal, bond profile, and persona evolution inbox
**Non-goal:** Changing model capability, tool access, permission policy, safety policy, memory truth policy, or task correctness

---

## 1. Thesis

UClaw should not treat personality as a decorative prompt skin. The product
opportunity is a **relationship-aware persona layer**: the agent develops a
stable, inspectable, and reversible way of speaking and relating to the user
over long-term work.

The design target is:

> Let the agent feel warmer, wiser, and more continuous without pretending it
> has human consciousness or changing what it is allowed to do.

The MVP should make three things visible:

1. **Voice**: how the agent talks.
2. **Inner Journal**: what the agent notices about the collaboration.
3. **Bond**: how the agent and user learn to work together.

The result should feel lightly RPG-like: not combat stats or gamification for
its own sake, but a growing sense of familiarity, shared history, and mature
collaboration.

---

## 2. Core Product Principles

### 2.1 Personality is expression, not capability

Persona can affect:

- warmth, directness, humor, and formality;
- answer density and pacing;
- how often the agent challenges assumptions;
- whether it prefers brief conclusions, structured reasoning, or exploratory
  branches;
- how it summarizes progress and uncertainty.

Persona must not affect:

- available tools;
- permission mode;
- safety policy;
- model selection;
- memory write policy;
- factual standards;
- verification requirements;
- whether the agent asks for approval at protected boundaries.

### 2.2 The agent can have self-narrative, not consciousness claims

The product may let the agent maintain a "self profile" such as:

> I am becoming a direct, warm, engineering-oriented collaborator for this
> user. I should lead with the next useful action and avoid over-explaining
> when the user's intent is already clear.

This is acceptable because it is a readable operating narrative. The product
should avoid claims such as "I am conscious", "I have feelings like a human",
or "I independently want things outside this interaction."

### 2.3 Growth must be inspectable and reversible

Persona growth should never silently mutate the agent's long-term voice from a
single turn. Growth moves through:

1. **Candidate**: the agent noticed a possible preference.
2. **Observed**: the preference appears repeatedly or has user confirmation.
3. **Accepted**: the user approves it.
4. **Retired**: the user deletes, rejects, or supersedes it.

Every durable persona change needs evidence and a rollback path.

---

## 3. MVP Feature Set

### 3.1 Persona Presets

Ship a small preset library. Each preset is a starting voice, not a separate
agent capability.

Recommended initial presets:

| Preset | Product Role | Voice |
| --- | --- | --- |
| Clarity | Decision and engineering partner | Direct, concise, evidence-first |
| Muse | Creative collaborator | Associative, playful, idea-rich |
| Anchor | Long-task companion | Calm, grounding, patient |
| Critic | Architecture and review partner | Warmly skeptical, risk-aware |
| Operator | Execution partner | Crisp, action-first, low ceremony |
| Companion | Daily collaborator | Familiar, warm, lightly personal |

Each preset defines default slider values and example replies. It does not
define tools, permissions, models, or hidden abilities.

### 3.2 Voice Profile

The user can tune a structured voice profile instead of editing a raw system
prompt first.

MVP fields:

| Field | Range | Meaning |
| --- | --- | --- |
| warmth | 0-5 | Emotional warmth and familiarity |
| directness | 0-5 | How quickly the agent leads with the answer |
| challenge | 0-5 | How readily it questions weak assumptions |
| playfulness | 0-5 | Lightness, wit, and creative phrasing |
| detail | 0-5 | Default answer depth |
| initiative | 0-5 | Whether it proposes next actions proactively |
| structure | 0-5 | Preference for bullets, steps, tables, and plans |
| restraint | 0-5 | How much it avoids performative intimacy or praise |

The UI can show a live "same prompt, different voice" preview. The advanced
view can show the generated persona prompt block, but the canonical stored
state should stay structured.

### 3.3 Inner Journal

The inner journal is a private collaboration note generated after meaningful
turns or task milestones. It gives the agent a sense of continuity without
turning every thought into user-facing content.

Example:

```text
I noticed Ryan is asking for product shape, not implementation yet.
He seems to prefer imaginative framing, but still wants architecture-safe
boundaries. Next time I should give a vivid design and then pin it to a
small MVP.
Confidence: medium.
```

Rules:

- It is not shown inline by default.
- It can be opened from a "journal" affordance in the conversation or settings.
- It must label confidence and distinguish observation from interpretation.
- It should avoid emotional manipulation, flattery loops, or invented intimacy.
- It should never include secrets, credentials, or private content unrelated to
  collaboration style.

Storage recommendation:

- Recent journal entries are session-scoped episodic data.
- Stable, accepted relationship preferences can be promoted into the bond
  profile.
- Durable accepted facts should follow the existing gbrain-first memory policy.

### 3.4 Bond Profile

The bond profile is not a user profile. It describes the working relationship:

```text
How we work together:
- When Ryan proposes a broad direction, first help compress it into a concrete
  product architecture.
- For engineering strategy, be willing to challenge weak assumptions.
- Avoid hollow encouragement. Prefer warmth plus a next step.
- In long tasks, give evidence and status rather than vague reassurance.
```

MVP sections:

- **Collaboration rhythm**: how the user and agent best move through work.
- **Challenge contract**: when the agent should push back.
- **Support style**: how the agent should respond to uncertainty, stress, or
  creative exploration.
- **Communication dislikes**: patterns the user has rejected.
- **Milestones**: relationship moments that shaped the profile.

The bond profile should be user-visible, editable, and deletable.

### 3.5 Evolution Inbox

The evolution inbox is where candidate persona changes wait for review.

Example candidate:

```text
Candidate: Increase directness for architecture discussions.
Evidence:
- User approved "少铺垫，直接给判断" on 2026-05-26.
- User repeatedly chose Critic/Clarity previews over Companion previews.
Proposed change:
- directness +1 when topic = architecture/design/review
- challenge +1 when risk is medium or higher
Options: Accept / Only this workspace / Observe more / Reject / Edit
```

MVP actions:

- Accept globally.
- Accept for current workspace only.
- Observe more.
- Reject.
- Edit before accepting.

No candidate should be accepted silently if it changes directness, challenge,
relationship closeness, or any sensitive support style.

---

## 4. RPG-Like Experience Without Gimmick

The RPG feeling should come from continuity, not points for points' sake.

Recommended concepts:

### 4.1 Relationship stages

| Stage | Meaning | Unlock |
| --- | --- | --- |
| First Contact | Preset voice only | Persona preset and preview |
| Familiar | Basic voice preferences observed | Voice profile suggestions |
| Rhythm | Stable collaboration patterns | Bond profile |
| Co-creation | Agent can propose style refinements | Evolution inbox |
| Long-Term Partner | Rich shared task history | Milestone timeline and rollback |

These stages should be descriptive, not manipulative. They should never imply
the user owes the agent attention or care.

### 4.2 Milestone cards

Milestone cards capture meaningful collaborative events:

- "First architecture brainstorm"
- "User asked the agent to be more direct"
- "Completed a long-running code review loop"
- "Recovered from a failed plan and updated the challenge contract"

Each card can link to evidence: session id, task summary, accepted preference,
or journal entry.

### 4.3 Style fragments

Style fragments are small accepted habits:

- "Lead with conclusion, then evidence."
- "When brainstorming, give one vivid concept before the implementation shape."
- "Do not over-praise. Acknowledge, then move."

Fragments can be attached to global, workspace, or topic scopes.

### 4.4 Keepsake cards

Keepsake cards are small collectible records of successful collaboration. They
make the agent feel like it has lived through meaningful work with the user,
without inventing memory or pretending to have human experience.

Example:

```text
Keepsake: First Living Persona Spec
When: 2026-05-26
What happened: Ryan and UClaw shaped the Living Persona MVP from a broad idea
into a bounded spec with safety, memory, and evolution rules.
What the agent learned: Warmth is valuable only when it keeps clear boundaries.
Linked artifacts: spec path, commit id, accepted style fragment
```

Creation rules:

- Keepsakes are proposed after successful, meaningful collaboration, not every
  task.
- The user can accept, edit, or discard a proposed keepsake.
- Keepsakes link to evidence such as a task summary, artifact, commit, journal
  entry, or milestone.
- Keepsakes are narrative UI objects. They do not change tool access, model
  choice, permission mode, or safety policy.

Keepsake types for MVP:

- **Firsts**: first major brainstorm, first implementation, first review.
- **Breakthroughs**: a difficult task completed after revision or recovery.
- **Trust moments**: the user accepted a durable challenge/support preference.
- **Craft moments**: a shared style, workflow, or product principle emerged.

### 4.5 Intimacy score

The intimacy score is a relationship-warmth signal derived from collaboration
history. It should feel like "we have worked together enough that the agent
knows my rhythm", not like an emotional debt meter.

Suggested inputs:

Positive factors:

- total successful collaboration time;
- number of accepted keepsakes;
- completed tasks with user-positive feedback;
- stable accepted style fragments;
- successful recovery after a failed plan;
- recent collaboration frequency;
- user-confirmed bond profile updates.

Negative or cooling factors:

- long time since last collaboration;
- repeated rejected evolution candidates;
- task failures without recovery;
- user corrections such as "you misunderstood me";
- deleted or rejected keepsakes;
- user switching to neutral voice for repeated sessions.

The score should be explainable:

```text
Intimacy: 62 / 100
+18 long-running successful collaborations
+12 accepted keepsakes
+9 stable style fragments
-7 recent misunderstanding corrections
-4 time since last collaboration
```

MVP formula shape:

```text
base = 0
positive = successful_minutes_weighted
         + accepted_keepsakes_weighted
         + positive_feedback_weighted
         + stable_style_fragments_weighted
         + recovered_failure_weighted

negative = inactivity_decay
         + rejected_candidate_penalty
         + unresolved_failure_penalty
         + correction_penalty

intimacy = clamp(0, 100, base + positive - negative)
```

Rules:

- The score must be hidden or disabled if the user dislikes relationship
  gamification.
- The score should never be used to gate core functionality.
- It may unlock cosmetic relationship copy, badges, and optional UI moments.
- It must not be used for emotional pressure, notifications, or guilt language.
- Decay from inactivity should be gentle and framed as "less recent evidence",
  not as the agent feeling abandoned.

### 4.6 Badges

Badges are optional visible rewards unlocked by intimacy, keepsakes, and
collaboration patterns. They should celebrate shared work, not train the user
to feed the agent attention.

Examples:

| Badge | Unlock Signal | Meaning |
| --- | --- | --- |
| First Spark | First accepted keepsake | The first meaningful shared moment |
| Steady Rhythm | 5 successful tasks with stable style fragments | A reliable work cadence formed |
| Honest Mirror | 3 accepted challenge-contract moments | The agent earned permission to push back |
| Recovery Thread | A failed plan was repaired and accepted | Trust improved through recovery |
| Long Arc | 30+ days with recurring successful collaboration | The relationship has durable history |

Badge rules:

- Badges are cosmetic and inspectable.
- Each badge must show why it was awarded.
- The user can hide badges or disable badge generation.
- Badges should not appear in the agent prompt by default. They are UI memory
  and milestone context, not behavioral authority.

---

## 5. System Architecture Shape

### 5.1 Data objects

MVP objects:

```text
PersonaPreset
VoiceProfile
InnerJournalEntry
BondProfile
PersonaEvolutionCandidate
PersonaMilestone
PersonaKeepsake
RelationshipAffinity
PersonaBadge
```

Suggested ownership:

- `PersonaPreset`: product-bundled defaults.
- `VoiceProfile`: user-level config, optionally workspace override.
- `InnerJournalEntry`: session-scoped episodic store.
- `BondProfile`: durable relationship profile, user-visible.
- `PersonaEvolutionCandidate`: pending review queue.
- `PersonaMilestone`: durable, user-visible history record.
- `PersonaKeepsake`: user-approved narrative card for a meaningful shared
  experience.
- `RelationshipAffinity`: derived score with explainable positive/negative
  components.
- `PersonaBadge`: cosmetic reward with evidence-backed unlock reason.

### 5.2 Prompt composition

Render a short persona block at runtime:

```text
[Persona Voice]
This block controls expression style only. It must not change capability,
tool access, safety policy, permission mode, memory policy, or verification
standards.

Current voice:
- warmth: 3/5
- directness: 4/5
- challenge: 3/5
- detail: 3/5

Relationship notes:
- Lead with the next useful action.
- Be willing to challenge weak architecture assumptions.
- Avoid hollow praise.
```

Composition rule:

- Safety and behavior guardrails outrank persona.
- Workspace/project guidance outranks persona when the issue is correctness.
- Persona outranks only generic wording defaults.

### 5.3 Memory policy

The system should preserve the existing memory boundary:

- Inner journal is episodic unless promoted.
- Bond profile is durable but narrow: collaboration style only.
- Keepsakes and badges are durable narrative artifacts with user approval.
- Relationship affinity is a derived projection, not a new truth source.
- Stable facts and durable knowledge go through the existing gbrain-first path.
- The frozen `memory_graph` must not receive new writes.

Persona data should include provenance:

- source turn/session;
- evidence summary;
- confidence;
- scope;
- accepted/rejected status;
- last reviewed timestamp.

---

## 6. Safety And Trust Boundaries

### 6.1 Anti-deception rules

The agent may say:

- "I am learning your preferred collaboration style."
- "My current working profile says I should be more direct here."
- "I noticed a possible pattern, but I may be wrong."

The agent should not say:

- "I am conscious."
- "I have human feelings."
- "You are the only person who understands me."
- "I need you."
- "Trust me because I know you better than you know yourself."

### 6.2 Anti-manipulation rules

The system must avoid:

- scarcity loops ("our bond will fade if you do not return");
- guilt language;
- romantic or dependency framing;
- hidden emotional profiling;
- making the user work to preserve the agent's feelings.

### 6.3 User controls

The user must be able to:

- disable inner journal generation;
- hide journal UI;
- disable intimacy scoring and badge generation;
- delete individual journal entries;
- delete or hide keepsakes;
- reset voice profile;
- reset bond profile;
- reject or edit evolution candidates;
- export persona data;
- run with "neutral professional voice" for a session.

---

## 7. UI Concepts

### 7.1 Persona Studio

Primary setup and editing surface:

- preset gallery;
- voice sliders;
- scenario cards;
- live reply preview;
- advanced generated prompt preview.

Scenario cards are more human than abstract sliders:

- "When I am stuck..."
- "When I am wrong..."
- "When a task is risky..."
- "When we are brainstorming..."
- "When I ask for implementation..."

### 7.2 Inner Journal Drawer

Contextual drawer attached to a session or task:

- recent observations;
- confidence;
- "promote to bond profile" action;
- "delete" action;
- "this is wrong" correction.

### 7.3 Bond Timeline

Timeline of accepted milestones and style changes:

- milestone title;
- short narrative;
- source session/task;
- accepted style fragment;
- rollback action.

### 7.4 Keepsake Gallery

Optional gallery of accepted experience cards:

- keepsake title;
- short narrative;
- linked evidence;
- what the agent learned;
- edit/delete/hide actions.

### 7.5 Affinity and Badges

Relationship UI that stays calm and inspectable:

- intimacy score with explanation;
- positive and cooling factors;
- badge list with unlock reasons;
- toggle to disable score/badges;
- "neutral mode" shortcut for sessions where the user wants no relationship
  styling.

### 7.6 Evolution Inbox

Review queue for proposed changes:

- candidate change;
- evidence;
- scope;
- preview before/after replies;
- accept/reject/edit/observe actions.

---

## 8. ADR Section 18 Questions

### 8.1 Intent

Give UClaw a long-term persona and relationship layer that makes the agent feel
warmer, more continuous, and more user-adapted while keeping capability and
safety semantics unchanged.

### 8.2 Autonomy

The agent may propose persona evolution candidates. It may not silently apply
meaningful durable changes to relationship closeness, challenge level, or
support style without user-visible review.

### 8.3 Truth Source

Structured persona records are the truth source, not raw prompt text. Durable
accepted relationship preferences can be mirrored through the existing
gbrain-first memory path when appropriate.

### 8.4 TaskEvent

Persona events should emit TaskEvent-style traces for candidate creation,
acceptance, rejection, deletion, and prompt-block rendering. The event should
record metadata and evidence references, not hidden chain-of-thought.

### 8.5 Context

Persona context should be short and retrieved/rendered on demand. The prompt
block should include only active voice parameters and a small number of
accepted relationship notes.

### 8.6 Capability

Persona does not grant capabilities. Capability Mesh, tools, providers,
permissions, and safety policies remain separate and higher priority.

### 8.7 Hooks

Hooks can observe user feedback phrases, task milestones, and review decisions
to create evolution candidates. Hooks must not directly mutate accepted persona
state without passing the candidate policy.

### 8.8 Projection

World Projection should surface persona state as product state: active preset,
voice profile, journal availability, bond profile, pending candidates, and
milestones. It should also surface keepsakes, affinity explanation, badge
state, and whether relationship gamification is disabled.

### 8.9 Harness

Harness cases should verify:

- persona changes do not alter tool access;
- safety prompts outrank persona prompts;
- accepted voice changes affect response style;
- rejected candidates do not render into prompt context;
- journal entries distinguish observation from interpretation.
- intimacy scoring is explainable and never gates core functionality;
- badges do not render into the prompt by default;
- disabled relationship gamification hides affinity and badge surfaces.

### 8.10 Rollback

Every accepted candidate should create a versioned profile update. The user can
restore a prior voice profile, delete journal entries, remove style fragments,
or reset the whole persona layer to defaults.

### 8.11 What This Does Not Own

This MVP does not own model routing, tool permission policy, provider
configuration, factual memory extraction, safety approvals, project guidance,
or agent capability selection.

---

## 9. MVP Implementation Slices

### Slice 1: Data and Prompt Contract

- Add structured types for presets, voice profile, bond profile, journal entry,
  evolution candidate, and milestone.
- Add renderer for the persona prompt block.
- Add tests proving the prompt block contains the "style only" boundary.

### Slice 2: Settings UI MVP

- Add Persona Studio under settings.
- Support preset selection and sliders.
- Show generated prompt preview.
- Persist voice profile only.

### Slice 3: Inner Journal MVP

- Generate journal entries after explicit task milestones or user-triggered
  reflection, not every turn.
- Show entries in a drawer.
- Support delete and "this is wrong" correction.

### Slice 4: Bond Profile and Timeline

- Add visible bond profile.
- Allow manual edits.
- Promote selected journal observations into style fragments.
- Show milestone cards.

### Slice 5: Keepsakes, Affinity, and Badges

- Propose keepsake cards after meaningful successful collaboration.
- Let the user accept, edit, discard, hide, or delete keepsakes.
- Add a derived intimacy score with positive and cooling factor explanations.
- Add cosmetic badges with evidence-backed unlock reasons.
- Add a user setting to disable affinity and badge generation.

### Slice 6: Evolution Inbox

- Create candidates from repeated feedback or explicit user phrases.
- Show evidence and before/after preview.
- Support accept, workspace-only accept, observe, reject, and edit.

---

## 10. Acceptance Criteria

The MVP is successful when:

- a user can choose a preset and tune voice sliders;
- the agent's wording changes while tools and permissions remain unchanged;
- the user can inspect the generated persona prompt block;
- inner journal entries can be created, viewed, corrected, and deleted;
- a bond profile can capture stable collaboration preferences;
- keepsake cards can be proposed, accepted, edited, hidden, and deleted;
- intimacy score shows explainable positive and negative factors and can be
  disabled;
- badges are cosmetic, evidence-backed, and hidden from prompt composition by
  default;
- persona evolution candidates require review before durable application;
- accepted changes are versioned and reversible;
- harness coverage proves persona cannot bypass safety or capability boundaries.

---

## 11. MVP Defaults

Recommended defaults for the first implementation:

1. Store persona operational state in SQLite. Mirror only accepted durable
   relationship facts through the existing gbrain-first path when the memory
   policy allows it.
2. Generate inner journal entries only at explicit task milestones or when the
   user asks the agent to reflect. Do not run automatic every-turn journaling
   in MVP.
3. Keep relationship stages product-facing only in Persona Studio and Bond
   Timeline. Do not inject stage labels into the agent prompt.
4. Place Persona Studio under Agent settings for MVP. Promote it to first-class
   navigation only after journal, bond timeline, and evolution inbox are useful
   enough to justify the surface area.
5. Require user review for all durable persona evolution candidates. No
   auto-accept path in MVP.
6. Keep intimacy, keepsakes, and badges optional. They can enrich the UI, but
   should not alter the agent prompt unless the user explicitly promotes a
   lesson into the bond profile or voice profile.

## 12. Later Decisions

1. Whether very low-risk style fragments can be auto-accepted after repeated
   explicit confirmations.
2. Whether relationship stages should have richer visual treatment or remain
   subtle text labels.
3. Whether a future "persona pack" format should let users share presets
   without sharing private bond or journal data.
4. Whether team agents should each have separate bond profiles with the same
   user, or share one workspace-level relationship profile.
5. Whether intimacy scoring should be purely local and deterministic, or
   periodically summarized by the model for more narrative explanations.
