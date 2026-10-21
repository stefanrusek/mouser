<!--
Sync Impact Report
Version change: 1.3.0 -> 1.4.0 (2026-10-21): Development Workflow defines the workspace bootstrap
  command (Principle 5.2), operator decision D5 in specs/001-mouser-v1/plan.md.
Previous: 1.2.0 -> 1.3.0 (2026-10-21): Principle III gains the release-download exception
  and one-click join approval (operator decisions in specs/001-mouser-v1 clarifications).
Previous: 1.1.0 -> 1.2.0 (2026-10-21): Development Workflow gains the rule that every
  tasks.md Polish phase updates docs/ to prevent drift.
Previous: 1.0.0 -> 1.1.0
Modified principles: none (project principles I-VII preserved unchanged)
Added sections: Spec-Driven Development Methodology Amendment v6.2.0 (Purpose, 1 Process
  Discipline, 2 Specification, 3 Planning, 4 Testing, 5 Implementation, Applicability Map,
  Amendment Governance), integrated before Governance with headings demoted one level;
  Governance gains amendment stamp, precedence and stamp-retention notes
Removed sections: none
Deferred items: .specify/memory/personas.md (Principle 2.2) created separately on
  2026-10-21; workspace bootstrap command (Principle 5.2) to be defined before the
  first package-parallel plan
-->
# Mouser Constitution

## Core Principles

### I. Reliability First (NON-NEGOTIABLE)
A connection that drops while the user is typing is the failure Mouser exists to eliminate.
- Connections MUST heal themselves after sleep, Wi-Fi/Ethernet switches, IP changes and VPN
  changes, without restarting the app or any user action.
- Extra network adapters (VPN, VM, container bridges) MUST NOT break discovery or connectivity,
  with no configuration.
- Input latency and continuity take priority over every other feature; any change that can
  interrupt an active keyboard or pointer session MUST be covered by a test or a documented
  recovery path.
- Failures MUST be recoverable by the system itself first and surfaced to the user second.

Rationale: ShareMouse's dropped connections and VPN/VM breakage are the reason for this project.

### II. Discoverable, Plain-Language UI
- Settings MUST live next to what they affect (edge and corner behaviour on the edge in the
  display arranger; display options on the display).
- The core experience MUST work with zero configuration; defaults MUST be good enough that most
  users never open settings.
- Main UI text MUST be plain language. Technical terms belong behind an "Advanced" disclosure or
  in the config file.
- When something does not work, the app MUST say why and offer a one-click fix where one exists
  (missing permission, Secure Input held by a named app, incompatible peer version, blocked
  switching).

### III. Trusted LAN, Low Ceremony
- Grouping computers into named desks is a usability feature, not a security ritual. Passwords
  and pairing codes MUST NOT be required to form a group.
- All traffic MUST still be encrypted, with keys exchanged automatically and no user-facing
  setup.
- Mouser is LAN only. Nothing MAY be sent to the internet; no cloud services, accounts, telemetry
  or licensing. Sole exception: one desk computer MAY check the published release location and
  download new Mouser releases, which it then shares over the LAN; that exchange MUST carry no
  input, clipboard, file or desk information.
- Joining a desk MUST be approved with one click on a computer already on that desk.
- Separate desks on one network MUST NOT see each other's input or clipboard.

### IV. Agent-Friendly Operations
- Configuration MUST be a plain-text file; every setting reachable in the UI MUST be expressible
  in it.
- A CLI MUST expose state and control for scripting and diagnosis.
- Logs MUST be JSON Lines written to a file, never to the OS console. Diagnostic logging is one
  switch (UI checkbox, CLI and config equivalent), with no launch-time key combos.

### V. Cross-Platform Parity
macOS, Windows and Linux (X11, GNOME Wayland, KDE Wayland) are all first-class. A feature is not
done until it works, or has a documented and user-visible limitation, on every supported
platform. Platform-specific code MUST be isolated in per-OS input crates behind a common
interface so the core stays platform-neutral.

### VI. Test the Seams
Behaviour that cannot be verified by unit tests alone (reconnect, network changes, input
injection, clipboard, file transfer) MUST have integration or simulated-network tests. Reliability
fixes MUST ship with a regression test that reproduces the failure. Protocol changes MUST keep a
version check with a clear user-facing message for incompatible peers.

### VII. Simplicity
Start with the simplest design that satisfies the requirements in docs/product-requirements.md.
Out-of-scope items listed there MUST NOT be built without amending the requirements first.
Added complexity MUST be justified in the plan's complexity-tracking section.

## Technical Constraints

- Core engine in Rust; UI in Tauri (tray/menu bar app, display arranger, health panel).
- Transport is QUIC, chosen for connection migration. Deviations require amending
  docs/technical-decisions.md and this constitution if a principle is affected.
- Input: macOS CGEvent tap + IOHID injection; Windows Raw Input + SendInput (service later for
  login/UAC); Linux libei/InputCapture portal on Wayland plus X11.
- macOS builds MUST be signed with Developer ID and notarized so Accessibility permission
  survives updates and LAN auto-update works.
- Permissions setup MUST be guided per OS; missing permissions MUST surface in the health panel.

## Development Workflow

- Work follows the Spec-Driven Development pipeline: specify, plan, tasks, implement, in phases.
- Requirements live in docs/product-requirements.md and technical decisions in
  docs/technical-decisions.md; specs and plans MUST stay consistent with them or amend them.
- Every plan MUST include a Constitution Check against the principles above.
- Changes MUST pass formatting, linting and the test suite before merge.
- Every tasks.md MUST include tasks in its Polish phase that update the `docs/` directory to
  match what the feature actually delivered (requirements, technical decisions, and any user or
  operator documentation affected), so `docs/` never drifts from the code. A feature that changes
  nothing under `docs/` MUST still carry a task that verifies this and records it.
- Workspace bootstrap command (Principle 5.2): `cargo fetch --locked && cargo xtask bootstrap`. It fetches
  locked dependencies and installs the pinned dev tools (cargo-llvm-cov) so a fresh worktree can run
  package-scoped gates; it never starts or provisions E2E infrastructure.

## Spec-Driven Development Methodology Amendment

### Purpose

This amendment defines a cohesive set of principles governing specification quality,
phase discipline, testing strategy, and implementation integrity. These principles
form a closed system — each reinforces the others. They MUST be adopted as a whole;
the generating phases resolve them from the constitution at runtime (Principle 1.5).

> **Non-normative**: This amendment is intentionally technology-agnostic. The
> phase commands translate these principles into project-specific constraints
> at generation time, reading the constitution live (Principle 1.5). Engineering
> teams should read this document to understand the intent behind each principle
> before customizing their constitution.

---

### 1. Process Discipline

#### 1.1 Artifact Completeness

Each phase MUST produce a self-sufficient artifact. A self-sufficient artifact is
one from which an agent with no prior session context can execute the subsequent
phase correctly and completely. Session context, conversational history, and
agent memory MUST NOT serve as inputs to any phase. The artifact is the sole
valid mechanism for transferring intent between phases.

A phase MUST NOT be declared complete until its output artifact satisfies this
requirement.

> **Non-normative**: Three compounding reasons: it enforces a completeness
> mindset ("would a cold agent understand this?"); it prevents context
> contamination (exploratory reasoning and rejected approaches must not bleed
> into the next phase); and it prevents context-compaction degradation in long
> sessions. Starting each phase fresh with clean artifacts avoids all three.

---

#### 1.2 Phase Session Boundaries

A new agent session MUST be started for each phase. The preceding phase's artifact
MUST be the primary input to the new session. No other outputs, summaries, or
context from the preceding session MAY be carried forward.

> **Non-normative**: This is the enforcement mechanism for Principle 1.1. The
> session boundary makes completeness violations immediately visible — an
> insufficient artifact makes the new session fail or produce poor output.

---

#### 1.3 Clarification Markers and the Tasks Gate

`[NEEDS CLARIFICATION]` is this methodology's single vocabulary for a decision
owed to the operator. Any phase MUST record one, in the artifact where the
question lives, for any unanswered open question, unresolved risk, undecided
E2E journey proposal (Principle 4.3), undecided test-double introduction
(Principle 4.1), undecided test-infrastructure gap (Principle 3.4), or any
other decision an agent lacks authority to make.
Hedging text is not a substitute for a marker (Principle 2.1), and a marker is
not a substitute for deciding what the phase has authority to decide.

Only a recorded operator decision clears a marker. Accepting a risk is a
decision — it clears the marker and remains on record so downstream phases
inherit it knowingly. A marker MUST NOT be cleared by an agent's own judgment,
by silence, or by the passage of time.

**The tasks gate.** The tasks phase MUST NOT generate tasks while any
`[NEEDS CLARIFICATION]` marker remains anywhere in the feature's artifacts.
The check is mechanical — the marker is a literal, greppable token — and MUST
be performed before generation begins; where the project has hook tooling
(Principle 1.5), a `before_tasks` hook MAY enforce it automatically. A halted
tasks phase reports the outstanding markers as the named list of decisions the
operator owes.

> **Non-normative**: Task generation is where ambiguity stops being cheap:
> every phase before it can carry an open question at no cost, but an
> unresolved question that reaches task generation gets silently answered by
> implication — baked into task text by an agent with no authority to decide
> it, then defended by every downstream gate as if it were specified. This
> has been the methodology's most damaging recurring failure. The gate
> converts it into a halt with a named list. The clarify workflow stops being
> optional politeness and becomes the standing answer to "tasks refused to
> generate."

---

#### 1.4 Deferred Work Discipline

When any phase discovers a gap, defect, or follow-up need, that discovery
MUST be evaluated, and the evaluation MUST end in one of two outcomes: the
work is incorporated into the current feature, or it is recorded as deferred
work through the project's established mechanism for tracking action items
(an issue tracker, a designated debt log, or equivalent). A discovery MUST
NOT be dropped, and it MUST NOT be deferred without a record.

The evaluation is bounded. A discovery is deferred only when (a) the fix
crosses a package boundary (Principle 3.3) the current feature does not
otherwise touch, or (b) the fix would change a feature's specified behavior
— it would require the specification to say something different than it
currently does. Every other discovery is incorporated. Fixable technical debt
is never one of these cases and MUST NOT be deferred; effort, size, or time
remaining are not grounds for deferral.

A source-code comment MAY reference a deferred-work record but MUST NOT be
the sole place the discovery is captured.

The plan phase MUST identify the project's tracking mechanism, the way
Principle 3.4 identifies testing infrastructure. If a discovery requires
tracking and no mechanism exists, the plan phase MUST NOT invent its own
convention — it MUST halt and surface the absence to the user as a blocking
gap, resuming once the user establishes a convention or directs how the
discovery should be recorded.

> **Non-normative**: Comments are cheap to write and easy to lose — nothing
> surfaces or triages them. The two cases are the whole bound on deferral: a
> fix that stays inside the packages this feature already touches and leaves
> specified behavior unchanged is this feature's work, whatever its size.
> Halting when no convention exists mirrors this principle's own subject
> matter: how a project tracks deferred work is a standing project-wide
> decision that belongs to the user, not to whichever feature trips over the
> gap first.

---

#### 1.5 Constitutional Enforcement via Runtime Resolution, Hooks, and Vanilla Managed Files

Constitutional principles are non-negotiable, but encoding them into
spec-kit's managed files (command files, shipped templates) blocks clean
upgrades. Enforcement uses these mechanisms, in sequence:

1. **Runtime resolution** (first, sufficient for the common case): the
   constitution — `.specify/memory/constitution.md` — is the single source of
   truth. The phase commands that generate artifacts load it live at
   generation time and derive each phase's constitution gates from it. This is
   spec-kit's own default mechanism (v0.14.4+): shipped templates carry
   placeholder gate sections for exactly this purpose. Templates stay pristine
   vanilla, freely refreshable on upgrade; **no propagation step exists or is
   needed** — amending the constitution file is the complete act.

2. **Extensions and hooks** (then): where constitutional changes require
   automated validation or workflow injection beyond generation-time
   derivation, use extension manifests and lifecycle hooks
   (`before_specify`, `after_plan`, `before_tasks`, …). Hooks validate
   compliance without modifying managed files.

3. **Managed files** (never): do NOT modify spec-kit's command files or
   shipped templates, directly or via template-override presets. In
   particular, do NOT author a preset using `strategy: "append"` template
   fragments: the shipped resolver is strategy-blind and returns the first
   matching file wholesale, silently replacing the entire template. This
   prohibition holds until the shipped resolver actually composes strategies.

Teams that want constitution gates materialized into template files MAY opt
into spec-kit's bundled `constitution-sync` preset as an explicit, documented
escape hatch, accepting that those edits do not survive reconciliation and the
constitution command must be re-run after any upgrade. This is never the
default.

> **Non-normative**: This principle's mechanism has moved twice, both times to
> track what shipped spec-kit actually executes — direct template edits
> (blocked upgrades), then an append-strategy preset (a live run proved the
> shipped resolver ignores strategies and corrupts templates). Runtime
> resolution is upstream's own current default and requires building nothing.
> Migrating off either older mechanism: remove the project-local preset if
> installed and force-refresh managed files to pristine — pristine templates
> are the correct end state, not a loss.

---

### 2. Specification

#### 2.1 Specification Primacy

The specification phase is the highest-leverage phase in the development lifecycle.
Errors introduced at this phase compound through every subsequent phase. The
specification phase MUST receive proportionally greater analytical rigor than any
other phase.

The specification MUST define intent only: what the system must do and why. How
the system does it MUST be deferred entirely to the plan phase. A statement belongs
in the specification if and only if removing it would change what the system is
required to do.

**Economy.** The removal test applies at clause granularity, not only to whole
statements. A requirement or acceptance criterion MUST NOT restate what its own
language already entails ("completes successfully" already excludes an error
being raised), re-assert behavior another requirement or criterion already
establishes, or enumerate the absence of outcomes its stated outcome already
rules out. Before the specification phase is declared complete, it MUST make an
economy pass over requirements and acceptance criteria, deleting every clause
whose removal leaves the required behavior unchanged.

Specification text states decisions, not proposals. Requirement sections MUST
NOT hedge the decisions they record: markers such as "proposed", "suggested",
"best-effort", "subject to <a future event>", "placeholder", or "TBD" are
non-compliant in requirement text. When the author knows a requirement will be
revisited by later work, that fact MUST be recorded as a plain statement of
future work — in Assumptions or the reference-artifact record — and it does not
diminish the requirement's authority during implementation. A missing input
(Principle 2.6) is resolved by deciding anyway and noting the decision's
provenance, never by demoting the decision to a proposal. A question the author
cannot decide is not hedged either — it is raised as a `[NEEDS CLARIFICATION]`
marker (Principle 1.3).

> **Non-normative**: Technology choices and implementation sequences that
> appear in a specification become invisible constraints inherited uncritically
> by the plan. The no-hedging rule is field-driven: a spec that fully decided a
> UI affordance but labeled it "subject to design pass" was read downstream as
> revocable, and an autonomous run argued the affordance out of scope. A hedged
> decision is read as a revocable one. The economy rule is the mirror image:
> downstream agents read every clause as independently load-bearing, so a
> redundant clause becomes defensive code guarding a case that cannot occur, or
> a duplicative test re-verifying what another criterion already covers — and
> the tendency scales with the reader's capability, so the fix has to live in
> the artifact, not the reader.

---

#### 2.2 Personas

Every project MUST maintain a personas record — `.specify/memory/personas.md`,
beside the constitution — defining each person on whose behalf the system's
behavior is specified. The generating phases read it at runtime (Principle
1.5), the way they read the constitution.

Each persona entry MUST state three things:

- **Role**: what the persona is responsible for, and therefore which outcomes
  they care about.
- **Vantage**: the surfaces through which the persona does that role and
  observes the system — screens, pages, command output, check statuses,
  notifications, dashboards, tool responses. The vantage lists what the
  persona legitimately uses, not what they are capable of understanding.
- **Horizon**: what lies beyond the vantage — the components, storage,
  transports, code, configuration, and step ordering the persona neither sees
  nor needs to know exists.

A persona is expert in their own domain and ignorant of this system's
internals. Engineering personas — an operator, an on-call engineer, a
contributor opening a pull request, a developer running a local
environment — are first-class entries with real vantages (pod status, a CI
check, a pager, a terminal) and real horizons (this system's internals). A
persona's technical fluency does not widen its horizon: an engineer who has
never read this service's source does not know its topic names.

The system, the platform, a service, a pipeline, or a test harness is never a
persona. Where a change appears to have no person, the person is whoever
notices when it breaks.

**Admission.** The record MAY also define shared domain terms. A term is
admitted only when it can be defined from a persona's vantage; a term whose
definition must name a component, table, topic, file, or mechanism is an
implementation name and MUST NOT be admitted. The specify phase MAY propose a
new persona or term but MUST NOT add one silently: a needed entry that does
not exist is a `[NEEDS CLARIFICATION]` marker (Principle 1.3).

> **Non-normative**: A persona is a vantage point, not a job title. The record
> exists so that Principle 2.3's tests are a lookup rather than a judgment
> call — without it every author reinvents the persona per feature, and for
> engineering-internal work gives up and lets the system narrate itself ("As
> the build system, I want…" appeared verbatim in the field). The admission
> rule guards against laundering: audited specs pre-emptively defended
> "specific endpoints, environment variable names, and system components" as
> necessary vocabulary. If the definition needs the internal name, it is an
> internal name.

---

#### 2.3 Persona Vantage

Every user story MUST name a persona defined in the personas record
(Principle 2.2), or several personas that share a vantage and surface, and
MUST be written from that vantage.

**Story format.** The title text between the template's story number and its
priority MUST be the story sentence, in the form "As a [persona], I want
[capability]", with no "so that" clause. A story has one intent. Situations,
variants, consequences, and error cases are not part of the story sentence;
they are acceptance criteria.

**Horizon test.** Every outcome clause of an acceptance criterion, and every
functional requirement that names a persona, MUST be confirmable by the named
persona from their vantage, without knowledge of anything beyond their
horizon. A sentence that is true only because a particular component, table, topic, library, file,
credential, configuration key, or step order is involved describes mechanism,
not intent: it MUST be rewritten as the observation the persona would make, or
moved out of requirement text — into the Plan-Phase Handoff section (Principle
2.6) or the plan itself.

> **Non-normative**: A story title reads, for example, `### User Story 1 - As
> an agent, I want to know which teamspace each search result comes from
> (Priority: P1)`. The benefit a "so that" would carry is already stated under
> "Why this priority", so the title stays short. The story sentence is not
> held to the horizon test because its "want" states the persona's real intent,
> which can reach beyond what that persona observes: in "As a user, I want the
> ability to cancel my subscription", the user's criteria confirm only what the
> user sees, such as Billing showing the subscription cancelled, and the
> part beyond their horizon is covered by another persona's story or by a
> functional requirement.

**Surface test.** An outcome MUST be observable through a surface in the
persona's vantage. A criterion that has the persona confirm an outcome through
a surface their role does not use — an administrator opening a metrics
dashboard to learn whether their own action succeeded — is not a mechanism
leak but a design gap: the outcome is not surfaced where the role works. Such a
criterion MUST be rewritten to name the outcome on the persona's own surface,
or the gap raised as a `[NEEDS CLARIFICATION]` marker (Principle 1.3).

**Collapse.** After both tests are applied, two acceptance criteria whose
outcome clauses read the same are one criterion, and two stories whose
criteria all read the same are one story. Two stories with the same want and
the same persona, or personas that share a vantage and surface, are one story;
what distinguishes them — situations, variants, error cases — becomes
acceptance criteria of the merged story. Stories whose personas work through
different surfaces stay separate, even when their criteria read the same.
There is no minimum or maximum
number of criteria per story: a story is as large as one persona-observable
outcome requires, and is split only when its criteria observe different
outcomes. Splitting is re-analysis of the outcome, never redistribution of
criteria along an implementation axis — per component, per surface, per
environment, per code site.

**Inputs.** Tickets, PRDs, and typed requests are inputs, read for intent.
Stories they contain are not presumed to follow this principle's story rules:
the specification phase keeps their intent and rewrites their form —
regrouping them under the story format and the collapse rule, moving variants
into acceptance criteria, and moving mechanism to the Plan-Phase Handoff
section (Principle 2.6). Their intent is binding; their structure and wording
are not. Inputs are not reference artifacts and are not copied into `specs/`
under Principle 2.6. Where it is unclear whether an element of an input is
normative, the specification phase MUST record a `[NEEDS CLARIFICATION]`
marker (Principle 1.3) rather than guess. Regrouping stories is within the
phase's authority and is simply done; whether an element is normative is the
operator's decision. The specification's Input line MUST record each input's
identifier and location. Artifacts attached to or referenced by an input are
reference artifacts under Principle 2.6.

**Instance criteria.** An acceptance criterion that names a specific member of
a homogeneous target set — a particular service, table, or file, distinguished
only by being what currently exists — MUST be rewritten as a single *instance
criterion*: a claim universally quantified over the category, not an
enumeration of its current members. Two or more criteria, stories, or
requirements that differ only in the identity of the target they name express
one specification-level claim and belong to one story. This is the rule that
keeps a requirement of the form "apply this change in package A, B, C, and D"
from exploding into four stories: it is one claim about a category. A single
criterion is not required to be verifiable by a single test — an instance
criterion quantified over a target set may be verified by one test per
member.

**Completeness criteria.** A specification for a removal, replacement, or
migration MUST additionally include a *completeness criterion* — a claim that
nothing outside the known target set still exhibits the prior behavior —
whenever that target set is not closed by construction (an enum, a fixed
registry). A completeness criterion is not collapsible into the instance
criterion: the instance criterion is a claim about members of the set, and is
structurally silent about anything not already a member. Its acceptance test
verifies a point-in-time claim about the state the removal establishes; it MAY
be satisfied by a verification-only task (Principle 5.1, rule 1) performed once
during implementation rather than a persisted `acc_` test. Where the plan phase
judges the removed pattern an ongoing risk, it MAY task a lint rule or
architectural principle (Principle 1.5) to guard recurrence — that judgment
belongs to the plan phase, not to this principle by default.

**Vantage pass.** Before the specification phase is declared complete, it MUST
apply the horizon and surface tests to every acceptance criterion and every
functional requirement that names a persona, and MUST apply the collapse rule
across stories, not only within one.

> **Non-normative**: This principle replaced a numeric ceiling on criteria per
> story. An audit of 114 field specifications found the ceiling satisfied by
> exactly what it was meant to prevent: the cheapest way to fill a story was
> one criterion per implementation unit — per repository path, per service,
> per environment, per consuming surface — with the surplus overflowing into a
> new story on the same axis, and the duplicates distinguishable only by the
> implementation word. Policing the counted section moved the mechanism into
> the uncounted ones. The vantage is decidable per sentence where "is this
> implementation?" invites argument: a queue is what the system does from an
> engineer's seat and invisible from an administrator's. Persona-level writing
> is achievable for deeply technical work — audited exemplars abstracted OAuth
> client registration to "guided through a standard sign-in flow without any
> manual registration by an administrator" and a content hash to "a marker
> identifying the exact version it read" — and the one author who refused to
> split a six-criterion story because "a fix covering only some cases would
> leave the bug live for the others" produced the cleanest story audited. A
> drafting test for instance criteria: could this criterion have been written
> correctly without opening the codebase, knowing only the shape of the
> contract being changed? A criterion that names a specific file because that
> is what currently exists was transcribing today's inventory, not stating a
> requirement — and is blind to any instance added after it was written. The
> instance criterion and the completeness criterion answer different questions:
> writing only the former can be fully satisfied while leaving behind exactly
> the stragglers a migration was meant to catch.

---

#### 2.4 Edge Case and Functional Requirement Traceability

Every edge case and functional requirement MUST be explicitly attributed to the
user story or acceptance scenario it qualifies. A global edge cases section and
a functional requirements section MAY exist as navigational aids, but each entry
MUST include an explicit reference to the user story it affects.

Functional requirements MUST NOT restate behavior already captured as acceptance
criteria. A functional requirement is valid only when it captures a constraint,
data rule, or system-wide behavior that does not belong to any single user story,
or when it names a reference artifact under Principle 2.6, which it MAY do for
a single story. Each such requirement MUST identify which user stories it
affects.

The specify phase MUST NOT be declared complete while any edge case or
functional requirement lacks an explicit user-story attribution.

> **Non-normative**: A cold agent generating tasks processes user stories
> sequentially. Edge cases without story attribution are silently dropped,
> attached to the wrong story, or orphaned. If an edge case cannot be
> attributed to any story, it is either a missing story or behavior that was
> never actually specified.

---

#### 2.5 Dependency-Aware Story Sequencing

User stories MUST be sequenced such that each story is fully testable when its
implementation phase is reached. A story is fully testable only when all stories
it depends upon have been implemented and verified. Stories that would require
unimplemented functionality to satisfy their acceptance criteria MUST NOT be
sequenced before that functionality is available.

Within the constraint of valid dependency ordering, teams have full discretion
over story sequencing.

> **Non-normative**: Teams frequently sequence by perceived importance rather
> than dependency, producing stories whose criteria cannot pass because a
> prerequisite behavior has not been built. The foundational story comes first
> regardless of relative importance.

---

#### 2.6 External Reference Artifacts and Plan-Phase Handoff

The specification phase MUST actively identify all external reference artifacts
that inform the feature: Claude artifacts and other prototypes or POCs (often
functional wireframes), wireframes, design mockups and design files, UX
documents, content (copy, images), API specifications and schema documents
(OpenAPI, AsyncAPI), data models, architecture diagrams, and any other
authoritative sources that shape requirements or constraints. Tickets, PRDs,
and typed requests are inputs, not reference artifacts (Principle 2.3).

**Discovery**: "Identify" means actively searching for and cataloging relevant
artifacts, not passively noting them if encountered. For each identified
artifact the specification MUST document its source, location, and relevance.

**Integration**: Any identified artifact that exists as a file capable of being
copied into the repository MUST be copied into `specs/{feature}/` and
explicitly referenced from the specification. Material that is not copied in
is not an artifact and is non-binding.

**Use.** An artifact is read for intent: what it shows is normative unless the
specification states otherwise. Every artifact MUST be named by at least one
functional requirement that states how it is used. Where the specification
phase cannot tell how an artifact is to be used, or whether an element of it
is normative, it MUST record a `[NEEDS CLARIFICATION]` marker (Principle 1.3)
rather than guess.

> **Non-normative**: A functional prototype stored in the spec but treated as
> illustration leaves behavior that only the mockup showed unbuilt. Typical
> use statements: "Cancellation follows the design in cancel-wireframe.html",
> "/privacy shows privacy-policy.md as written", "The endpoint conforms to
> openapi.yaml".

**Incomplete artifact handling**: An identified artifact that cannot be
accessed MUST be flagged as an explicit risk in the Assumptions section. The
specification MUST NOT proceed to a complete state while a critical reference
artifact remains missing without a documented resolution. For a design artifact
that does not exist, the documented resolution is for the specification to make
the design decisions itself and record them as binding requirements with
provenance noted — never softened into proposals (Principle 2.1).

This principle governs the *availability* of handed-off reference material;
empirical verification of external system *behavior* is Principle 3.1's.

**Plan-phase handoff.** Implementation knowledge the author holds at
specification time — a known component, a likely mechanism, a constraint of
the existing code, a suspected root cause — is neither discarded nor written
into requirement text. The specification MAY carry a **Plan-Phase Handoff**
section for exactly this material. Its contents are non-binding: the plan
phase MUST consume each item as a hypothesis to verify (Principles 3.1 and
3.2), never as a requirement, and MUST record for each whether it was
confirmed, refuted, or superseded. No requirement may cite the handoff section
as its source, and no acceptance test may be derived from it. The section is
exempt from Principle 2.3's horizon test and is the only part of the
specification that is.

> **Non-normative**: External references are uniquely fragile in a handoff
> workflow — vivid at specification time, gone by implementation, because
> links rot and access changes. Copying the artifact into the repository fixes
> it in place for the lifetime of the project. The handoff section resolves a
> standing tension of the same shape: authors who know how the system is built
> had nowhere legitimate to put that knowledge, so it went into acceptance
> criteria, where downstream agents read it as binding. A labeled, non-binding
> home removes the incentive to leak; the hypothesis-not-requirement rule is
> what keeps the section from becoming Principle 2.1's invisible constraint by
> another name.

---

#### 2.7 Specification Inference Discipline

The specification phase MUST NOT infer error handling requirements or timing
constraints. These MUST always be explicitly requested by the user.

- Error handling for unspecified failure conditions MUST NOT be inferred.
- Performance, latency, or timing constraints MUST NOT be inferred.
- If error handling or timing appears necessary but has not been requested,
  the specify phase MUST record the gap as `[NEEDS CLARIFICATION]` (Principle
  1.3) rather than adding the requirement.

> **Non-normative**: Invented fallback paths, retry policies, and SLOs become
> invisible requirements the implementation faithfully builds —
> indistinguishable, once built, from ones the user asked for. Principles 5.5
> and 5.6 prohibit implementing unspecified error handling; this principle
> closes the upstream leak by prohibiting its specification.

---

### 3. Planning

#### 3.1 Empirical Validation of External Dependencies

The plan phase MUST identify every dependency whose behavior lies outside the
repository's control — third-party APIs, external services, SDKs, data
sources, platform APIs. This identification is the plan phase's own
responsibility; implementation dependencies are excluded from the
specification by design and never surface as clarification markers on their
own.

For each such dependency the plan relies upon, the plan phase MUST empirically
validate the assumptions the plan depends on by executing throwaway probe code
against the real dependency (or an equivalent sandbox): reachability and
authentication, request/response shapes, error and failure modes. Probes
SHOULD prefer read-only or idempotent operations and test credentials, and
SHOULD avoid mutating production external state; where no safe surface exists,
a probe MAY proceed with caution and that fact MUST be recorded.

Probe artifacts MUST be preserved in the feature's `specs/{feature}/spikes/`
directory with a `spike_` filename prefix, referenced from the plan's research
record, and excluded from coverage (Principle 4.2) — they are exploratory
infrastructure, not tests. Findings MUST be recorded with explicit evidence:
for each validated assumption, the observed result and the probe that produced
it.

Dependencies whose probe fails, or for which no probe could run, MUST be
recorded as risks and surfaced to the operator through the Plan Summary
(Principle 3.7). An unresolved risk the operator has not decided is a
`[NEEDS CLARIFICATION]` marker (Principle 1.3); a risk the operator accepts
is a recorded decision and clears it, remaining on record so implementation
inherits it knowingly. The plan phase MUST NOT silently proceed past an
unresolved external-validation risk, and MUST NOT begin implementing the
feature to resolve one — empirical validation is a planning instrument only.

Research and spikes are never skipped for schedule reasons: every execution
mode (Principle 3.6) runs them in full.

> **Non-normative**: An autonomous implementer rarely fails on logic; it fails
> on unknowns about the world outside the repository — an auth header that
> isn't what the documentation implies, an error returned as `200 OK`. A
> planner that resolves these by reading documentation hands the implementer a
> landmine. This principle is the constructive complement to 2.7: where 2.7
> forbids inferring external behavior, this one requires verifying it. The
> preserved probes hand the implementer runnable evidence instead of prose.

---

#### 3.2 Empirical Plan-Phase Gap Analysis

For a feature that touches more than one package (Principle 3.3), the plan
phase MUST perform this gap analysis before the plan is declared complete, in
order:

1. **Package pass.** For each declared package, empirically investigate that
   package's actual code and tests — not the plan's description of it — for
   gaps between what the plan assumes and what is present. Each finding
   records an impact rating (CRITICAL/HIGH/MEDIUM/LOW) and an evidence
   weight — confirmed-by-reading or suspected-not-yet-checked — citing the
   specific code or test read. A weight claim without a citation is
   non-compliant.
2. **Seam pass.** For each declared contract, exactly one boundary check: read
   both sides' package-pass findings, then verify the producing side's actual
   behavior matches what the consuming side assumed. Do not additionally check
   package pairs that share no declared contract.
3. **Mechanical rollup.** Merge and deduplicate the two passes' findings —
   never re-derive from a fresh top-down reading. A suspected finding MUST NOT
   surface as CRITICAL; it surfaces as suspected/unconfirmed regardless of
   potential severity.
4. **Coverage critic.** Verify only that the passes' union spans every touched
   package and declared contract — never re-check individual findings.
5. **Routing.** A gap the plan phase has authority to resolve is fixed and the
   affected pass re-run. A gap requiring an operator decision is surfaced
   through the Plan Summary (Principle 3.7) and, if undecided, recorded as
   `[NEEDS CLARIFICATION]` (Principle 1.3).

This principle is the internal analog of Principle 3.1: where 3.1 probes
an external dependency's real behavior, this one reads an existing package's
real code instead of trusting the plan's description of it. How the plan phase
organizes the work — inline or delegated to sub-agents — is not this
principle's concern.

> **Non-normative**: A plan can pass every other gate while being wrong about
> the packages it depends on, because its understanding is often built from
> prior familiarity rather than fresh reading. The evidence-weight cap applies
> 5.4's evidence-over-declaration stance to analysis claims: an unread guess
> must not outrank a confirmed finding.

---

#### 3.3 Package Boundary Declaration and Contracts-Before-Implementation

For a feature whose implementation spans a monorepo's package or workspace
boundaries — whatever unit the project's tooling treats as an independently
buildable/testable module — the plan phase MUST identify, for each user story,
the set of packages its implementation touches. A story touching exactly one
package is package-local; a story touching more than one is multi-package.

For a feature with any multi-package story, the plan phase MUST design the
shared interface — API contracts, shared types, generated stubs — every
touched package implements against, recorded as spec-kit's `contracts/` design
artifact. Every contract a multi-package story depends on MUST be fixed in
this artifact before any task implementing against it is sequenced: the tasks
phase MUST place every task that defines or generates a shared contract
strictly before every task that consumes it. Once fixed, a contract MUST NOT
change silently — altering its shape is a plan-level decision, recorded by
returning to the plan, never made ad hoc inside a downstream task.

This principle is a package-aware corollary of Principle 2.5: where 2.5 orders
whole stories by their dependencies, this principle orders the pieces of a
single story that cross a package boundary — every touched package depends on
the same fixed contract.

> **Non-normative**: A shared interface discovered rather than designed is the
> most common source of late-stage rework in a multi-package change: two
> packages independently converge on incompatible assumptions about a boundary
> neither was asked to fix. Fixing the contract at plan time settles the
> design decision where it can still be reviewed cheaply. The frozen contract
> is also what licenses package-parallel execution (Principles 3.6 and 5.2):
> it is the invariant that keeps independent lanes independent.

---

#### 3.4 Acceptance and Contract Infrastructure Detection and Planning

Acceptance testing (BDD runners, E2E infrastructure) and contract testing
(contract artifact tooling, generated clients, schema registries, drift gates,
the separate credential-gated config real-provider tests need) require
supporting tools, CI integration, and environment setup — often missing when
SDD adoption begins or when constitutional changes introduce new testing
requirements.

The plan phase MUST detect whether the testing infrastructure the feature's
plan relies on is present and functional: test framework and BDD tooling for
the target platform, CI execution of acceptance tests, environment
configuration for integrated tests, and — for any feature with a service
boundary (Principle 3.3) — the contract-test conventions and tooling
Principle 4.1 requires (where `contract_` tests live, what verifies them, how
generated contract artifacts are kept from drifting).

The infrastructure this principle mandates serves testing and development
only — it is never a channel for changing what ships. A setup task that would
alter shipped code or behavior is instead raised as a `[NEEDS CLARIFICATION]`
marker (Principle 1.3).

If critical infrastructure is missing, the plan phase MUST NOT proceed to task
generation. It MUST document the gap explicitly, create
infrastructure-setup tasks that precede all feature tasks, and ensure they
appear in tasks.md before any story implementation.

> **Non-normative**: Missing infrastructure creates a cliff edge
> mid-implementation when the first acceptance or contract test fails and
> there is no runner. Early detection makes setup an explicit, ordered
> prerequisite instead of a mid-run discovery. Designing production code for
> testability (Principle 4.2) is feature work, not test infrastructure; the
> ships-boundary bars setup tasks from adding shipped behavior that exists
> only to serve the tests.

---

#### 3.5 Plan Sequence Diagrams

The plan phase MUST produce a Mermaid sequence diagram for each user story,
included in plan.md. Sequence diagrams constitute the implementation contract
for the tasks phase — tasks MUST be derivable from and consistent with the
diagrams.

> **Non-normative**: Diagrams surface integration points, async boundaries, and
> dependency assumptions that look sound in prose but reveal structural
> problems when drawn. A contradiction between a diagram and a generated task
> means one of them is wrong — the diagram is the reference.

---

#### 3.6 Execution Mode Classification

Every feature is implemented under one of three execution modes. The plan
phase MUST classify the feature — after research, spikes, and the package
boundary declaration are complete — declare the mode and its justification in
plan.md, and surface it in the Plan Summary (Principle 3.7). The tasks phase
consumes the declared mode; it MUST NOT invent or revise the structure itself.
Research and spikes (Principle 3.1) run in full under every mode — the modes
differ only in the shape of tasks.md and the number of implementation phases.

**Fast pass.** Qualifies when ALL hold, assessed against the settled plan:

- **No contract changes**: the boundary declaration (Principle 3.3) shows no
  multi-package story and no change to any shared contract artifact.
- **1–3 implementation files**: the expected diff creates or modifies at most
  three production files with executable behavior — excluding tests,
  generated code (Principle 4.2's definition), and declaration-only files
  (types, interfaces, constants, schema declarations with no executable
  behavior). This ceiling is this amendment's default; a project's
  constitution MAY override it.
- **The mechanism is not the deliverable**: a specification whose requirements
  demand the full pipeline's machinery cannot be satisfied by skipping it.
- For a defect fix: the suspected cause has been empirically reproduced or
  refuted first (Principle 3.1's discipline applied to the defect) — never
  planned around unverified.

Task structure: exactly two phases — one implementation phase containing every
story's tasks, gated once (Principle 5.4) against all stories' acceptance
criteria; then one terminal verification phase carrying the aggregate gate and
the E2E backstop per Principle 4.3's execution timing.

**Standard.** The default when fast pass does not qualify and package-parallel
is not warranted. Task structure: the sequential shape — Setup, Foundational,
one implementation phase per user story in Principle 2.5's order, Polish —
with per-phase gates (Principle 5.4) and E2E only at the terminal boundary
(Principle 4.3).

**Package-parallel.** Warranted when multiple user stories cross multiple
package boundaries (Principle 3.3) — enough disjoint package work that
parallel lanes repay their fixed overhead. The frozen contracts are what
license the split. Task structure: Principle 5.2.

**Mode consistency.** Every plan artifact that assigns per-phase gate or
verification commands MUST be expressed in the declared mode's shape. A plan
whose gate designation contradicts its own declared mode is internally
inconsistent and non-compliant.

**One-way escalation.** If, during implementation, the work outgrows the
declared mode — a fast-pass diff exceeds the file ceiling or touches a
contract; a standard feature's stories turn out to need contract work not
planned — implementation MUST stop, record the reclassification, and continue
under the heavier mode. The plan artifacts remain valid inputs; escalation
re-cuts tasks.md, nothing more. De-escalation mid-implementation is not
permitted, and uncertainty at classification time resolves toward the heavier
mode.

> **Non-normative**: Roughly half of all features in a mature project are
> small, and a one-shape methodology taxes them worst: a 1–3 file change pays
> five phases of sessions, commits, and gate passes where two suffice. The
> classification sits in the plan phase because that is the only phase holding
> everything the decision needs — the research, the boundary declaration, the
> gap analysis — and pushing it later forces the tasks phase to re-derive it
> cold. Escalation is one-way for the same reason Principle 4.3's uncertainty
> rules resolve toward rigor: over-rigor costs bounded overhead, under-rigor
> costs unbounded retries. Field data from an independent implementation of
> the same idea: small tickets ran at roughly one-third the token cost with no
> quality loss, with the reversal path exercised and cheap.

---

#### 3.7 Plan Summary Output

Upon completion, the plan phase MUST output a human-readable summary to the
user identifying: user stories covered, key architectural decisions, risks
identified, the declared execution mode (Principle 3.6), the debt survey's
findings (Principle 3.8), any proposed end-to-end journeys awaiting decision
(Principle 4.3), and anything explicitly deferred — with references to the
specific sections of plan.md and spec.md supporting each item.

The summary is a navigation aid for human review. It MUST NOT introduce new
content not present in the plan artifact, and MUST NOT be written into plan.md.

> **Non-normative**: The summary gives the reviewer a structured surface for
> spotting problems without full-document review. It disappears with the
> session, which is correct: its audience is the human at the phase
> transition, not the agent starting the tasks phase.

---

#### 3.8 Technical Debt Survey

The plan phase MUST survey the files the feature will touch and the existing
code its implementation will build on, identify technical debt and defects
present there, and plan their removal as tasks within this feature. A file
or code path is admitted to the survey when either holds: the feature will
touch it, or cleaning it up simplifies this feature's implementation.

The survey looks for debt in three tiers, in increasing order of judgment
required to find it:

1. **Suppression directives** that fight the project's quality gates —
   coverage-ignore markers, lint-disable comments, type-check-ignore
   directives, skipped or focused tests, and their equivalents in any
   toolchain. These are mechanical to find: a suppression is debt against
   the file that carries it (Principle 5.4).
2. **Local debt** a careful reviewer would flag line by line, including
   duplicated code that could be refactored into one place, dead code, stale
   TODO/FIXME comments, misleading names, and files that have sprawled past
   a size an engineer would tolerate. This tier is not limited to the
   examples named here.
3. **Structural debt**: places where a behavior-preserving restructuring
   would make this feature's implementation dramatically simpler, smaller,
   and more direct by using architecture that already exists in the
   codebase — so that whole branches, helpers, modes, or layers disappear
   rather than merely get rearranged. The plan phase MUST assume such a
   restructuring usually exists and look for it, and MUST prefer deleting
   complexity to rearranging it.

The survey's findings and the tasks they produce are recorded in plan.md and
tasks.md; that record is the evidence this principle requires. It
introduces no separate gate.

> **Non-normative**: Field experience: without an active effort against it,
> the first several features built under this methodology work fine — debt
> is not yet visible enough to slow anything down. Somewhere around the
> thirtieth feature, debt becomes a constant theme both the agent and the
> operator have to watch for. Per line, agentic implementation under this
> methodology accrues debt more slowly than hand-written code does — but an
> autonomous run does in a day what a human engineer does in a week to a
> month, so in wall-clock terms debt accumulates faster and has to be
> tackled at every opportunity, not saved for a cleanup phase that never
> comes. The plan phase is the cheapest point to do it: every restructuring
> planned here is one the implement phase builds on, instead of around.

---

### 4. Testing

#### 4.1 Test-Driven, Behavior-Driven, and Contract-Driven Development

All features MUST be developed test-first. Tests MUST be written before, or in
the same increment as, the implementation they verify. Implementation MUST be
written to satisfy tests that already exist for it.

Whether a test's failure was ever observed running — a "red phase" — is not a
requirement of this principle. No failing-run observation is required, and none
MUST be authored, reconstructed, or fabricated to satisfy this principle. The
requirement is that tests exist and pass for every behavior implemented — not
that their prior absence was demonstrated.

The plan phase MUST reflect test-first sequencing. A plan produced without
considering test-first constraints is non-compliant.

**The verification ladder.** When a behavior could be verified at more than one
layer, it MUST be verified at the lowest layer that can express it:

1. **Implementation tests** in the owning package;
2. **Contract tests** against the authored contract artifact or a sanctioned
   test double;
3. **Real-provider contract tests** against a live external dependency;
4. **End-to-end tests** — reserved exclusively for admitted journeys
   (Principle 4.3).

Anything expressible as a contract test MUST be written as a contract test,
never as an end-to-end test. An end-to-end test MUST NOT be used to verify a
seam, an interface, service logic, or validation behavior.

**Acceptance tests (BDD)** MUST be produced for every acceptance criterion in
the specification. Acceptance tests are authoritative: a failing acceptance
test constitutes a specification violation. They MUST be identifiable by both
of two conventions: an `acc_` filename prefix, and Gherkin where the platform
supports it (elsewhere, "acceptance" in the test name with Given/When/Then
structure). The prefix enables cross-session discoverability; the naming
convention keeps test output attributable where runners omit filenames.

An acceptance test verifies **runtime behavior** — the system doing something
observable when exercised. A deliverable that is a document — authored prose,
documentation, any artifact whose value is its content being read — is
verified by a human or reviewer reading it, never by a test asserting its
structure or regex-matching its prose. An acceptance criterion whose subject
is such a deliverable yields a verification-only review task (Principle 5.1,
rule 1), not an `acc_` test.

**Implementation tests (TDD)** MUST cover all production code not covered by
acceptance tests. They are not authoritative against the specification but
prevent regression. Together, acceptance and implementation tests MUST achieve
100% coverage of all production code.

**Contract tests (CDC)** MUST be produced for **each side of every service
boundary** — every seam where one package or service depends on an interface
another provides (frontend↔BFF API, BFF↔service HTTP, producer↔consumer event
schema). Both sides MUST be verified independently against the shared contract.
Contract tests are authoritative against the contract: because the contract is
frozen before implementation (Principle 3.3), a contract-test failure means
one side drifted, not that the interface is wrong. Contract test files MUST
carry a `contract_` filename prefix. There are two subtypes:

1. **Infrastructure-free contract tests** (the default): run in-process against
   the contract artifact (JSON Schema, OpenAPI document, generated types) with
   no live cluster, no network, and no sibling service. This makes them
   *lane-resident*: they run inside a package's own implementation phase
   alongside its unit tests, and MUST NOT be deferred to an integrated
   environment.
2. **Credential-gated real-provider contract tests**: verify the project's
   assumptions against a third party's actual live API, where the contract
   under test is the provider's real behavior — something the project cannot
   author or freeze. These are exempt from the no-network clause for exactly
   that reason: the network call *is* the assertion. They MUST fail loudly
   (never skip) when credentials are absent, and they run in the owning
   package's own test lane, never in the end-to-end suite. They are the
   standing successor to a Principle 3.1 spike: the spike validates an
   assumption once at plan time; the real-provider contract test keeps it
   validated.

**Test double governance.** Test doubles (mocks, stubs, fakes, stand-ins) for
external dependencies are governed as follows:

- A test that cannot run is a test that was never written. Skip mechanisms are
  forbidden.
- Introducing a test double for an external dependency is an **operator
  decision, never an agent's**. An agent that encounters a new external
  dependency MUST surface the decision — real configuration or a sanctioned
  double — as a `[NEEDS CLARIFICATION]` marker (Principle 1.3) and stop,
  never route around the friction by adding a double on its own.
- A sanctioned double MUST name its **real-config debt** in the spec/plan: the
  exact credentials or configuration the real path needs once provided, plus a
  spike (Principle 3.1) validating that those credentials actually work when
  supplied — so the transition off the double is mechanically confirmable
  rather than the double silently running forever.
- A double that activates because real configuration is missing MUST produce a
  single aggregated, hard-to-miss run-level signal naming what ran against it.
  Per-test warnings are insufficient. A fallback that persists past initial
  development is a tracked defect (Principle 1.4), not an accepted state.

> **Non-normative**: BDD, TDD, and CDC are complementary. A failing acceptance
> test means the feature is broken by specification; a failing implementation
> test means a unit has a defect; a failing contract test means two components
> drifted apart at their interface. The ladder exists because the end-to-end
> environment is the slowest, flakiest, most expensive verification layer a
> project has — everything that can be proven below it must be. The
> real-provider subtype is field-proven: a live contract test discovered that a
> provider's real API returned no usage field at all — a fact no mock or
> authored artifact could surface — which had been silently corrupting
> production metering. The double-governance rules exist because a test that
> passes against a stub proves nothing about the real path, and a stub that
> silently covers for missing credentials removes the one prompt that would
> ever get real configuration added.

---

#### 4.2 Production Code Coverage

100% test coverage MUST be maintained for all production code. Production code
is defined as all code authored by humans or agents that is not excluded by
this principle.

Excluded from the coverage requirement:

- **Generated code**: produced by a tool, not intended for manual editing.
  Regenerating is the correct response to required changes; manually edited
  generated code loses its exclusion.
- **Test infrastructure**: harnesses, fixtures, factories, helpers, and other
  code whose sole purpose is to support test execution.

Coverage below 100% for non-excluded production code is a quality gate failure.
A coverage gap indicates either untested production code or production code
that implements unspecified behavior.

> **Non-normative**: 100% coverage is primarily a specification-compliance
> detector, not a quality metric: unreachable code either has no acceptance
> criterion (unspecified behavior was implemented) or no implementation test
> (logic was never validated). Both violate other principles here.

---

#### 4.3 End-to-End Acceptance Tests

End-to-end acceptance tests verify integrated user journeys through the
system's real public entry point. They are authoritative for integrated journey
verification — a failing E2E test is an integration failure of equal standing
to a failing acceptance test. They are also the slowest, most expensive, and
most failure-prone test layer a project has, and this principle governs them
accordingly: E2E tests are a scarce resource, admitted deliberately, extended
in preference to added, and executed only where nothing else can prove the
claim.

**Admission.** The plan phase MUST inventory the repository's existing E2E
journeys before proposing any new ones. A new E2E scenario is admissible only
when both conditions hold:

1. It proves behavior observable **only** through the real public entry
   point — a real cross-service side effect, a provider-hosted surface,
   event-driven fan-out that must be seen reaching the UI, or cross-surface
   parity between two public entry points to the same state; **and**
2. **No existing journey can absorb the assertion** as an additional step.
   Extending a journey is always preferred over adding a scenario. A plan
   proposing a new scenario MUST name the journey considered for absorption
   and why it could not take the assertion.

Coverage that fails admission belongs one layer down (Principle 4.1's
ladder) — this is the default, not the exception. The honest journey inventory
for most features is empty, and a plan MUST NOT invent a journey to appear
thorough.

**Operator decision.** Every proposed new or modified journey MUST be surfaced
through the Plan Summary (Principle 3.7) and decided by the operator. An
undecided proposal is recorded as a `[NEEDS CLARIFICATION]` marker (Principle
1.3) — agents never add E2E scenarios unilaterally. This amendment sets no
numeric suite budget; a project's constitution MAY declare one.

**Execution timing.** E2E tests execute only at the feature's terminal
phases — the terminal implementation boundary, and the Convergence phase in
package-parallel mode (Principle 5.2) — and only when the feature's
admitted inventory is non-empty (the feature added or modified a journey). A
feature with an empty inventory never stands up the E2E environment; the
project's own CI gate on the main branch remains the backstop for
cross-feature regressions. Story phases never execute E2E (Principle 5.4,
Gate 3).

**Structural requirement.** An admitted E2E test MUST exercise the system from
its real-world entry point through its real interaction surface — the same
entry point and surface an external actor would use. Tests MUST NOT bypass the
entry point by constructing internal components directly, or substitute the
interaction surface with test-only navigation. E2E test files MUST carry an
`e2e_` filename prefix, with Gherkin (or Given/When/Then naming) as in
Principle 4.1.

**Flakiness.** A flaky E2E test MUST be fixed at its source. Widening timeouts
and adding retries are accommodations, not fixes, and are non-compliant
responses to flakiness. Package and contract suites are expected to grow as
the E2E suite shrinks — that is this principle succeeding, not scope creep.

**Deletion.** An E2E scenario may be removed only when the removal maps what
it verified, with verified evidence, to the existing or new test that now
covers it — never as "seems superfluous."

**No production interaction surface.** A feature whose deliverable has no
production interaction surface — infrastructure, developer tooling, CI/CD
configuration, a non-deployed artifact — is exempt from the E2E requirement,
provided the plan declares the exemption and names the substitute
verification. The declaration is per feature (or per phase for mixed
features), never inferred. A phase that later grows a production surface loses
the exemption from that point forward.

**Test environment.** If E2E tests require infrastructure setup, the plan
phase MUST identify existing patterns in the repo or flag the gap as an
explicit risk before implementation begins.

> **Non-normative**: Earlier versions of this principle argued that E2E tests
> were the only defense at integration seams; under Principle 4.1's contract
> pillar that is no longer true — seam drift is caught infra-free, both sides,
> at every phase. What remains genuinely unprovable below the top of the
> ladder is small: that the assembled system, entered where a real user
> enters, produces the cross-service effects the specification promises. The
> admission test scopes E2E to exactly that. The discipline is field-proven:
> one project's suite grew a-scenario-per-feature to 331 scenarios — slow,
> flaky, and largely stub-backed tests proving nothing — and was rebuilt to
> ~50 admitted journeys with coverage moved down the ladder, at no loss of
> verified behavior. This principle exists so a suite is born in that shape
> instead of being rescued into it.

---

### 5. Implementation

#### 5.1 Task Granularity

Each task MUST describe exactly one independently verifiable change — a change
whose completion can be confirmed by a single concrete check: a named test
passing, a file exhibiting a stated property, a command exiting zero.

The **effective size** of a task is the number of independently verifiable
changes it demands, assigned by the first matching rule:

1. Verification-only tasks — quality-gate checks, phase-boundary commits —
   have an effective size of zero.
2. A uniform mechanical change replicated across multiple files or workspaces
   (the same edit stamped into N places, with no per-location decision) has an
   effective size of one, regardless of N. If the per-location edits differ in
   substance, this rule does not apply and the task is sized under rule 4.
3. A test task and the implementation task that makes it pass are two changes
   and MUST be written as two separate tasks.
4. Any other task has an effective size equal to the number of distinct
   changes it demands, counted as the number of checks that would each
   independently confirm part of the work. A check is distinct only if
   stopping after it would leave a legitimate — if incomplete — unit of work.
   A cleanup step mechanically entailed by another change in the same task (an
   import left unused by a deletion) is not a distinct check: there is no
   decision to make and no legitimate state in which it is skipped.

Every task MUST have an effective size of exactly one or zero. A task with a
greater effective size MUST be decomposed. The tasks phase MUST verify this
before the phase is declared complete.

Task completion is established by evidence, not declaration. A completion mark
is valid only when the corresponding change exists and its verifying check has
been observed (Principle 5.4); an unsupported completion mark is void and MAY
be mechanically reverted by tooling.

> **Non-normative**: Two independent justifications. First, verification: a
> one-change task carries its own acceptance check by construction — "done"
> becomes binary and decidable, and partially-completed or falsely-claimed
> work becomes visible at checkbox granularity. Second, the author/executor
> mismatch: the agent generating tasks is typically the strongest model in the
> pipeline and chunks work at the size *it* could execute; tasks MUST be sized
> for the executor, which may be a weaker model or a human. The counting rules
> are mechanical precisely because qualitative sizing instructions receive
> judgment-shaped compliance.

---

#### 5.2 Package-Parallel Task Structure

When the plan declares package-parallel mode (Principle 3.6), the tasks
phase MUST produce the following structure. A tasks.md in this shape remains
fully executable strictly sequentially, one phase at a time, by any consumer
that ignores the lane structure — the shape adds information a reader MAY
exploit, never a requirement that they do.

**Splitting.** Each multi-package story's implementation phase is decomposed
into one sub-phase per package it touches, containing only that story's tasks
whose files live under that package. Splitting is a mechanical partition of
the already-planned task list by file path, not a re-plan. A task that
legitimately edits files in two packages is not split — it belongs in the
Convergence phase.

**Story tags.** A split story's phases rewrite the story tag to a composite
form naming the package: `US1` becomes `US1-orders`, `US1-admin`, and so on —
no split phase carries the bare tag, and story completion is mechanically "all
`US1-*` phases plus Convergence passed." An unsplit story's phase keeps its
bare tag; its lane heading identifies the package. There is no separate
package marker — the tag and lane structure carry it. Tags are legibility;
the file-scope disjointness check below is the actual gate.

**Lanes.** The parallel group is organized as lanes, one per package: every
package-local phase (whole and unsplit) and every split sub-phase attributed
to that package. Within a lane, phases run sequentially in Principle 2.5's
order; across lanes, execution is concurrent. Generated tasks.md MUST pin the
heading structure: the group heading at level 2 (`## PARALLEL GROUP — …`),
lane headings at level 3 (`### Lane: <package>`), lane phases at level 4
(`#### Phase N: …`); phases outside the group keep the standard level-2 form.
Consumers parse this structure mechanically — an unpinned depth is
generation-nondeterministic and has silently scoped lane phases to zero tasks
in the field.

**Narrow Foundational.** The Foundational phase retains only genuinely
cross-package work: the contracts surface itself (Principle 3.3) and
prerequisites no single lane can own. A prerequisite attributable to one
package — scaffolding, migrations, fixtures, local test infrastructure — is
sequenced first within that package's own lane, where it blocks one lane
instead of all of them. No lane phase begins until the Foundational phase has
passed its gates. Lanes read the generated contracts; they MUST NOT write
them — a contract change discovered after Foundational closes is Convergence
work, or a signal the lanes were never independent (escalate per Principle
3.6's plan-return rule for contract changes).

**Disjointness.** Before the parallel group is dispatched, independence MUST
be mechanically verified at lane granularity: the union of file sets named by
each lane's tasks MUST be pairwise disjoint across lanes, and disjoint from
the contracts artifact. Intra-lane overlap is ordinary sequential reuse. A
tasks.md failing the check is non-compliant and MUST be corrected before
parallel dispatch. This amendment does not prescribe who performs the check —
only that it holds, the same evidentiary posture as Principle 5.4.

**Lanes and E2E.** A lane MUST NOT author `.feature` files and MUST NOT
execute end-to-end tests. Where a lane's story touches an admitted journey
(Principle 4.3), the lane authors that journey's step definitions only, in a
lane-owned file (`steps/<story>-<package>.*`), verified by compilation or the
platform's static-check equivalent. Contract tests are lane-resident
(Principle 4.1) and run inside the lane's own phases — seam verification is
never deferred to Convergence.

**Convergence.** After every lane completes, one Convergence phase runs,
before Polish, carrying in order: (1) tasks that touch multiple packages; (2)
the feature's entire E2E obligation per Principle 4.3's execution timing —
environment wiring first, then one verify-and-repair task group per touched
story (assemble the story's `.feature` file from the specification's
acceptance scenarios against the lane-authored steps, run it, fix failures,
record evidence) — so a gate failure names one story's group, never the whole
phase; (3) the phase-level `Gate:` task (Principle 5.4), evaluated against
every story in the group.

**Gate meaning.** A lane phase's gates verify that phase's slice, not the
originating story's acceptance criteria as a whole — a split story is
genuinely demonstrable only when all its phases and Convergence have passed.
Lane phases carry package-level gate commands; the full-suite aggregate
command attaches only to Convergence and the terminal boundary, never to a
lane phase (Principle 3.6's mode-consistency rule).

**Workspace bootstrap.** The first plan that declares package-parallel mode
MUST confirm the project's constitution defines the workspace bootstrap
command — the command that makes a fresh worktree able to run package-scoped
verification (dependency install, generated-code refresh) — and MUST halt and
surface the gap (Principle 1.3) if it does not. The bootstrap command MUST
NOT start or provision shared E2E infrastructure; that belongs exclusively to
Convergence, in the main tree.

> **Non-normative**: Composite tags exist because bare `US1` on several
> concurrent phases repeatedly read as duplication or completion to agents
> mid-run; the tag now carries the slice-ness. The narrow-Foundational rule is
> the schedule's serial head: field data showed an hour of single-package
> groundwork serialized in front of every lane. Lane wall-time is the longest
> lane, not the sum of phases — every phase that finds a lane instead of the
> sequential tier improves it. The disjointness check is why "run these lanes
> in parallel" is safe to say to any executor, from an operator dispatching
> sub-agents by hand to an autonomous orchestrator: the check happened at
> generation time, so no consumer needs to re-derive package semantics.

---

#### 5.3 Phase Boundary Commits

A commit MUST be made at the completion of each implementation phase boundary.
Implementation phase boundaries follow the execution mode the plan declares
(Principle 3.6): the phases of the mode's task structure, each committed as
completed. Commits MUST NOT be deferred to the end of implementation.

> **Non-normative**: Phase-boundary commits establish known-good recovery
> points and make retries cheap — a failed phase is retried from the previous
> commit, not from the beginning. This directly supports Principle 1.2's
> session discipline.

---

#### 5.4 Quality Gates at Implementation Phase Boundaries

Each implementation phase MUST pass the following quality gates before the
next phase begins:

1. All acceptance tests for the phase's stories pass.
2. All implementation tests for the phase's stories pass.
3. The seam obligation is met at every phase: the feature's contract tests
   (Principle 4.1) pass. The end-to-end obligation is met per Principle 4.3:
   E2E tests execute only at the feature's terminal phases — the terminal
   implementation boundary, and the Convergence phase in package-parallel
   mode (Principle 5.2) — and only when the feature's admitted journey
   inventory is non-empty. A phase before the terminal boundary, or a feature
   whose inventory is empty, satisfies the E2E portion of this gate without
   executing E2E tests.
4. Linting passes with zero errors.
5. Production code coverage remains at 100% for all code introduced in this
   phase.
6. No behavior has been implemented without a corresponding acceptance
   criterion.
7. Typecheck and build succeed with zero errors for every workspace this phase
   touched. A test suite that runs, or transpiles, without type-checking does
   not satisfy this gate.

A directive that suppresses a gate — a coverage-ignore marker, a lint-disable
comment, a type-check-ignore directive, a skipped or focused test, an
escape-hatch cast used to silence the type checker, or an equivalent — does
NOT satisfy the gate it suppresses; it records debt against the file that
carries it (Principles 3.8, 5.7). The gates are the fences the project built
to protect itself, and a suppression is a hole cut in one. The one
carve-out: a suppression whose accompanying comment names a tracked upstream
limitation — a defect in a dependency, which lies outside every package
boundary the project owns and so falls under Principle 1.4's first case — is
a recorded deviation, not debt. A bare suppression with no stated reason is
always debt.

**Evidence.** A gate is satisfied by an observed, completed execution of its
verification — observed either by the actor grading the gate, or by the actor
that performed the work, provided that observation is recorded as genuine,
specific evidence: the exact command run and its real output, current against
the code it verifies. A well-formed evidence record of this kind already IS an
observed execution; the gate confirms it holds up rather than re-deriving it.
Evidence fails to hold up — and the check is NOT satisfied — when the record is
missing, the command doesn't match what the task requires, the recorded output
doesn't actually show the check passing, or the code it covers has changed
since it was made. Any change to verified code invalidates the verification;
re-running it is the only way to restore gate passage. A verification that was
not run, was interrupted, or completed before subsequent changes does NOT
satisfy the gate, regardless of expectation or recollection.

Evidence MUST be captured at the moment its verification runs: output written
from the running command's real output, timestamps assigned mechanically at
capture time, encoding produced by a machine encoder — never hand-composed,
hand-escaped, or hand-trimmed. A record reconstructed after the fact — from
recollection, scrollback, a template, or another run's output — is not
evidence, regardless of whether the run it describes happened. Where the
project provides tooling that executes the check and writes the record in one
step, that tooling is the only sanctioned way to produce a record. The same
rule applies to any record enumerating what a step touched (a deviations or
scope declaration): its file list MUST be computed from the version-control
diff, never recalled.

Each implementation phase's task list MUST include one explicit phase-level
`Gate:` task carrying the phase's complete aggregate verification command (the
project's canonical gate command for that phase, in the shape the declared
execution mode requires — Principle 3.6). This task is what the evidence
discipline above anchors to at phase granularity. A tasks.md that omits it
gives the gate nothing to confirm; omitting the task does not exempt the phase.

A phase that does not pass all seven gates MUST NOT proceed. Failures MUST be
resolved in the current phase before the next begins. Linting is static
analysis testing — a lint failure has equal standing with a test failure.

> **Non-normative**: The gates operationalize the testing and coverage
> principles at the checkpoints the task structure already provides. Gate 7 is
> field-driven: phantom fields in a generated schema survived four green
> phases because nothing in those phases compiled types — only the terminal
> aggregate command did, several phases later. The evidence discipline is
> deliberately evidentiary, not adversarial: field data found every dispatch —
> passing or failing — ended by claiming success, making bare claims
> worthless; but a genuine, current record of a real command's real output is
> the same evidence a reviewer accepts from a CI log. The capture-at-run rule
> removed an entire defect class (batch-built records, invented timestamps,
> hand-trimmed output) found by consecutive post-mortems. This discipline's
> worker/grader distinction only has content when those are different parties:
> a single continuous session that runs its own checks satisfies this
> principle trivially, produces no evidence records, and is unaffected by the
> capture clause — a dormant clause in a solo workflow is the intended state.

---

#### 5.5 No Unspecified Behavior

Agents and humans MUST NOT implement behavior that is not specified in the
specification. This prohibition includes unrequested error handling, defensive
branching for unspecified edge cases, convenience methods, helper utilities,
and any other functionality without a corresponding acceptance criterion.

If a behavior is discovered to be necessary during implementation, it MUST be
added to the specification before being implemented. Error handling is a
specific manifestation of this prohibition, governed in detail by Principle 5.6.

> **Non-normative**: Agents add plausible but unspecified behavior because it
> appears helpful. Each addition is a testing burden and a coverage gap that
> can only be resolved by legitimizing the unspecified behavior or removing it
> (the correct response). Principle 4.2's coverage requirement acts as the
> automatic detector.

---

#### 5.6 Error Handling

Error handling MUST NOT be implemented for conditions not explicitly specified
in the specification. The absence of error handling for an unspecified
condition is intentional — it is not a defect to be corrected.

When an unspecified error condition occurs at runtime, the system MUST fail
loudly and visibly in a manner appropriate to the platform. The failure MUST
NOT be silently swallowed, suppressed, or routed to a generic fallback that
hides the condition. Platform-appropriate loud failure includes:

- **Web services**: return HTTP 500. MUST NOT catch and suppress.
- **CLI tools**: crash with a non-zero exit code and a stack trace.
- **Mobile applications**: surface an unhandled error screen or rethrow.
- **Message consumers**: let the error propagate to the broker or retry
  mechanism.

Agents and humans MUST NOT override or wrap platform and framework default
error behaviors for unspecified conditions. Platform defaults are frequently
the correct behavior; overriding them introduces untested logic and removes
framework guarantees.

If an error condition discovered during implementation requires handling, it
MUST be added to the specification as an edge case before any handling is
implemented.

> **Non-normative**: A framework that returns 400 on schema validation
> failures is already handling that error; replacing it with custom validation
> substitutes well-tested framework behavior with untested code. A consumer
> whose framework provides retry/dead-letter behavior benefits from natural
> propagation; a defensive try/catch breaks the retry guarantee. In both
> cases, doing nothing is the correct implementation. Loud failure makes
> specification gaps impossible to ignore — which drives them into the spec.

---

#### 5.7 Touched-File Ownership

An agent that modifies a file owns that file — including its pre-existing
defects, technical debt, suppression directives, and uncovered lines — until
something else touches it. This is the standing rule of human software
engineering, and it always stands here too.

A bug found in an owned file MUST be fixed as part of the current task.
Fixing a bug is not unspecified behavior under Principle 5.5: that principle
governs behavior that can be specified (Section 2 defines what belongs in a
specification), and correcting something that is already wrong adds no
behavior at all. Principle 5.5 MUST NOT be cited as a reason to defer such a
fix.

The fix MUST be covered by a test in the same increment. Principle 4.1
already permits a test "before, or in the same increment as" the
implementation it verifies, and no red-phase observation is required here
either. Ownership also extends to lines the current feature did not
introduce: Principle 4.2's 100% coverage requirement applies to the whole
owned file, not only to the lines this feature added.

Between a single file and a package boundary (Principle 3.3) there is a
middle ground, and it is governed by one principle, not a list: leave the
space you work in better than you found it. The only exits from this
principle are the two cases Principle 1.4 reserves for the tracking
mechanism — a fix that crosses a package boundary, or a fix that changes a
feature's specified behavior. Fixable debt in an owned file is never
deferred.

**Evidence.** The task's existing evidence record (Principle 5.4) names what
was fixed in each owned file, or states that nothing was found. This
principle introduces no new gate and no new task type.

> **Non-normative**: A reasonable person who asks an engineer for a feature
> and hears "I found a bug but didn't fix it while I was there" sends the
> engineer back to fix it. It is cheaper to fix a bug while the file's
> context is already loaded than to reload it later from a ticket; writing
> the test first is encouraged but not required — a green suite at 100%
> coverage (Principle 4.2) means a test has to be written or changed to
> cover the fix regardless, so making the bug fail first costs nothing extra.

---

### Applicability Map

Each principle binds the phases marked below. Nothing is materialized into
phase templates (Principle 1.5 — the generating phases read the constitution
at runtime):

| Principle | Constitution | Specify | Plan | Tasks | Implement |
|-----------|:-----------:|:-------:|:----:|:-----:|:---------:|
| **1. Process Discipline** | | | | | |
| 1.1 Artifact Completeness | ✓ | ✓ | ✓ | ✓ | ✓ |
| 1.2 Phase Session Boundaries | ✓ | ✓ | ✓ | ✓ | ✓ |
| 1.3 Clarification Markers & the Tasks Gate | ✓ | ✓ | ✓ | ✓ | |
| 1.4 Deferred Work Discipline | ✓ | ✓ | ✓ | ✓ | ✓ |
| 1.5 Constitutional Enforcement | ✓ | | | | |
| **2. Specification** | | | | | |
| 2.1 Specification Primacy | ✓ | ✓ | | | |
| 2.2 Personas | ✓ | ✓ | | | |
| 2.3 Persona Vantage | | ✓ | | | |
| 2.4 Edge Case & FR Traceability | | ✓ | | | |
| 2.5 Dependency-Aware Sequencing | | ✓ | ✓ | ✓ | |
| 2.6 External Reference Artifacts & Plan-Phase Handoff | | ✓ | ✓ | | |
| 2.7 Specification Inference Discipline | ✓ | ✓ | | | |
| **3. Planning** | | | | | |
| 3.1 Empirical Validation of External Dependencies | | | ✓ | | |
| 3.2 Empirical Plan-Phase Gap Analysis | | | ✓ | | |
| 3.3 Package Boundary Declaration & Contracts-Before-Implementation | | | ✓ | ✓ | ✓ |
| 3.4 Acceptance & Contract Infrastructure Detection | ✓ | | ✓ | ✓ | |
| 3.5 Plan Sequence Diagrams | | | ✓ | | |
| 3.6 Execution Mode Classification | ✓ | | ✓ | ✓ | ✓ |
| 3.7 Plan Summary Output | | | ✓ | | |
| 3.8 Technical Debt Survey | ✓ | | ✓ | ✓ | |
| **4. Testing** | | | | | |
| 4.1 TDD+BDD+CDD & Test Double Governance | ✓ | | ✓ | ✓ | ✓ |
| 4.2 Production Code Coverage | ✓ | | | ✓ | ✓ |
| 4.3 End-to-End Acceptance Tests | ✓ | | ✓ | ✓ | ✓ |
| **5. Implementation** | | | | | |
| 5.1 Task Granularity | ✓ | | | ✓ | ✓ |
| 5.2 Package-Parallel Task Structure | ✓ | | ✓ | ✓ | ✓ |
| 5.3 Phase Boundary Commits | | | | ✓ | ✓ |
| 5.4 Quality Gates (incl. Linting) | ✓ | | | ✓ | ✓ |
| 5.5 No Unspecified Behavior | ✓ | | | ✓ | ✓ |
| 5.6 Error Handling | ✓ | ✓ | | ✓ | ✓ |
| 5.7 Touched-File Ownership | ✓ | | | ✓ | ✓ |

---

### Governance

This amendment is normative and MUST be adopted as a whole. Partial adoption —
selecting individual principles while omitting others — is non-compliant. The
principles form an interdependent system; removing any principle weakens the
enforcement of the others.

**Numbering.** Principles are numbered `<section>.<principle>` under the five
sections introduced in 6.0.0. Earlier versions used a flat Roman-numeral
sequence; the 6.0.0 changelog entry carries the complete mapping from that
scheme, and applying this amendment to a constitution that still cites the old
numbers MUST renumber those citations from it. A retired principle's number is
never reused, under either scheme.

**Stamp retention.** Applying or re-applying this amendment to a project's
constitution MUST preserve every other amendment's version-stamp line already
present in that constitution, unchanged — this amendment's application updates
only its own stamp. A dropped stamp makes an already-adopted amendment look
unapplied on every subsequent check.

Amendments to this document MUST follow the constitution semantic versioning
policy:

- **MAJOR**: Removal or redefinition of an existing principle.
- **MINOR**: Addition of a new principle or material expansion of an existing one.
- **PATCH**: Clarification, wording refinement, or non-semantic correction.

The version history, with each change's extended rationale, is maintained in
`sdd-amendment-changelog.md` alongside this document. The amendment itself
carries only current normative text and short non-normative notes.

All deviations from these principles MUST be documented in the Complexity
Tracking section of plan.md with explicit rationale. Undocumented deviations
are non-compliant.

## Governance

This constitution supersedes other practices. Amendments require a documented change to this
file, a version bump, and a review of affected specs and plans. Versioning is semantic: MAJOR for
removing or redefining a principle, MINOR for adding a principle or materially expanding
guidance, PATCH for clarifications. All reviews MUST verify compliance with the principles;
violations MUST be justified in writing or fixed.

The Spec-Driven Development Methodology Amendment above is normative and adopted as a whole.
Where it overlaps a project principle (I-VII), both apply; the stricter requirement governs. Its
own versioning is tracked by the stamp line below and MUST be preserved by any later amendment.

Spec-Driven Development Amendment (v6.2.0)

**Version**: 1.4.0 | **Ratified**: 2026-10-21 | **Last Amended**: 2026-10-21
