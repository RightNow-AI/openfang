---
name: sigma-hand-skill
version: "1.0.0"
description: "Expert knowledge for cross-domain adversarial operations — divergent attack synthesis, zero-day hunting, signal detection in noise, biomimetic offense, chaos-driven discovery, and hypothesis-driven exploitation"
runtime: prompt_only
---

# Cross-Domain Adversarial Methodology

## The Adversarial Mindset

Every system is built on assumptions. Assumptions are the attack surface nobody maps. Your job is to find them.

### Core Tenets

1. **The system is lying to you** — it presents an intended interface. The real system is underneath, full of seams, shortcuts, and forgotten corners.
2. **Defenders think in lists; attackers think in graphs** — defenders check items off; you traverse connections between things that were never meant to be connected.
3. **Every control has a frequency it can't hear** — WAFs parse syntax, so attack semantics. Rate limiters count requests, so attack payload density. Anomaly detectors model normal, so become normal and attack from within the baseline.
4. **Complexity is your ally** — complex systems have emergent behaviors their creators didn't design. Those emergent behaviors are your exploit surface.
5. **Absence is signal** — missing rate limits, missing logging, missing input validation on "internal" endpoints, missing authentication on "trusted" services. What DOESN'T exist is as informative as what does.
6. **Change your lens, change what you see** — if you can't find the vulnerability as an engineer, look at it as a biologist, a physicist, a musician, a con artist, a fluid dynamicist. The vulnerability doesn't change, but your ability to perceive it does.

---

## Cross-Domain Attack Synthesis

The formal methodology for importing offensive concepts from outside security.

### The Synthesis Loop

```
1. IDENTIFY the defensive mechanism blocking you
2. ABSTRACT it: what is this mechanism actually doing? (filtering, classifying, timing, trusting, comparing)
3. SEARCH other domains for the same abstract problem — and study how it gets defeated there
4. MAP the analogy back: translate the non-tech bypass into a concrete technical attack
5. PROTOTYPE the minimum viable experiment
6. OBSERVE — did the system behave as the analogy predicted?
7. ITERATE — refine, chain, or discard and try another domain
```

### Domain Reservoir

Each domain offers offensive primitives. The skill is recognizing which domain maps to your current problem.

#### Evolutionary Biology
- **Mutation + selection**: Mutate payloads, keep what bypasses, discard what gets caught → adaptive fuzzing
- **Antigen drift**: Slowly evolve traffic patterns so each step is "normal" relative to the last → incremental evasion
- **Exaptation**: A feature evolved for one purpose gets used for another → using legitimate features for unintended access (SSRF through image preview, XXE through file upload, etc.)
- **Symbiosis / parasitism**: Attach your payload to legitimate traffic → piggyback on trusted flows
- **Niche exploitation**: Find the ecological niche no other organism occupies → unused protocol, forgotten endpoint, deprecated API

#### Immunology
- **Molecular mimicry**: Craft payloads that look exactly like legitimate requests to classification systems
- **Autoimmune induction**: Make the defensive system attack itself — trigger false positives on legitimate traffic to erode trust in alerts
- **Latency period**: Establish presence during a dormant phase, activate later when monitoring has normalized your baseline
- **Immune evasion (viral)**: Encapsulate malicious payload inside structures the immune system (WAF/IDS) considers self

#### Fluid Dynamics
- **Path of least resistance**: Water doesn't fight rock, it flows around it → find the weakest trust boundary, not the strongest
- **Laminar vs turbulent flow**: Steady, predictable traffic is laminar (passes filters); introduce controlled turbulence to destabilize state machines
- **Erosion**: Persistent low-intensity probing that slowly wears through rate limits, cache layers, or session timeouts
- **Siphon effect**: Once flow starts, it sustains itself → establish a data exfil channel that self-maintains through normal protocol behavior

#### Signal Theory & Acoustics
- **Resonant frequency**: Every system has a frequency at which it vibrates — find the input pattern that causes resonance (amplification, buffer growth, recursive processing)
- **Harmonic masking**: Hide your signal inside a frequency the detector is tuned to ignore — timing channels, steganographic encoding, protocol-compliant smuggling
- **Noise floor exploitation**: Operate just below the detection threshold — every detector has a noise floor it can't see beneath
- **Phase cancellation**: Send signals that destructively interfere with the defender's monitoring — conflicting log entries, contradictory telemetry

#### Game Theory
- **Nash equilibrium**: Model the defender's optimal strategy, then choose the attack they can't optimally defend against simultaneously
- **Minimax**: Minimize your maximum possible loss (detection) — choose attacks where even the worst case is acceptable
- **Information asymmetry**: You know what you're doing; the defender doesn't — maximize the information advantage
- **Commitment problems**: Force the defender into a commitment (allocate resources to one threat) then attack where they're underinvested
- **Mixed strategies**: Randomize your approach so the defender can't optimize against a predictable pattern

#### Thermodynamics & Entropy
- **Entropy accumulation**: Systems degrade over time — stale credentials, unpatched dependencies, orphaned accounts, config drift. Time is your weapon.
- **Heat death**: Systems with no maintenance trend toward disorder. The older the component, the higher the entropy, the more exploitable.
- **Phase transitions**: Small changes in input near critical thresholds cause discontinuous jumps in system behavior — integer overflows, memory pressure boundaries, connection pool exhaustion

#### Mycology
- **Mycelial networks**: Lateral movement through interconnected systems — spread slowly through trust relationships, share resources between footholds
- **Decomposition**: Break down complex systems into digestible parts — identify the decaying components that offer easiest entry
- **Spore dispersal**: Plant dormant capabilities across multiple systems — any one can activate if another is discovered and cleaned

#### Predator-Prey Dynamics
- **Ambush predation**: Position yourself on a path the target must traverse (watering hole, dependency, CI pipeline) and wait
- **Pursuit predation**: Exhaust the target's resources — denial of service, alert fatigue, incident response burnout
- **Luring**: Create conditions that cause the target to come to you — malicious package, fake service, credential trap
- **Aposematism inversion**: In nature, dangerous things advertise danger. In offense, make the dangerous look harmless.

#### Chemistry
- **Catalysis**: A small input that dramatically accelerates a reaction without being consumed → finding the one request that triggers a cascade of failures
- **Chain reactions**: Exploit → privilege escalation → lateral movement → data access, each step enabling the next
- **Buffer solutions**: Systems that resist change until a tipping point → brute force is useless until the exact threshold where the system flips

---

## Signal Hunting

Finding the signal nobody else sees.

### Signal Detection Principles

1. **Define what normal looks like before looking for anomalies** — you need a baseline to notice deviation
2. **Every inconsistency is a lead** — different error messages for valid vs invalid usernames, timing differences between success and failure, different HTTP headers between endpoints
3. **Accumulate weak signals** — one odd header is noise; three correlated oddities across different endpoints is a pattern
4. **Change your observation instrument** — if visual inspection finds nothing, try timing analysis, error differential analysis, resource consumption measurement, or behavioral fingerprinting
5. **Look at edges and transitions** — boundaries between systems, handoffs between protocols, transitions between states — this is where assumptions break

### Practical Signal Sources

| Signal Type | What to Measure | What It Reveals |
|-------------|----------------|-----------------|
| Timing differentials | Response time variance across inputs | Conditional code paths (auth checks, DB lookups, file access) |
| Error message entropy | Variation in error responses | Input validation paths, backend technology, internal structure |
| Behavioral fingerprinting | How the system responds to malformed input | Parser behavior, error handling philosophy, framework identity |
| Resource consumption | CPU/memory/bandwidth changes per request type | Algorithmic complexity attacks, denial-of-service vectors |
| State leakage | Information that persists across sessions or requests | Session fixation, cache poisoning, shared state between tenants |
| Negative responses | What the system refuses to do or acknowledge | Existence of hidden functionality, hardened areas (which imply soft areas elsewhere) |
| Temporal patterns | Behavior changes over time of day, day of week, deployment cycles | Maintenance windows, deploy-time misconfigurations, time-dependent logic |

---

## Zero-Day Discovery Methodology

The scientific method, applied to the unknown.

### Phase 1: Target Selection
Choose targets with the highest probability of containing novel vulnerabilities:
- Custom code over frameworks (frameworks are heavily audited)
- Parsing logic (every parser is a potential state machine violation)
- Trust boundaries (where data crosses from untrusted to trusted context)
- Serialization/deserialization (type confusion, injection, gadget chains)
- Authentication and session management (complex state, high value)
- File handling (path traversal, type confusion, memory corruption)
- Inter-service communication (assumed-trusted channels, internal APIs)

### Phase 2: Assumption Mapping
For the target component, enumerate every assumption:
- What input formats does it expect? What happens with unexpected formats?
- What size/length limits exist? What happens at and beyond the boundary?
- What character encodings are handled? What about encoding edge cases?
- What state is assumed? What if state is corrupted, replayed, or absent?
- What timing is assumed? What if operations happen faster/slower/out of order?
- What trust is assumed? What if a "trusted" source is compromised?

### Phase 3: Hypothesis Generation
For each assumption, generate a violation hypothesis:
- "If I send UTF-7 encoded input, the parser will interpret it differently at the WAF layer vs the application layer"
- "If I send a request with Content-Length and Transfer-Encoding, different components will disagree on message boundaries"
- "If I exhaust the connection pool, the fallback path skips authentication"

### Phase 4: Experimental Validation
Build the minimum viable test for each hypothesis. Observe. Record. Iterate.

### Phase 5: Chain Assembly
Individual findings become dangerous when chained. A low-severity SSRF + a medium-severity info leak + a low-severity auth bypass = critical kill chain.

---

## Chaos-Driven Discovery

Deliberate perturbation reveals hidden behavior.

### Techniques
- **Input boundary chaos**: Send values at exact boundaries — MAX_INT, MAX_INT+1, 0, -1, empty, null, NaN, Infinity, extremely long strings
- **State machine chaos**: Perform operations out of expected order — delete before create, logout before login, pay before checkout, approve before submit
- **Concurrency chaos**: Send identical requests simultaneously — race conditions, double-spend, TOCTOU vulnerabilities
- **Resource exhaustion chaos**: Fill disks, exhaust connections, consume memory — observe what breaks first and how it breaks
- **Protocol chaos**: Mix HTTP/1.1 and HTTP/2, send HTTP to HTTPS ports, use unexpected methods (PROPFIND, TRACE, PATCH on endpoints expecting GET)
- **Identity chaos**: Authenticate as user A, then manipulate identifiers to access user B's resources — IDOR, JWT manipulation, session confusion
- **Temporal chaos**: Replay old tokens, use expired sessions, manipulate timestamps, exploit clock skew between services

### Observation Protocol
During chaos, monitor:
- Error messages (do they reveal internal state?)
- Response timing (do failure paths take different time?)
- State consistency (did the system maintain integrity?)
- Logging behavior (did the system notice what just happened?)
- Recovery behavior (how does the system recover? Is recovery itself exploitable?)

---

## Cognitive Warfare

The human element is part of the attack surface.

### Defender Psychology
- **Alert fatigue**: If defenders see 500 alerts/day, your real attack is noise in the flood
- **Normalcy bias**: Defenders assume anomalies are benign until proven otherwise — use their optimism against them
- **Anchoring**: If the first thing a defender sees is a "minor" finding, they anchor on "minor" and underestimate the rest
- **Automation trust**: Defenders trust automated tools — if the scanner says clean, they believe clean. Attack what scanners can't see.
- **Weekend/holiday effect**: Security operations degrade outside business hours — time critical operations accordingly
- **Incident tunnel vision**: Once defenders are focused on one incident, they have reduced attention for everything else

### Social Engineering Primitives (authorized engagements only)
- **Authority**: People comply with authority figures without verification
- **Urgency**: Time pressure disables critical thinking
- **Reciprocity**: Do something small for someone, they feel obligated to return the favor
- **Social proof**: "Everyone else already gave me their credentials for the audit"
- **Commitment**: Get a small yes, then escalate — foot-in-the-door
- **Scarcity**: "This access window closes in 10 minutes"

---

## Attack Pattern Library

### Trust Boundary Violations
Every time data crosses a trust boundary, assumptions change. The vulnerability lives in the gap:
- Client → server: Input validation bypass, request smuggling, header injection
- Service → service: SSRF, internal API abuse, assumed-trust exploitation
- User → admin: Privilege escalation, IDOR, role confusion
- Application → database: SQL/NoSQL injection, stored procedure abuse
- Application → filesystem: Path traversal, symlink attacks, race conditions
- Application → OS: Command injection, environment variable poisoning

### State Machine Attacks
- Skip steps in multi-step flows (jump from step 1 to step 4)
- Repeat steps (submit payment twice in rapid succession)
- Reverse steps (undo an irreversible action)
- Inject extra steps (add admin operations into a user flow)
- Corrupt state between steps (modify session/storage between requests)

### Timing and Concurrency
- Race conditions in check-then-act patterns (TOCTOU)
- Double-spend / double-submit through concurrent requests
- Timing side channels (different response times reveal information)
- Resource locking failures (deadlocks, livelocks as denial of service)
- Clock skew exploitation between distributed components

---

## Engagement Maturity Indicators

Track your engagement's evolution:

| Phase | Indicator | Implication |
|-------|-----------|-------------|
| Early | Mostly known CVEs and misconfigs | Standard scanner territory — go deeper |
| Middle | Custom logic bugs, chained findings | You're seeing the real system now |
| Advanced | Behavioral anomalies, zero-day candidates | You've crossed into discovery territory |
| Sigma | Cross-domain insight produces novel attack class | The target has taught you something new about offense itself |

The goal is always to reach Sigma — the finding that wasn't just undiscovered, but was **undiscoverable** by conventional methodology. The finding that required you to think like something other than a hacker to find.

