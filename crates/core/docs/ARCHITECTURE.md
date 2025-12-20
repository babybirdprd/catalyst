# Catalyst Core Architecture

## Overview

The Catalyst Core (`catalyst_core`) functions as the central nervous system of the application. It employs a **Swarm Architecture** where specialized agents collaborate to solve complex programming tasks. The core is responsible for orchestration, state management, and providing deterministic tools to agents.

## The Coordinator

The `Coordinator` is the primary entry point for the system. It manages the lifecycle of a user request ("Goal") and orchestrates the movement of data between agents.

### Responsibilities
1.  **Event Loop**: Emits `SwarmEvent`s for every major action (Agent Started, Agent Completed, Decision Made).
2.  **Pipeline Management**: Advances the state machine through the defined stages.
3.  **Human-in-the-Loop**: Pauses execution to request human approval for critical decisions (Architect choices, Critic rejections).
4.  **A2A Dispatch**: Manages the Agent-to-Agent (A2A) bridge for asynchronous tasks like parallel research.

## The Pipeline

The `Pipeline` is a state machine that enforces the order of operations. It ensures that planning happens before coding ("Compile-Time Intelligence").

### Stages

1.  **UnknownsParsing**: The `Unknowns Parser` analyzes the user goal to identify ambiguities.
2.  **Researching**: The `Researcher` (and `WebScraper`) investigate the unknowns. This can happen in parallel.
3.  **Architecting**: The `Architect` proposes a solution based on the research.
4.  **Critiquing**: The `Critic` reviews the proposal.
    *   *Pass*: Advance to `Atomizing`.
    *   *Reject*: Loop back to `Architecting` (up to `max_rejections` times).
5.  **Atomizing**: The `Atomizer` breaks the feature into small, agent-sized modules.
6.  **TaskGeneration**: The `Taskmaster` creates detailed "Mission Prompts" for the builder.
7.  **Execution (Implicit)**: The `Builder` agent executes the missions in isolated worktrees.

## Agent-to-Agent (A2A) Bridge

Catalyst supports asynchronous agent communication via the A2A Bridge. This allows the Coordinator to "fire and forget" missions to background agents.

*   **Pattern**: Dispatch -> Await Result (or stream progress).
*   **Usage**: Currently used for the `Researcher` to perform parallel investigations of multiple unknowns.
*   **Events**: The bridge emits `ResearchProgress`, `ResearchCompleted`, and `AgentFailed` events which are bubbled up by the Coordinator.

## "Brain Dump" vs "Reactor"

The system supports two primary modes of interaction:

*   **Brain Dump**: The input phase where the user provides the goal, context, and answers to the `Unknowns Parser`.
*   **Reactor**: The visualization of the pipeline execution. The UI subscribes to the stream of `SwarmEvent`s to show real-time progress, currently processing agents, and decisions.

## Concurrency Model

The Coordinator supports parallel execution of features:
1.  **Planning**: Often sequential per feature to ensure coherence.
2.  **Drafting (Speed Demon)**: The `DraftingSkill` uses a "Scatter-Gather" pattern to generate code for multiple files in parallel.
3.  **Building**: The `Builder` runs in isolated Git worktrees, allowing multiple features to be implemented simultaneously without file locking issues.
