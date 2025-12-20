# **The Catalyst Manifesto**

### **An Agent-First Architecture for Recursive Intelligence**

#### **I. The Philosophy: Breaking Skeuomorphism**

Current AI development tools suffer from a critical design flaw: **Skeuomorphism**. They mimic human workflows (Agile, Jira, Ticket Tracking) designed for single-threaded biological brains with limited short-term memory.

* **The Human Limitation:** Humans must "lazy load" context. We code until we hit a blocker, stop, research, and resume. We cannot hold the entire dependency graph in our heads.  
* **The Agent Reality:** Agents are massively parallel but suffer from "context drift." They excel at "eager loading" information (researching 10 paths simultaneously) but fail at maintaining long-term architectural coherence.

The Thesis:  
To achieve true productivity, we must shift from Iterative Discovery (figuring it out as we go) to Recursive Resolution (solving the entire graph before writing code). We treat the Agent not as a junior developer, but as a Compiler.

#### **II. The Core Mechanism: "Compile-Time Intelligence"**

In software, a compiler resolves types, dependencies, and memory safety *before* the program runs. If the compiler succeeds, the runtime is stable. We apply this strictness to intelligence:

1. **Compile-Time (The Harness):** The act of resolving ambiguity, selecting libraries, and defining schemas. This happens *before* a single line of feature code is written.  
2. **Runtime (The Execution):** The act of generating syntax. This is performed by the **Builder Agent**.

The Shift:  
Instead of an agent stopping mid-task to ask "Which library should I use?", that ambiguity is treated as a Compile Error. The system must resolve the "Unknown" via a swarm before the coding agent is ever invoked.

#### **III. The Architecture: Opinionated Reliability**

We reject the "use any language" approach. To guarantee stability, the Harness must be rigid so the Agent can be flexible.

**1\. The Stack**

* **Language:** **Rust** (Non-negotiable). Rust's strict compiler prevents agents from "lying" about types or methods. If it builds, it is mathematically likely to work.  
* **Frontend:** **React \+ SWC**. Instant compilation, standard ecosystem.  
* **Runtime:** **Radkit**. The Local, Rust-native agent orchestration engine embedded directly in the application.

2\. The Structure: "One Brain, Two Bodies"  
We utilize a single Cargo Workspace to support both Web and Desktop targets without code duplication.

* **crates/core (The Brain):** Contains the **Radkit Swarm**, state management, git logic, and business rules. It is interface-agnostic.  
* **crates/frontend (The Face):** The React UI, compiled via SWC.  
* **crates/server (Body A):** An Axum server that embeds the frontend. Runs as a lightweight CLI/Container.  
* **crates/desktop (Body B):** A Tauri shell that wraps the same core. Adds native OS integration only when needed.

3\. The Execution Engine (Pluggable Intelligence)  
The "Builder" role is a slot, not a hardcoded dependency.

* **Default:** **Local Radkit Agents** (running open weights or API models).  
* **Optional:** **Jules** (Google's Async Agent) for massive refactors.  
* **Optional:** **Claude/GPT** via API.

#### **IV. The Operational Modes: The "Tri-Mode" Logic**

Users do not always need enterprise rigor. Catalyst offers a "Depth Slider" to balance speed vs. safety.

**Mode 1: The Speed Run (Prototype)**

* **Philosophy:** "Move Fast and Break Things."  
* **The Swarm:** Single Agent (**The Hacker**).  
* **Logic:** Unknowns Parser is disabled. Constraints are loose (unwrap() allowed).  
* **Result:** Instant code generation. Good for hackathons and visual tests.

**Mode 2: The Lab (Pre-Production)**

* **Philosophy:** "Measure Twice, Cut Once."  
* **The Swarm:** Two Agents (**Architect** \+ **Builder**).  
* **Logic:** Unknowns Parser active. Standard linting enabled. Selects only stable, standard libraries.  
* **Result:** Clean, componentized code suitable for MVPs.

**Mode 3: The Fortress (Enterprise)**

* **Philosophy:** "Zero Trust. Zero Defects."  
* **The Swarm:** The Full Council (**Researcher**, **Architect**, **Critic**, **Red Team**, **Auditor**).  
* **Logic:** Aggressive Unknowns Parser. Adversarial Testing Loop active (Red Team writes hostile tests *before* code generation).  
* **Result:** Bulletproof, secure, formally specified code.

#### **V. The Inception Strategy: Recursive Development**

**The Catalyst Template is not a skeleton; it is an embryo.**

* **Self-Contained Intelligence:** Every catalyst-core project contains the full radkit binaries required to modify itself. You do not need external tools to maintain the project. The project contains its own developer.  
* **The Bootstrap Loop:**  
  1. User clones catalyst-core.  
  2. User runs cargo run.  
  3. The embedded Radkit Agent wakes up: *"I am Catalyst. What are we mutating into today?"*  
  4. The Agent rewrites its own Cargo.toml and source code to evolve into the target application (e.g., a Crypto Vault).

#### **VI. The Circular Lifecycle (The Agent SDLC)**

We replace the linear "Plan \-\> Code" workflow with a self-correcting loop managed entirely by the local Radkit runtime.

1. **Input:** User Goal ("Build a Stock Visualizer").  
2. **Resolution (Compile-Time):** Radkit Swarm resolves "Unknowns" (APIs, Libs) \-\> Generates **Frozen Context** (Spec).  
3. **Build (Runtime):** The Builder Agent (Radkit/Jules) executes the plan deterministically.  
4. **Verify (Adversarial):** Red Team runs hostile tests. If fail \-\> Return to Step 3\.  
5. **Observe (Watchtower):** Agent monitors stderr logs in production.  
6. **Evolve:** Runtime errors are parsed into new **Constraints** injected back into the Harness (Step 2).

#### **VII. The Blueprint: Repository Structure**

The catalyst-core repository is structured to support this self-building, dual-target architecture immediately.

catalyst-core/  
├── Cargo.toml          \# Workspace Definition  
├── package.json        \# Frontend Dependencies  
├── harness/            \# The "Self-Hosted Brain"  
│   ├── spec.md         \# The Frozen Context  
│   ├── state.json      \# The Project State  
│   ├── prompts/        \# System Prompts for local Radkit agents  
│   └── knowledge/      \# Local RAG Data (Docs/Notes)  
└── crates/  
    ├── core/           \# \[LIB\] Business Logic, Radkit Runtime, Git Wrapper  
    ├── frontend/       \# \[APP\] React \+ TypeScript \+ SWC \+ Shadcn  
    ├── server/         \# \[BIN\] Axum \+ rust-embed (Default Target)  
    └── desktop/        \# \[BIN\] Tauri Shell (Optional Plugin)

The Mandate:  
We are not managing tasks. We are orchestrating intelligence. We build the Harness so the Agent can fly.