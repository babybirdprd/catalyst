# **The Brownfield Protocol: Integrating Legacy Systems into Catalyst**

### **I. The Philosophy: "The Hostile Takeover"**

When integrating an existing "Brownfield" project (e.g., an existing Rust backend or React frontend), Catalyst does not ask for permission. It assumes the role of **Senior Architect**.

It performs a **Reverse-Compilation**:

1. **Ingest:** It reads the raw code (AST).  
2. **Understand:** It derives the implied spec.md that *would* have generated this code.  
3. **Align:** It proposes a refactor to fit the "One Brain, Two Bodies" workspace structure.

### **II. Phase 1: The Deep Scan (Ingestion)**

The user runs: catalyst adopt ./my-legacy-project

1\. The AST Map (Tree-Sitter)  
Catalyst uses tree-sitter-rust and tree-sitter-typescript to build a semantic map of the project. It identifies:

* **Structs & Types:** The data model.  
* **Public API Surface:** What is exposed via Axum/Actix/Tauri?  
* **Dependency Graph:** Which crates are actually used vs. dead weight?

2\. The "Unknowns" Inversion  
In a new project, we list "Unknowns" to solve. In a Brownfield project, we list "Inferred Knowns" to verify.

* *Inference:* "I see sqlx and postgres strings. You are using PostgreSQL."

### **III. Phase 2: The Harness Retrofit (Reverse Engineering)**

1\. Generating spec.md (The "As-Built" Documentation)  
The Swarm (Architect Agent) writes the Spec based on the code.

* *Section 1:* "Current Architecture" (inferred from code).  
* *Section 2:* "Tech Stack" (locked to current versions).

**2\. Generating state.json**

* It marks all detected features as "Completed."  
* It identifies "TODO" comments in the code and promotes them to "Pending Tasks".

3\. The "Gap Analysis"  
The Critic Agent compares the Inferred Spec against Best Practices (The Fortress Standard).

* *Flag:* "You are using unwrap() in 45 places. This violates Fortress Mode constraints."

### **IV. Phase 3: The Structure Migration (The Implant)**

To gain the "Self-Building" capability, the legacy project must be "infected" with the Catalyst Runtime.

1\. The Workspace Wrapper  
Catalyst creates a Cargo.toml workspace at the root, wrapping the existing code.

* Moves existing backend code \-\> crates/core (or crates/legacy\_backend).  
* Moves existing frontend code \-\> crates/frontend.

2\. The Runtime Injection  
It adds radkit as a dependency to the backend crate and generates a src/bin/catalyst\_agent.rs entry point.

### **V. Complex Addendum: Migration & Modularity ("The Strangler & The Atomizer")**

#### **A. The "Strangler" Engine (TS \-\> Rust Backend Migration)**

We apply the **Strangler Fig Pattern** to migrate non-Rust (Next.js/TS) backends.

1. **The Dissection:** Scan Next.js API routes (pages/api/\*, app/api/\*).  
2. **The Crate Scavenger:** Hunt for Rust equivalents (e.g., zod \-\> validator, prisma \-\> sqlx).  
3. **The Missing Link Generator:** If logic is unique, generate a **New Local Crate** (e.g., crates/tax\_engine) instead of polluting main.rs.  
4. **The Switch:** Update crates/server (Axum) to intercept requests and route them to the new Rust Core.

#### **B. The "Atomizer" (Strict Modularity Enforcement)**

Catalyst enforces **"Agent-Sized" Code** to prevent context window saturation.

**The Rule of 100 (The Constraint):**

* **Hard Constraint:** No source file in crates/core may exceed 150 lines.  
* **Function Constraint:** No single function may exceed 30 lines.

**The Refactor Loop:**

1. **Bloat Scanner:** Flags user\_controller.ts (800 lines).  
2. **Fission Process:** The Atomizer Agent breaks it into 5 distinct modules (Auth, Profile, Storage, etc.).  
3. **Facade Pattern:** Creates a mod.rs to re-export functions, maintaining a clean API surface.

### **VI. Summary of the Brownfield Workflow**

| Step | Action | Catalyst Role |
| :---- | :---- | :---- |
| **1\. Adopt** | catalyst adopt ./project | **Auditor:** Scans AST, builds dependency graph. |
| **2\. Document** | Generates spec.md | **Historian:** Reverse-engineers the "Why" from the "What." |
| **3\. Implant** | Wraps in Workspace, adds radkit | **Surgeon:** Injects the AI runtime into the host. |
| **4\. Fix** | "Rescue Mode" Refactoring | **Mechanic:** Fixes compilation errors from the move. |
| **5\. Evolve** | User asks for new feature | **Architect:** Now behaves exactly like a Greenfield project. |

